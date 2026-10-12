//! ICON-D2: folder listings say which hours are out; each field is a bz2-compressed file.

use std::io::Read;

use super::{Error, MapData};
use crate::grid::{self, LatLonGrid};
use crate::icon_d2;

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
            let ready: Vec<u32> = u.into_iter().filter(|s| v.contains(s)).collect();
            // Gusts come with the wind; an hour without them is drawn all the same.
            let gusts = self.listed_icon_d2(run_at, icon_d2::GUST).await?;
            if ready.is_empty() {
                // Not out yet; try the run before.
                continue;
            }
            let have = self.steps_on_disk(model, run_at).await?;
            for step in ready.into_iter().filter(|s| !have.contains(s)) {
                let u = self.fetch_icon_d2(run_at, step, icon_d2::U).await?;
                let v = self.fetch_icon_d2(run_at, step, icon_d2::V).await?;
                let gust = if gusts.contains(&step) {
                    Some(self.fetch_icon_d2(run_at, step, icon_d2::GUST).await?)
                } else {
                    None
                };
                self.write_hour(model, run_at, step, u, v, gust, true)
                    .await?;
                written += 1;
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
