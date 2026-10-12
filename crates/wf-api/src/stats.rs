//! A station's wind over ten whole years: the rose per month and the windy days, made from an
//! archive on the first request (`wf_ingest::climate`).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::header;
use axum::response::IntoResponse;
use serde::Serialize;
use ts_rs::TS;
use wf_core::climate::WindStats;
use wf_core::station;
use wf_ingest::climate::Status;

use crate::{ApiError, AppState};

/// `GET /api/stations/{slug}/stats`
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StationStats {
    /// Null while being made or when the station has no archive
    pub stats: Option<WindStats>,
    /// Being made: ask again in a little while
    pub pending: bool,
}

pub async fn stats(
    State(s): State<AppState>,
    Path(slug): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let st = station::by_slug(&s.db, &slug)
        .await?
        .ok_or(ApiError::NotFound)?;
    let id = station::id_of(&s.db, &slug)
        .await?
        .ok_or(ApiError::NotFound)?;
    let status = match &s.climate {
        Some(c) => c
            .status(id, &st, wf_core::time::now())
            .await
            .map_err(ApiError::internal)?,
        None => Status::None,
    };
    let (body, cache) = match status {
        // Made once a year: a day in caches is fine
        Status::Ready(stats) => (
            StationStats {
                stats: Some(stats),
                pending: false,
            },
            "public, max-age=86400",
        ),
        Status::Pending => (
            StationStats {
                stats: None,
                pending: true,
            },
            "no-store",
        ),
        Status::None => (
            StationStats {
                stats: None,
                pending: false,
            },
            "public, max-age=3600",
        ),
    };
    Ok(([(header::CACHE_CONTROL, cache)], Json(body)))
}
