//! ECMWF: a step is out once its index is; U, V and the gusts come by byte range from the
//! step's file, the whole world.

use reqwest::StatusCode;
use reqwest::header::RANGE;

use super::{Error, MapData};

/// Between two steps, so as not to run into ECMWF's request limit
const PAUSE: std::time::Duration = std::time::Duration::from_millis(300);
/// When the portal is busy (it allows 750 connections from everyone at once, and answers 429
/// beyond), a round waits this long before asking again, a few times, then leaves the rest
/// to the next round.
const BUSY_WAIT: std::time::Duration = std::time::Duration::from_secs(20);
const BUSY_TRIES: u32 = 3;
use crate::ecmwf;
use crate::grid::{self, LatLonGrid};

/// U, V and the gusts if the step has them
type Fields = (LatLonGrid, LatLonGrid, Option<LatLonGrid>);

fn is_busy(e: &reqwest::Error) -> bool {
    e.status() == Some(StatusCode::TOO_MANY_REQUESTS)
}

impl MapData {
    /// Fetches the steps of the newest ECMWF run that are out and not on disk yet; returns how
    /// many textures were written.
    pub async fn update_ecmwf(&self, now: i64) -> Result<usize, Error> {
        let model = ecmwf::MODEL;
        let mut written = 0;
        let mut busy = 0;
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
                let fields = loop {
                    match self.ecmwf_step(run_at, step, first.take()).await {
                        Err(Error::Http(e)) if is_busy(&e) && busy < BUSY_TRIES => {
                            busy += 1;
                            tokio::time::sleep(BUSY_WAIT).await;
                        }
                        res => break res,
                    }
                };
                let (u, v, gust) = match fields {
                    Ok(Some(f)) => f,
                    // Steps come out in order: the first one missing ends this round.
                    Ok(None) => break,
                    // Still busy: the rest comes next round.
                    Err(Error::Http(e)) if is_busy(&e) => {
                        tracing::debug!("map {model}: portal busy after {written} hours");
                        break;
                    }
                    Err(e) => return Err(e),
                };
                self.write_hour(model, run_at, step, u, v, gust).await?;
                written += 1;
                tokio::time::sleep(PAUSE).await;
            }
            break;
        }
        self.prune(model).await?;
        Ok(written)
    }

    /// A step's fields, or None while the step is not out; `index` when it was read already.
    async fn ecmwf_step(
        &self,
        run_at: i64,
        step: u32,
        index: Option<String>,
    ) -> Result<Option<Fields>, Error> {
        let index = match index {
            Some(i) => i,
            None => match self.ecmwf_index(run_at, step).await? {
                Some(i) => i,
                None => return Ok(None),
            },
        };
        self.ecmwf_fields(run_at, step, &index).await.map(Some)
    }

    /// U, V and, where the step has them, the gusts.
    async fn ecmwf_fields(&self, run_at: i64, step: u32, index: &str) -> Result<Fields, Error> {
        let u = self.fetch_ecmwf(run_at, step, index, ecmwf::U).await?;
        let v = self.fetch_ecmwf(run_at, step, index, ecmwf::V).await?;
        let gust = match ecmwf::field_range(index, ecmwf::GUST) {
            Some(_) => Some(self.fetch_ecmwf(run_at, step, index, ecmwf::GUST).await?),
            None => None,
        };
        Ok((u, v, gust))
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
            grid::decode(&bytes).map_err(|source| Error::Grid {
                file: format!("{url} {param}"),
                source,
            })
        })
        .await?
    }
}
