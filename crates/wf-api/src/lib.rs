//! The HTTP side of Windfenster: JSON API under `/api`, station pages with meta tags,
//! everything else is the SPA.

mod error;
pub mod forecast;
pub mod history;
pub mod live;
pub mod map;
mod meta;
pub mod ranking;
pub mod search;
mod spa;
pub mod water;

use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use sqlx::SqlitePool;
use tower_http::CompressionLevel;
use tower_http::compression::predicate::{NotForContentType, Predicate};
use tower_http::compression::{CompressionLayer, DefaultPredicate};
use ts_rs::TS;
use wf_core::geo::GridPoint;
use wf_core::model::Model;
use wf_ingest::forecasts::Forecasts;
use wf_ingest::geocode::Geocoder;
use wf_ingest::measurements::Measurements;
use wf_map::MapData;

pub use error::{ApiError, ErrorCode};

/// Version of the running binary, as shown in the UI.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub forecasts: Arc<Forecasts>,
    /// Place search; without it the search finds stations only.
    pub geocoder: Option<Arc<Geocoder>>,
    /// Fetches a station's measurements on request; without it only stored ones are served.
    pub measurements: Option<Arc<Measurements>>,
    /// The wind map's textures; without them the map has no layers.
    pub map: Option<Arc<MapData>>,
    /// Public address, e.g. https://windfenster.example, for canonical links.
    pub public_url: Option<String>,
    /// Vite dev server to redirect to while no SPA is built in (`pnpm dev`).
    pub dev_ui: Option<String>,
    cache: forecast::ForecastCache,
    places: search::PlaceCache,
    /// Places whose request time was written within the last hour.
    touched: moka::future::Cache<GridPoint, ()>,
    /// Stations (row ids) whose measurements were fetched on request within ten minutes.
    fetched: moka::future::Cache<i64, ()>,
    gauge_history: water::HistoryCache,
    rankings: ranking::RankingCache,
    pins: map::PinCache,
    /// Stations (row ids) whose past days were filled in within six hours.
    backfilled: moka::future::Cache<i64, ()>,
}

impl AppState {
    pub fn new(
        db: SqlitePool,
        forecasts: Arc<Forecasts>,
        geocoder: Option<Arc<Geocoder>>,
        measurements: Option<Arc<Measurements>>,
        public_url: Option<String>,
        dev_ui: Option<String>,
    ) -> Self {
        let cache = forecast::new_cache();
        forecast::spawn_invalidation(&forecasts, cache.clone());
        Self {
            db,
            forecasts,
            geocoder,
            measurements,
            map: None,
            public_url,
            dev_ui,
            cache,
            places: search::new_cache(),
            touched: moka::future::Cache::builder()
                .max_capacity(50_000)
                .time_to_live(Duration::from_secs(3_600))
                .build(),
            fetched: moka::future::Cache::builder()
                .max_capacity(10_000)
                .time_to_live(Duration::from_secs(600))
                .build(),
            gauge_history: water::new_cache(),
            rankings: ranking::new_cache(),
            pins: map::new_pin_cache(),
            backfilled: moka::future::Cache::builder()
                .max_capacity(10_000)
                .time_to_live(Duration::from_secs(6 * 3_600))
                .build(),
        }
    }

    /// With the wind map's textures.
    pub fn with_map(self, map: Arc<MapData>) -> Self {
        Self {
            map: Some(map),
            ..self
        }
    }

    /// Keeps a place served from the cache "hot", so the scheduler keeps refreshing it.
    async fn touch(&self, p: GridPoint, pinned: bool) {
        if self.touched.contains_key(&p) {
            return;
        }
        let now = wf_core::time::now();
        match wf_core::forecast::touch_place(&self.db, p, pinned, now).await {
            Ok(()) => self.touched.insert(p, ()).await,
            Err(e) => tracing::warn!("touch place: {e}"),
        }
    }
}

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .route("/models", get(models))
        .route("/stations", get(search::search))
        .route("/stations/{slug}", get(forecast::station))
        .route("/stations/{slug}/live", get(live::live))
        .route("/stations/{slug}/history", get(history::history))
        .route("/point", get(forecast::point))
        .route("/gauges/near", get(water::near))
        .route("/map", get(map::index))
        .route("/map/stations", get(map::stations))
        .route("/map/series", get(map::series))
        .route("/map/basemap.pmtiles", get(map::basemap))
        .route("/map/{model}/{run}/{file}", get(map::file))
        .fallback(|| async { ApiError::NotFound });
    Router::new()
        .nest("/api", api)
        .route("/station/{slug}", get(meta::station_page))
        .fallback(spa::serve)
        .with_state(state)
        // gzip/br for the bundle and JSON; fonts (woff2) are compressed already, the basemap
        // goes out in byte ranges of compressed tiles.
        .layer(
            CompressionLayer::new()
                .quality(CompressionLevel::Precise(6))
                .compress_when(
                    DefaultPredicate::new()
                        .and(NotForContentType::const_new("font/"))
                        .and(NotForContentType::const_new(map::PMTILES)),
                ),
        )
}

/// `GET /api/health`: the server is up and the database answers.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Health {
    pub version: String,
    /// Newest applied database migration.
    #[ts(type = "number")]
    pub schema_version: i64,
    #[ts(type = "number")]
    pub stations: i64,
    #[ts(type = "number")]
    pub models: i64,
    #[ts(type = "number")]
    pub model_runs: i64,
}

async fn health(State(s): State<AppState>) -> Result<Json<Health>, ApiError> {
    let schema_version = wf_core::db::schema_version(&s.db)
        .await
        .map_err(ApiError::internal)?;
    let (stations, model_runs): (i64, i64) =
        sqlx::query_as("SELECT (SELECT COUNT(*) FROM station), (SELECT COUNT(*) FROM model_run)")
            .fetch_one(&s.db)
            .await?;
    Ok(Json(Health {
        version: VERSION.to_owned(),
        schema_version,
        stations,
        models: s.forecasts.models().len() as i64,
        model_runs,
    }))
}

/// `GET /api/models`: the active models.
async fn models(State(s): State<AppState>) -> Json<Vec<Model>> {
    Json(s.forecasts.models().to_vec())
}
