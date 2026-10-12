//! ICON global: like ICON-D2, folder listings say which hours are out, each field a
//! bz2-compressed file; the fields come on the model's triangles and go onto the regular grid
//! by a table made once from the cells' centres and kept next to the runs.

use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use super::{Error, MapData};
use crate::grid;
use crate::icon_global as files;
use crate::ready::{Ready, ready_steps};

/// How often to look for new hours (a run comes out over about two hours)
const EVERY: Duration = Duration::from_secs(600);

impl MapData {
    /// The background loop for ICON global, apart from the others: a run is about a gigabyte
    /// to fetch and must not hold up ICON-D2 and ECMWF.
    pub async fn keep_icon_global(self: Arc<Self>) {
        let mut tick = tokio::time::interval(EVERY);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            match self.update_icon_global(wf_core::time::now()).await {
                Ok(0) => {}
                Ok(n) => tracing::info!("map: {n} new {} hours", files::MODEL),
                Err(e) => tracing::warn!("map {}: {e}", files::MODEL),
            }
        }
    }

    /// Fetches the hours of the newest run that are out and not on disk yet; returns how many
    /// textures were written.
    pub async fn update_icon_global(&self, now: i64) -> Result<usize, Error> {
        let model = files::MODEL;
        let mut written = 0;
        for run_at in files::candidate_runs(now) {
            let (u, v) = (
                self.listed_icon_global(run_at, files::U).await?,
                self.listed_icon_global(run_at, files::V).await?,
            );
            if u.is_empty() || v.is_empty() {
                continue;
            }
            let gusts = self.listed_icon_global(run_at, files::GUST).await?;
            let ready = ready_steps(&u, &v, &gusts);
            if ready.is_empty() {
                continue;
            }
            let have = self.steps_on_disk(model, run_at).await?;
            let todo: Vec<Ready> = ready
                .into_iter()
                .filter(|r| !have.contains(&r.step))
                .collect();
            if !todo.is_empty() {
                let near = self.icon_global_grid(run_at).await?;
                for Ready { step, gust } in todo {
                    // One bad file costs this hour, not the run
                    let fetched = async {
                        let u = self.fetch_icon_global(run_at, step, files::U).await?;
                        let v = self.fetch_icon_global(run_at, step, files::V).await?;
                        let gust = match gust {
                            true => Some(self.fetch_icon_global(run_at, step, files::GUST).await?),
                            false => None,
                        };
                        let near = near.clone();
                        let (u, v, gust) = tokio::task::spawn_blocking(move || {
                            (
                                files::regrid(&u, &near),
                                files::regrid(&v, &near),
                                gust.map(|g| files::regrid(&g, &near)),
                            )
                        })
                        .await?;
                        // The world texture is not sent as such: the app takes pieces of it
                        // (`texture::crop`), cut from the texels.
                        self.write_hour(model, run_at, step, u, v, gust, false)
                            .await
                    };
                    match fetched.await {
                        Ok(()) => written += 1,
                        Err(e) => tracing::warn!("map {model}: hour {step} of run {run_at}: {e}"),
                    }
                }
            }
            break;
        }
        self.prune(model).await?;
        Ok(written)
    }

    fn icon_global_grid_path(&self) -> PathBuf {
        self.root.join(format!("{}-grid.u32", files::MODEL))
    }

    /// The nearest cell of every grid point: from memory, from disk, or made from the cells'
    /// centres of the run (once; about 17 MB).
    async fn icon_global_grid(&self, run_at: i64) -> Result<Arc<Vec<u32>>, Error> {
        self.icon_global_near
            .get_or_try_init(|| async {
                let path = self.icon_global_grid_path();
                let want = files::NX * files::NY;
                if let Ok(bytes) = tokio::fs::read(&path).await
                    && bytes.len() == want * 4
                {
                    let near = bytes
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|b| u32::from_le_bytes(*b))
                        .collect();
                    return Ok(Arc::new(near));
                }
                let clat = self
                    .fetch_icon_global_file(
                        run_at,
                        files::CLAT,
                        files::invariant_name(run_at, files::CLAT),
                    )
                    .await?;
                let clon = self
                    .fetch_icon_global_file(
                        run_at,
                        files::CLON,
                        files::invariant_name(run_at, files::CLON),
                    )
                    .await?;
                let near =
                    tokio::task::spawn_blocking(move || files::nearest_cells(&clat, &clon)).await?;
                let bytes: Vec<u8> = near.iter().flat_map(|c| c.to_le_bytes()).collect();
                tokio::fs::create_dir_all(&self.root).await?;
                tokio::fs::write(&path, bytes).await?;
                tracing::info!("map: made the {} grid table", files::MODEL);
                Ok(Arc::new(near))
            })
            .await
            .cloned()
    }

    async fn listed_icon_global(&self, run_at: i64, var: &str) -> Result<Vec<u32>, Error> {
        let url = files::folder_url(&self.urls.icon_global, run_at, var);
        let html = self
            .http
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        Ok(files::listed_steps(&html, run_at, var))
    }

    async fn fetch_icon_global(
        &self,
        run_at: i64,
        step: u32,
        var: &str,
    ) -> Result<Vec<f32>, Error> {
        self.fetch_icon_global_file(run_at, var, files::file_name(run_at, step, var))
            .await
    }

    /// A field's values on the triangles, in the model's order.
    async fn fetch_icon_global_file(
        &self,
        run_at: i64,
        var: &str,
        file: String,
    ) -> Result<Vec<f32>, Error> {
        let url = format!(
            "{}{file}",
            files::folder_url(&self.urls.icon_global, run_at, var)
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
            grid::decode_values(&raw).map_err(|source| Error::Grid { file, source })
        })
        .await?
    }
}
