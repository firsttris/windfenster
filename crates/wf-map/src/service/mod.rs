//! Keeps the textures current: finds the newest model run, fetches the hours not yet on disk
//! and turns them into textures; old runs are removed. One file per model.

mod basemap;
pub mod basemap_admin;
mod ecmwf;
mod icon_d2;
mod icon_global;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::grid::{GridError, LatLonGrid};
use crate::index::{self, MapIndex};
use crate::store::{self, MapRun};
use crate::texture::{self, TextureError};
use crate::{ecmwf as ecmwf_files, icon_d2 as icon_d2_files, icon_global as icon_global_files};

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
    #[error("{file}: no field {param} in the index")]
    Missing { file: String, param: &'static str },
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
    pub icon_global: String,
    pub ecmwf: String,
    /// Protomaps' daily builds, for the basemap
    pub protomaps: String,
    /// Mapterhorn's downloads, for the relief
    pub mapterhorn: String,
}

impl Default for Urls {
    fn default() -> Self {
        Self {
            icon_d2: icon_d2_files::BASE.into(),
            icon_global: icon_global_files::BASE.into(),
            ecmwf: ecmwf_files::BASE.into(),
            protomaps: crate::basemap::fetch::BUILDS.into(),
            mapterhorn: crate::basemap::terrain::DOWNLOADS.into(),
        }
    }
}

/// Runs kept per model: the newest and the one before, which stays while the newest fills.
const KEEP_RUNS: usize = 2;

/// How often to look for new hours: ICON-D2 publishes an hour about every three minutes for
/// two hours after a run starts, ECMWF its steps over about an hour.
const EVERY: Duration = Duration::from_secs(600);

pub struct MapData {
    root: PathBuf,
    http: reqwest::Client,
    urls: Urls,
    /// Open basemap archives, for the detail tiles
    readers: crate::basemap::serve::Readers,
    /// Spots tapped on the map, with when (service/basemap.rs)
    tapped: std::sync::Mutex<Vec<(crate::basemap::spots::Spot, i64)>>,
    /// How much basemap and relief to keep, ICON global or not, renew by itself or ask: the
    /// admin page's settings
    settings: Arc<wf_core::admin::settings::SettingsStore>,
    /// What the admin page said about the basemap's renewal (service/basemap.rs)
    renewal: basemap_admin::Renewal,
    /// ICON global's triangles to the regular grid, once loaded or made
    icon_global_near: tokio::sync::OnceCell<Arc<Vec<u32>>>,
}

/// The basemap's and the relief's region (west, south, east, north).
pub const REGION: [f64; 4] = [-25.0, 27.0, 45.0, 72.0];

impl MapData {
    pub fn new(root: PathBuf, http: reqwest::Client, urls: Urls) -> Self {
        Self {
            root,
            http,
            urls,
            readers: Default::default(),
            tapped: Default::default(),
            settings: wf_core::admin::settings::SettingsStore::defaults(),
            renewal: Default::default(),
            icon_global_near: Default::default(),
        }
    }

    /// With the admin page's settings.
    pub fn with_settings(self, settings: Arc<wf_core::admin::settings::SettingsStore>) -> Self {
        Self { settings, ..self }
    }

    /// How much basemap and relief to keep, as the settings say now.
    pub fn depth(&self) -> crate::basemap::Depth {
        let s = self.settings.get();
        crate::basemap::Depth {
            world_zoom: s.basemap_world_zoom,
            region: s.basemap_region,
            region_zoom: s.basemap_zoom,
            max_spots: s.basemap_spots as usize,
            terrain_zoom: s.terrain_zoom,
        }
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
            let runs = self.runs(model).await?;
            let mut layer = index::layer(model, &runs);
            layer.area = self.area(model, &runs).await;
            layers.push(layer);
        }
        Ok(MapIndex {
            layers,
            basemap: crate::basemap::info(&self.root).await,
            terrain: crate::basemap::terrain::info(&self.root).await,
        })
    }

    /// What the newest run's first texture covers.
    async fn area(&self, model: &str, runs: &[MapRun]) -> Option<[f64; 4]> {
        let run = runs.first()?;
        let step = run.steps.first()?;
        let path = store::run_dir(&self.root, model, run.run_at).join(format!("{step:03}.json"));
        let meta: texture::TextureMeta =
            serde_json::from_slice(&tokio::fs::read(path).await.ok()?).ok()?;
        Some([meta.west, meta.south, meta.east, meta.north])
    }

    /// The background loop: new hours every ten minutes.
    pub async fn keep_current(self: Arc<Self>) {
        let board = wf_core::status::board();
        board.register("map", EVERY.as_secs() as i64);
        let mut tick = tokio::time::interval(EVERY);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = tick.tick() => {}
                () = board.asked() => if !board.take("map") { continue },
            }
            let now = wf_core::time::now();
            let report = board.start("map");
            let mut written = 0;
            let mut failed = Vec::new();
            for (model, source, result) in [
                (icon_d2_files::MODEL, "dwd", self.update_icon_d2(now).await),
                (ecmwf_files::MODEL, "ecmwf", self.update_ecmwf(now).await),
            ] {
                match result {
                    Ok(n) => {
                        board.source_ok(source);
                        written += n;
                        if n > 0 {
                            tracing::info!("map: {n} new {model} hours");
                        }
                    }
                    Err(e) => {
                        tracing::warn!("map {model}: {e}");
                        board.source_failed(source, &e);
                        failed.push(format!("{model}: {e}"));
                    }
                }
            }
            let written = Some(written as i64);
            match failed.len() {
                0 => report.ok(written),
                1 => report.partly(written, failed.join("; ")),
                _ => report.failed(failed.join("; ")),
            }
        }
    }

    /// Encodes U, V and the gust of an hour and writes the texture (as PNG too, unless the app
    /// only takes pieces of it).
    #[allow(clippy::too_many_arguments)]
    async fn write_hour(
        &self,
        model: &'static str,
        run_at: i64,
        step: u32,
        u: LatLonGrid,
        v: LatLonGrid,
        gust: Option<LatLonGrid>,
        with_png: bool,
    ) -> Result<(), Error> {
        let (png, rgba, meta) = tokio::task::spawn_blocking(move || {
            let (rgba, meta) = texture::pixels(&u, &v, gust.as_ref(), model, run_at, step)?;
            let png = if with_png {
                Some(texture::png(&rgba, meta.width, meta.height)?)
            } else {
                None
            };
            Ok::<_, texture::TextureError>((png, rgba, meta))
        })
        .await??;
        Ok(store::write_step(&self.root, model, png.as_deref(), &rgba, &meta).await?)
    }

    /// Hours of a run already on disk.
    async fn steps_on_disk(&self, model: &str, run_at: i64) -> Result<Vec<u32>, Error> {
        Ok(self
            .runs(model)
            .await?
            .into_iter()
            .find(|r| r.run_at == run_at)
            .map(|r| r.steps)
            .unwrap_or_default())
    }

    /// Keeps the newest runs of a model.
    async fn prune(&self, model: &str) -> Result<(), Error> {
        let pruned = store::prune(&self.root, model, KEEP_RUNS).await?;
        if pruned > 0 {
            tracing::info!("removed {pruned} old {model} map runs");
        }
        Ok(())
    }
}
