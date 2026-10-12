//! ICON-D2: folder listings say which hours are out; each field is a bz2-compressed file.

use std::io::Read;

use super::{Error, MapData};
use crate::grid::{self, LatLonGrid};
use crate::icon_d2;
use crate::ready::{Ready, ready_steps};

impl MapData {
    /// Fetches the hours of the newest ICON-D2 run that are out and not on disk yet; returns how
    /// many textures were written.
    pub async fn update_icon_d2(&self, now: i64) -> Result<usize, Error> {
        let model = icon_d2::MODEL;
        let mut written = 0;
        for run_at in icon_d2::candidate_runs(now) {
            let (u, v) = (
                self.listed_icon_d2(run_at, icon_d2::U).await?,
                self.listed_icon_d2(run_at, icon_d2::V).await?,
            );
            if u.is_empty() || v.is_empty() {
                // Not out yet; try the run before.
                continue;
            }
            let gusts = self.listed_icon_d2(run_at, icon_d2::GUST).await?;
            let ready = ready_steps(&u, &v, &gusts);
            if ready.is_empty() {
                continue;
            }
            let have = self.steps_on_disk(model, run_at).await?;
            for Ready { step, gust } in ready.into_iter().filter(|r| !have.contains(&r.step)) {
                // One bad file (a 404, a truncated archive) costs this hour, not the run
                let fetched = async {
                    let u = self.fetch_icon_d2(run_at, step, icon_d2::U).await?;
                    let v = self.fetch_icon_d2(run_at, step, icon_d2::V).await?;
                    let gust = match gust {
                        true => Some(self.fetch_icon_d2(run_at, step, icon_d2::GUST).await?),
                        false => None,
                    };
                    self.write_hour(model, run_at, step, u, v, gust, true).await
                };
                match fetched.await {
                    Ok(()) => written += 1,
                    Err(e) => tracing::warn!("map {model}: hour {step} of run {run_at}: {e}"),
                }
            }
            break;
        }
        self.prune(model).await?;
        Ok(written)
    }

    async fn listed_icon_d2(&self, run_at: i64, var: &str) -> Result<Vec<u32>, Error> {
        let url = icon_d2::folder_url(&self.urls.icon_d2, run_at, var);
        let html = self
            .http
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        Ok(icon_d2::listed_steps(&html, run_at, var))
    }

    async fn fetch_icon_d2(&self, run_at: i64, step: u32, var: &str) -> Result<LatLonGrid, Error> {
        let file = icon_d2::file_name(run_at, step, var);
        let url = format!(
            "{}{file}",
            icon_d2::folder_url(&self.urls.icon_d2, run_at, var)
        );
        let bz2 = self
            .http
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        tokio::task::spawn_blocking(move || {
            let mut raw = Vec::new();
            bzip2::read::BzDecoder::new(&bz2[..])
                .read_to_end(&mut raw)
                .map_err(|source| Error::Unpack {
                    file: file.clone(),
                    source,
                })?;
            grid::decode(&raw).map_err(|source| Error::Grid { file, source })
        })
        .await?
    }
}
