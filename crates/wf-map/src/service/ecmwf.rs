//! ECMWF: a step is out once its index is; U, V and the gusts come by byte range from the
//! step's file, the whole world. When data.ecmwf.int is busy (429), the round goes on from
//! ECMWF's mirror.

use reqwest::StatusCode;
use reqwest::header::RANGE;

use super::{Error, MapData};

/// Between two steps, so as not to run into ECMWF's request limit
const PAUSE: std::time::Duration = std::time::Duration::from_millis(300);
use crate::ecmwf;
use crate::grid::{self, LatLonGrid};

/// U, V and the gusts if the step has them
type Fields = (LatLonGrid, LatLonGrid, Option<LatLonGrid>);

/// Busy: the portal allows 750 connections from everyone at once and answers 429 beyond, for
/// hours from some addresses; a cloud bucket answers 503 SlowDown.
fn is_busy(e: &Error) -> bool {
    matches!(e, Error::Http(e) if matches!(
        e.status(),
        Some(StatusCode::TOO_MANY_REQUESTS | StatusCode::SERVICE_UNAVAILABLE)
    ))
}

impl MapData {
    /// Fetches the steps of the newest ECMWF run that are out and not on disk yet, from
    /// data.ecmwf.int or, when it is busy, from the mirror; returns how many textures were
    /// written.
    pub async fn update_ecmwf(&self, now: i64) -> Result<usize, Error> {
        let mut written = 0;
        let round = self.ecmwf_round(&self.urls.ecmwf, now, &mut written).await;
        let round = match round {
            Err(e) if is_busy(&e) => {
                tracing::debug!(
                    "map {}: data.ecmwf.int busy after {written} hours, on from the mirror",
                    ecmwf::MODEL
                );
                self.ecmwf_round(&self.urls.ecmwf_mirror, now, &mut written)
                    .await
            }
            r => r,
        };
        // The old runs are pruned whatever the round brought.
        self.prune(ecmwf::MODEL).await?;
        round.map(|()| written)
    }

    /// One round from `base`: the newest run out, its steps not on disk; ends early with the
    /// error when `base` is busy.
    async fn ecmwf_round(&self, base: &str, now: i64, written: &mut usize) -> Result<(), Error> {
        let model = ecmwf::MODEL;
        for run_at in ecmwf::candidate_runs(now) {
            let have = self.steps_on_disk(model, run_at).await?;
            // A run is out once its first step is.
            let mut first = None;
            if !have.contains(&0) {
                match self.ecmwf_index(base, run_at, 0).await? {
                    Some(index) => first = Some(index),
                    None => continue,
                }
            }
            for step in ecmwf::steps().filter(|s| !have.contains(s)) {
                let (u, v, gust) = match self.ecmwf_step(base, run_at, step, first.take()).await {
                    Ok(Some(f)) => f,
                    // Steps come out in order: the first one missing ends this round.
                    Ok(None) => break,
                    Err(e) if is_busy(&e) => return Err(e),
                    // A bad step ends this round (steps come in order); the run is tried again
                    // next round
                    Err(e) => {
                        tracing::warn!("map {model}: hour {step} of run {run_at}: {e}");
                        break;
                    }
                };
                self.write_hour(model, run_at, step, u, v, gust, true)
                    .await?;
                *written += 1;
                tokio::time::sleep(PAUSE).await;
            }
            break;
        }
        Ok(())
    }

    /// A step's fields, or None while the step is not out; `index` when it was read already.
    async fn ecmwf_step(
        &self,
        base: &str,
        run_at: i64,
        step: u32,
        index: Option<String>,
    ) -> Result<Option<Fields>, Error> {
        let index = match index {
            Some(i) => i,
            None => match self.ecmwf_index(base, run_at, step).await? {
                Some(i) => i,
                None => return Ok(None),
            },
        };
        self.ecmwf_fields(base, run_at, step, &index)
            .await
            .map(Some)
    }

    /// U, V and, where the step has them, the gusts.
    async fn ecmwf_fields(
        &self,
        base: &str,
        run_at: i64,
        step: u32,
        index: &str,
    ) -> Result<Fields, Error> {
        let u = self
            .fetch_ecmwf(base, run_at, step, index, ecmwf::U)
            .await?;
        let v = self
            .fetch_ecmwf(base, run_at, step, index, ecmwf::V)
            .await?;
        let gust = match ecmwf::field_range(index, ecmwf::GUST) {
            Some(_) => Some(
                self.fetch_ecmwf(base, run_at, step, index, ecmwf::GUST)
                    .await?,
            ),
            None => None,
        };
        Ok((u, v, gust))
    }

    /// A step's index, or None while the step is not out.
    async fn ecmwf_index(
        &self,
        base: &str,
        run_at: i64,
        step: u32,
    ) -> Result<Option<String>, Error> {
        let url = ecmwf::file_url(base, run_at, step, "index");
        let res = self.http.get(url).send().await?;
        if res.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        Ok(Some(res.error_for_status()?.text().await?))
    }

    async fn fetch_ecmwf(
        &self,
        base: &str,
        run_at: i64,
        step: u32,
        index: &str,
        param: &'static str,
    ) -> Result<LatLonGrid, Error> {
        let url = ecmwf::file_url(base, run_at, step, "grib2");
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
