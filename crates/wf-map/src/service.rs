//! Keeps the textures current: finds the newest model run, fetches the hours not yet on disk
//! and turns them into textures; old runs are removed.

use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::grid::{self, GridError, LatLonGrid};
use crate::icon_d2;
use crate::index::{self, MapIndex};
use crate::store::{self, MapRun};
use crate::texture::{self, TextureError};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("unpacking {file}: {source}")]
    Unpack {
        file: String,
        source: std::io::Error,
    },
    #[error("{file}: {source}")]
    Grid { file: String, source: GridError },
    #[error("texture: {0}")]
    Texture(#[from] TextureError),
    #[error("writing textures: {0}")]
    Io(#[from] std::io::Error),
    #[error("decoding thread: {0}")]
    Join(#[from] tokio::task::JoinError),
}

#[derive(Debug, Clone)]
pub struct Urls {
    pub icon_d2: String,
}

impl Default for Urls {
    fn default() -> Self {
        Self {
            icon_d2: icon_d2::BASE.into(),
        }
    }
}

/// Runs kept per model: the newest and the one before, which stays while the newest fills.
const KEEP_RUNS: usize = 2;

/// How often to look for new hours: ICON-D2 publishes an hour about every three minutes for
/// two hours after a run starts.
const EVERY: Duration = Duration::from_secs(600);

pub struct MapData {
    root: PathBuf,
    http: reqwest::Client,
    urls: Urls,
}

impl MapData {
    pub fn new(root: PathBuf, http: reqwest::Client, urls: Urls) -> Self {
        Self { root, http, urls }
    }

    pub fn root(&self) -> &std::path::Path {
        &self.root
    }

    /// The runs on disk of a model, newest first.
    pub async fn runs(&self, model: &str) -> std::io::Result<Vec<MapRun>> {
        store::runs(&self.root, model).await
    }

    /// The hours of every model on disk.
    pub async fn index(&self) -> std::io::Result<MapIndex> {
        let mut layers = Vec::new();
        for model in crate::MODELS {
            layers.push(index::layer(model, &self.runs(model).await?));
        }
        Ok(MapIndex { layers })
    }

    /// The background loop: new hours every ten minutes.
    pub async fn keep_current(self: Arc<Self>) {
        let mut tick = tokio::time::interval(EVERY);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            match self.update_icon_d2(wf_core::time::now()).await {
                Ok(0) => {}
                Ok(n) => tracing::info!("map: {n} new {} hours", icon_d2::MODEL),
                Err(e) => tracing::warn!("map {}: {e}", icon_d2::MODEL),
            }
        }
    }

    /// Fetches the hours of the newest ICON-D2 run that are out and not on disk yet; returns how
    /// many textures were written.
    pub async fn update_icon_d2(&self, now: i64) -> Result<usize, Error> {
        let model = icon_d2::MODEL;
        let on_disk = self.runs(model).await?;
        let mut written = 0;
        for run_at in icon_d2::candidate_runs(now) {
            let listed = |var| async move {
                let url = icon_d2::folder_url(&self.urls.icon_d2, run_at, var);
                let html = self
                    .http
                    .get(url)
                    .send()
                    .await?
                    .error_for_status()?
                    .text()
                    .await?;
                Ok::<_, Error>(icon_d2::listed_steps(&html, run_at, var))
            };
            let (u, v) = (listed(icon_d2::U).await?, listed(icon_d2::V).await?);
            let ready: Vec<u32> = u.into_iter().filter(|s| v.contains(s)).collect();
            if ready.is_empty() {
                // Not out yet; try the run before.
                continue;
            }
            let have: Vec<u32> = on_disk
                .iter()
                .find(|r| r.run_at == run_at)
                .map(|r| r.steps.clone())
                .unwrap_or_default();
            for step in ready.into_iter().filter(|s| !have.contains(s)) {
                let u = self.fetch_icon_d2(run_at, step, icon_d2::U).await?;
                let v = self.fetch_icon_d2(run_at, step, icon_d2::V).await?;
                let (png, meta) = tokio::task::spawn_blocking(move || {
                    texture::encode(&u, &v, icon_d2::MODEL, run_at, step)
                })
                .await??;
                store::write_step(&self.root, model, &png, &meta).await?;
                written += 1;
            }
            break;
        }
        let pruned = store::prune(&self.root, model, KEEP_RUNS).await?;
        if pruned > 0 {
            tracing::info!("removed {pruned} old {model} map runs");
        }
        Ok(written)
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
