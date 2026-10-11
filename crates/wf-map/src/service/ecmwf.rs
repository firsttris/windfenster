//! ECMWF: a step is out once its index is; U and V come by byte range from the step's file
//! and are cut to the map's region.

use reqwest::StatusCode;
use reqwest::header::RANGE;

use super::{Error, MapData, REGION};

/// Between two steps, so as not to run into ECMWF's request limit
const PAUSE: std::time::Duration = std::time::Duration::from_millis(300);
use crate::ecmwf;
use crate::grid::{self, LatLonGrid};

impl MapData {
    /// Fetches the steps of the newest ECMWF run that are out and not on disk yet; returns how
    /// many textures were written.
    pub async fn update_ecmwf(&self, now: i64) -> Result<usize, Error> {
        let model = ecmwf::MODEL;
        let mut written = 0;
        for run_at in ecmwf::candidate_runs(now) {
            let have = self.steps_on_disk(model, run_at).await?;
            // A run is out once its first step is.
            let mut first = None;
            if !have.contains(&0) {
                match self.ecmwf_index(run_at, 0).await? {
                    Some(index) => first = Some(index),
                    None => continue,
                }
            }
            for step in ecmwf::steps().filter(|s| !have.contains(s)) {
                let index = match first.take() {
                    Some(index) => Some(index),
                    None => self.ecmwf_index(run_at, step).await?,
                };
                // Steps come out in order: the first one missing ends this round.
                let Some(index) = index else { break };
                let fields = async {
                    let u = self.fetch_ecmwf(run_at, step, &index, ecmwf::U).await?;
                    let v = self.fetch_ecmwf(run_at, step, &index, ecmwf::V).await?;
                    Ok::<_, Error>((u, v))
                };
                let (u, v) = match fields.await {
                    Ok(f) => f,
                    // ECMWF limits how fast one may ask: the rest comes next round.
                    Err(Error::Http(e)) if e.status() == Some(StatusCode::TOO_MANY_REQUESTS) => {
                        tracing::debug!("map {model}: rate limited after {written} hours");
                        break;
                    }
                    Err(e) => return Err(e),
                };
                self.write_hour(model, run_at, step, u, v).await?;
                written += 1;
                tokio::time::sleep(PAUSE).await;
            }
            break;
        }
        self.prune(model).await?;
        Ok(written)
    }

    /// A step's index, or None while the step is not out.
    async fn ecmwf_index(&self, run_at: i64, step: u32) -> Result<Option<String>, Error> {
        let url = ecmwf::file_url(&self.urls.ecmwf, run_at, step, "index");
        let res = self.http.get(url).send().await?;
        if res.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        Ok(Some(res.error_for_status()?.text().await?))
    }

    async fn fetch_ecmwf(
        &self,
        run_at: i64,
        step: u32,
        index: &str,
        param: &'static str,
    ) -> Result<LatLonGrid, Error> {
        let url = ecmwf::file_url(&self.urls.ecmwf, run_at, step, "grib2");
        let range = ecmwf::field_range(index, param).ok_or_else(|| Error::Missing {
            file: url.clone(),
            param,
        })?;
        let bytes = self
            .http
            .get(&url)
            .header(RANGE, range.header())
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        tokio::task::spawn_blocking(move || {
            let g = grid::decode(&bytes).map_err(|source| Error::Grid {
                file: format!("{url} {param}"),
                source,
            })?;
            let [west, south, east, north] = REGION;
            g.crop(west, south, east, north)
                .ok_or(Error::Region { file: url })
        })
        .await?
    }
}
