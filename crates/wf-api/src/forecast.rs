//! Station and point forecasts, held in memory until a new run arrives for the place.

use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, Query, State};
use moka::future::Cache;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use wf_core::forecast::Forecast;
use wf_core::geo::GridPoint;
use wf_core::station::{self, Station};
use wf_ingest::forecasts::Forecasts;

use crate::client::ClientIp;
use crate::ranking::{RANKING_MAX_KM, Ranking, with_mix};
use crate::{ApiError, AppState};

/// Free points take over a station's ranking only this close (Phase 2); farther stations are
/// still named, up to this distance.
const NEAREST_MAX_KM: f64 = 50.0;

/// Forecasts per grid point. An entry is dropped when the scheduler stores new series for
/// the place; the time limit only bounds memory for places nobody refreshes.
pub type ForecastCache = Cache<GridPoint, Arc<Forecast>>;

pub fn new_cache() -> ForecastCache {
    Cache::builder()
        .max_capacity(20_000)
        .time_to_live(Duration::from_secs(6 * 3_600))
        .build()
}

/// Drops cached forecasts whenever the scheduler updates a place.
pub fn spawn_invalidation(forecasts: &Forecasts, cache: ForecastCache) {
    let mut updates = forecasts.subscribe();
    tokio::spawn(async move {
        loop {
            match updates.recv().await {
                Ok(p) => cache.invalidate(&p).await,
                // Missed updates: start over rather than serve something stale.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => cache.invalidate_all(),
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

/// Loads a place's forecast once for concurrent requests. A forecast with a notice (source
/// down) is returned but not cached, so the next request tries again.
async fn forecast_for(
    s: &AppState,
    p: GridPoint,
    pinned: bool,
    ip: ClientIp,
) -> Result<Arc<Forecast>, ApiError> {
    if let Some(hit) = s.cache.get(&p).await {
        s.touch(p, pinned).await;
        return Ok(hit);
    }
    // Not served since the last run: loading it may cost calls to Open-Meteo.
    if !s.limits.new_place(ip, wf_core::time::now()).await {
        return Err(ApiError::TooManyPlaces);
    }
    let forecasts = s.forecasts.clone();
    let loaded = s
        .cache
        .try_get_with(p, async move {
            let failed = |e: anyhow::Error| Partial::Failed(format!("{e:#}"));
            let notice = forecasts.ensure(p, pinned).await.map_err(failed)?;
            let fc = forecasts.load(p, notice).await.map_err(failed)?;
            match notice {
                None => Ok(Arc::new(fc)),
                Some(_) => Err(Partial::Forecast(Box::new(fc))),
            }
        })
        .await;
    match loaded {
        Ok(fc) => Ok(fc),
        Err(e) => match e.as_ref() {
            Partial::Forecast(fc) => Ok(Arc::new((**fc).clone())),
            Partial::Failed(msg) => Err(ApiError::Internal(msg.clone())),
        },
    }
}

enum Partial {
    Forecast(Box<Forecast>),
    Failed(String),
}

/// `GET /api/stations/{slug}`
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StationForecast {
    pub station: Station,
    /// With the Windfenster mix in front, weighted by the station's scores
    pub forecast: Forecast,
    pub ranking: Ranking,
}

pub async fn station(
    State(s): State<AppState>,
    ip: ClientIp,
    Path(slug): Path<String>,
) -> Result<Json<StationForecast>, ApiError> {
    let station = station::by_slug(&s.db, &slug)
        .await?
        .ok_or(ApiError::NotFound)?;
    let id = station::id_of(&s.db, &slug)
        .await?
        .ok_or(ApiError::NotFound)?;
    let p = GridPoint::new(station.lat, station.lon).ok_or(ApiError::NotFound)?;
    let forecast = forecast_for(&s, p, true, ip).await?;
    let verified = s.verified(id, &forecast.models).await?;
    Ok(Json(StationForecast {
        station,
        forecast: with_mix(&forecast, &verified.weights, wf_core::time::now()),
        ranking: verified.ranking.clone(),
    }))
}

#[derive(Deserialize)]
pub struct PointQuery {
    pub lat: f64,
    pub lon: f64,
}

/// The station nearest to a free point.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NearbyStation {
    pub station: Station,
    pub distance_km: f64,
}

/// `GET /api/point?lat=&lon=`
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PointForecast {
    /// With the Windfenster mix in front, weighted by the ranking if there is one
    pub forecast: Forecast,
    /// The nearest station within 50 km.
    pub nearest: Option<NearbyStation>,
    /// The nearest station's ranking, when it is at most 5 km away
    pub ranking: Option<Ranking>,
}

pub async fn point(
    State(s): State<AppState>,
    ip: ClientIp,
    query: Result<Query<PointQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<PointForecast>, ApiError> {
    let Query(q) = query.map_err(|_| ApiError::InvalidCoordinates)?;
    let p = GridPoint::new(q.lat, q.lon).ok_or(ApiError::InvalidCoordinates)?;
    let forecast = forecast_for(&s, p, false, ip).await?;
    let nearest = station::nearest(&s.db, q.lat, q.lon, NEAREST_MAX_KM)
        .await?
        .map(|(station, d)| NearbyStation {
            station,
            distance_km: (d * 10.0).round() / 10.0,
        });
    let verified = match &nearest {
        Some(n) if n.distance_km <= RANKING_MAX_KM => {
            match station::id_of(&s.db, &n.station.slug).await? {
                Some(id) => Some(s.verified(id, &forecast.models).await?),
                None => None,
            }
        }
        _ => None,
    };
    let weights = verified
        .as_ref()
        .map(|v| v.weights.clone())
        .unwrap_or_default();
    Ok(Json(PointForecast {
        forecast: with_mix(&forecast, &weights, wf_core::time::now()),
        nearest,
        ranking: verified.map(|v| v.ranking.clone()),
    }))
}
