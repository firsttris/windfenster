//! The wind map's data: which hours there are, the textures themselves, and the stations'
//! newest measurements for the pins.

use std::sync::Arc;

use axum::Json;
use axum::body::Body;
use axum::extract::{Path, Query, Request, State};
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use tower::ServiceExt;
use tower_http::services::ServeFile;
use wf_core::geo::GridPoint;
use wf_core::observation::{self, StationWind};
use wf_map::MapIndex;
use wf_map::series::MapSeries;

use crate::forecast::PointQuery;
use crate::{ApiError, AppState};

/// `GET /api/map`: the hours of every model on the map; empty without map data.
pub async fn index(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let index = match &s.map {
        Some(map) => map.index().await.map_err(ApiError::internal)?,
        None => MapIndex {
            layers: vec![],
            basemap: None,
        },
    };
    // New hours arrive every few minutes.
    Ok(([(header::CACHE_CONTROL, "public, max-age=60")], Json(index)))
}

/// `GET /api/map/series?lat=&lon=`: the place over every hour of every model on the map, read
/// from the textures (no Open-Meteo call); empty without map data.
pub async fn series(
    State(s): State<AppState>,
    query: Result<Query<PointQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<impl IntoResponse, ApiError> {
    let Query(q) = query.map_err(|_| ApiError::InvalidCoordinates)?;
    GridPoint::new(q.lat, q.lon).ok_or(ApiError::InvalidCoordinates)?;
    let series = match &s.map {
        Some(map) => {
            let index = map.index().await.map_err(ApiError::internal)?;
            wf_map::series::series(map.root(), &index, q.lat, q.lon).await
        }
        None => MapSeries { layers: vec![] },
    };
    Ok((
        [(header::CACHE_CONTROL, "public, max-age=60")],
        Json(series),
    ))
}

/// The pins, built at most once a minute.
pub type PinCache = moka::future::Cache<(), Arc<Vec<StationWind>>>;

pub fn new_pin_cache() -> PinCache {
    moka::future::Cache::builder()
        .max_capacity(1)
        .time_to_live(std::time::Duration::from_secs(60))
        .build()
}

/// A pin shows a measurement up to this old; METARs come every 30 or 60 minutes.
const PIN_MAX_AGE_S: i64 = 2 * 3_600;

/// `GET /api/map/stations`: every station with its newest measurement of the last two hours.
pub async fn stations(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let pins = s
        .pins
        .try_get_with((), async {
            let since = wf_core::time::now() - PIN_MAX_AGE_S;
            observation::newest_per_station(&s.db, since)
                .await
                .map(Arc::new)
        })
        .await
        .map_err(ApiError::internal)?;
    Ok((
        [(header::CACHE_CONTROL, "public, max-age=60")],
        Json(pins.as_ref().clone()),
    ))
}

/// The basemap's media type; responses of it are byte ranges and are never compressed.
pub const PMTILES: &str = "application/vnd.pmtiles";

/// `GET /api/map/basemap.pmtiles`: the detailed basemap, in byte ranges as MapLibre's PMTiles
/// protocol asks for them; 404 without one.
pub async fn basemap(State(s): State<AppState>, req: Request) -> Result<Response, ApiError> {
    let map = s.map.as_ref().ok_or(ApiError::NotFound)?;
    let path = wf_map::basemap::path(map.root());
    if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
        return Err(ApiError::NotFound);
    }
    let mut res = ServeFile::new(path)
        .oneshot(req)
        .await
        .map_err(ApiError::internal)?
        .map(Body::new);
    let headers = res.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(PMTILES));
    // The operator may swap the file; the ETag tells MapLibre when.
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=3600"),
    );
    Ok(res)
}

/// Whether `{model}/{run}/{file}` names a texture or its description: a known model, a run
/// as YYYYMMDDHH, an hour as three digits. Nothing else reaches the file system.
fn valid(model: &str, run: &str, file: &str) -> Option<&'static str> {
    let digits = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_digit());
    if !wf_map::MODELS.contains(&model) || !digits(run, 10) {
        return None;
    }
    let (step, ext) = file.split_once('.')?;
    if !digits(step, 3) {
        return None;
    }
    match ext {
        "png" => Some("image/png"),
        "json" => Some("application/json"),
        _ => None,
    }
}

/// `GET /api/map/{model}/{run}/{step}.png|json`: a texture never changes once written.
pub async fn file(
    State(s): State<AppState>,
    Path((model, run, file)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let map = s.map.as_ref().ok_or(ApiError::NotFound)?;
    let mime = valid(&model, &run, &file).ok_or(ApiError::NotFound)?;
    let path = map.root().join(&model).join(&run).join(&file);
    let bytes = match tokio::fs::read(&path).await {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(ApiError::NotFound),
        Err(e) => return Err(ApiError::internal(e)),
    };
    Ok((
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        bytes,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_texture_paths_pass() {
        assert_eq!(valid("icon_d2", "2026101106", "003.png"), Some("image/png"));
        assert_eq!(
            valid("icon_d2", "2026101106", "048.json"),
            Some("application/json")
        );
        for (m, r, f) in [
            ("other", "2026101106", "003.png"),
            ("icon_d2", "..", "003.png"),
            ("icon_d2", "2026101106", "../x.png"),
            ("icon_d2", "2026101106", "003.txt"),
            ("icon_d2", "2026101106", "3.png"),
            ("icon_d2", "20261011", "003.png"),
        ] {
            assert_eq!(valid(m, r, f), None, "{m}/{r}/{f}");
        }
    }
}
