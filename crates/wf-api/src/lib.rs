//! The HTTP side of Windfenster: JSON API under `/api`, everything else is the SPA.

mod error;
mod spa;

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use sqlx::SqlitePool;
use tower_http::CompressionLevel;
use tower_http::compression::predicate::{NotForContentType, Predicate};
use tower_http::compression::{CompressionLayer, DefaultPredicate};
use ts_rs::TS;
use wf_core::model::{self, Model};

pub use error::ApiError;

/// Version of the running binary, as shown in the UI.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    /// Vite dev server to redirect to while no SPA is built in (`pnpm dev`).
    pub dev_ui: Option<String>,
}

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .route("/models", get(models))
        .fallback(|| async { ApiError::NotFound });
    Router::new()
        .nest("/api", api)
        .fallback(spa::serve)
        .with_state(state)
        // gzip/br for the bundle and JSON; fonts (woff2) are compressed already.
        .layer(
            CompressionLayer::new()
                .quality(CompressionLevel::Precise(6))
                .compress_when(DefaultPredicate::new().and(NotForContentType::const_new("font/"))),
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
    let (stations, models, model_runs): (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM station), (SELECT COUNT(*) FROM model), \
         (SELECT COUNT(*) FROM model_run)",
    )
    .fetch_one(&s.db)
    .await?;
    Ok(Json(Health {
        version: VERSION.to_owned(),
        schema_version,
        stations,
        models,
        model_runs,
    }))
}

/// `GET /api/models`
async fn models(State(s): State<AppState>) -> Result<Json<Vec<Model>>, ApiError> {
    Ok(Json(model::list(&s.db).await?))
}
