//! The water gauge near a place: level with its last week, flood state, flow, water
//! temperature and the estimated current.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::header;
use axum::response::IntoResponse;
use serde::Serialize;
use ts_rs::TS;
use wf_core::gauge::{self, GaugeState};

use crate::forecast::PointQuery;
use crate::{ApiError, AppState};

/// A gauge counts for a place up to this far away.
pub const NEAR_KM: f64 = 15.0;

/// History kept per gauge in memory; PEGELONLINE publishes every 15 minutes.
pub type HistoryCache = moka::future::Cache<String, Arc<Vec<LevelPoint>>>;

pub fn new_cache() -> HistoryCache {
    moka::future::Cache::builder()
        .max_capacity(2_000)
        .time_to_live(std::time::Duration::from_secs(900))
        .build()
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct Reading {
    pub value: f64,
    #[ts(type = "number")]
    pub at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct LevelPoint {
    #[ts(type = "number")]
    pub at: i64,
    pub cm: f32,
}

/// `GET /api/gauges/near?lat=&lon=`: the nearest gauge within 15 km, or null.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NearGauge {
    pub name: String,
    pub water: Option<String>,
    /// River kilometre
    pub km: Option<f64>,
    pub distance_km: f64,
    /// Water level in cm against the gauge's zero
    pub level: Option<Reading>,
    /// Against mean low and mean high water: high is above MHW (flood)
    pub level_state: GaugeState,
    /// Against the navigable range: high is above HSW, where shipping stops
    pub shipping_state: GaugeState,
    /// m³/s
    pub flow: Option<Reading>,
    /// °C
    pub water_temp: Option<Reading>,
    /// Estimated surface current in m/s, only where a cross-section is known
    pub current_ms: Option<f64>,
    /// The level over the last seven days, one value per hour, oldest first
    pub history: Vec<LevelPoint>,
}

/// One value per full hour, and the newest.
fn hourly(points: &[(i64, f32)]) -> Vec<LevelPoint> {
    let mut out: Vec<LevelPoint> = points
        .iter()
        .filter(|(at, _)| at % 3_600 == 0)
        .map(|&(at, cm)| LevelPoint { at, cm })
        .collect();
    if let Some(&(at, cm)) = points.last()
        && out.last().is_none_or(|p| p.at != at)
    {
        out.push(LevelPoint { at, cm });
    }
    out
}

pub async fn near(
    State(s): State<AppState>,
    query: Result<Query<PointQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<impl IntoResponse, ApiError> {
    let Query(q) = query.map_err(|_| ApiError::InvalidCoordinates)?;
    if !(-90.0..=90.0).contains(&q.lat) || !(-180.0..=180.0).contains(&q.lon) {
        return Err(ApiError::InvalidCoordinates);
    }
    let near = match gauge::nearest(&s.db, q.lat, q.lon, NEAR_KM).await? {
        None => None,
        Some((g, d)) => {
            let history = match &s.measurements {
                Some(m) => {
                    let uuid = g.uuid.clone();
                    s.gauge_history
                        .get_with(g.uuid.clone(), async move {
                            match m.gauge_history(&uuid).await {
                                Ok(points) => Arc::new(hourly(&points)),
                                Err(e) => {
                                    tracing::warn!("gauge history {uuid}: {e:#}");
                                    Arc::new(Vec::new())
                                }
                            }
                        })
                        .await
                }
                None => Arc::new(Vec::new()),
            };
            let reading = |value: Option<f64>, at: Option<i64>| {
                Some(Reading {
                    value: value?,
                    at: at?,
                })
            };
            Some(NearGauge {
                current_ms: g
                    .flow_m3s
                    .and_then(|q| gauge::estimated_current_ms(g.water.as_deref(), q)),
                level: reading(g.level_cm, g.level_at),
                level_state: GaugeState::parse(g.level_state.as_deref()),
                shipping_state: GaugeState::parse(g.shipping_state.as_deref()),
                flow: reading(g.flow_m3s, g.flow_at),
                water_temp: reading(g.water_temp_c, g.water_temp_at),
                distance_km: (d * 10.0).round() / 10.0,
                history: (*history).clone(),
                name: g.name,
                water: g.water,
                km: g.km,
            })
        }
    };
    Ok(([(header::CACHE_CONTROL, "public, max-age=300")], Json(near)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_full_hours_and_the_newest() {
        let points = [(3_600, 60.0), (4_500, 61.0), (7_200, 62.0), (8_100, 63.0)];
        let got: Vec<i64> = hourly(&points).iter().map(|p| p.at).collect();
        assert_eq!(got, [3_600, 7_200, 8_100]);
        assert!(hourly(&[]).is_empty());
    }
}
