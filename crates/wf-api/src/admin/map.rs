//! The map on the admin page: basemap and relief (what is there, whether due, the job under way),
//! renewing now, postponing, cancelling, the detail spots, the wind map's runs.

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use wf_map::service::basemap_admin::BasemapState;
use wf_map::store::MapRun;

use super::Admin;
use crate::{ApiError, AppState};

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AdminMap {
    pub basemap: BasemapState,
    /// The wind map's runs on disk per model (model id, runs newest first)
    pub runs: Vec<(String, Vec<MapRun>)>,
}

/// `POST /api/admin/map/postpone`
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct Postpone {
    pub days: u32,
}

/// A spot by its place
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct SpotAt {
    pub lat: f64,
    pub lon: f64,
}

fn map(s: &AppState) -> Result<&wf_map::MapData, ApiError> {
    s.map.as_deref().ok_or(ApiError::NotFound)
}

pub async fn get(State(s): State<AppState>, _: Admin) -> Result<Json<AdminMap>, ApiError> {
    let m = map(&s)?;
    let mut runs = Vec::new();
    for model in wf_map::MODELS {
        runs.push((
            (*model).to_owned(),
            m.runs(model).await.map_err(ApiError::internal)?,
        ));
    }
    Ok(Json(AdminMap {
        basemap: m.basemap_state(&s.db).await,
        runs,
    }))
}

pub async fn renew(State(s): State<AppState>, admin: Admin) -> Result<StatusCode, ApiError> {
    map(&s)?.renew_now();
    super::settings::apply(&s, "basemapPostponedUntil", 0.into()).await?;
    admin.log(&s, "basemap.renew", None).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn cancel(State(s): State<AppState>, admin: Admin) -> Result<StatusCode, ApiError> {
    map(&s)?.cancel_renewal();
    admin.log(&s, "basemap.cancel", None).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn postpone(
    State(s): State<AppState>,
    admin: Admin,
    Json(p): Json<Postpone>,
) -> Result<StatusCode, ApiError> {
    map(&s)?;
    if !(1..=365).contains(&p.days) {
        return Err(ApiError::InvalidSetting);
    }
    let until = wf_core::time::now() + i64::from(p.days) * 86_400;
    super::settings::apply(&s, "basemapPostponedUntil", until.into()).await?;
    admin
        .log(&s, "basemap.postpone", Some(&p.days.to_string()))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/admin/map/spots?lat=&lon=`
pub async fn remove_spot(
    State(s): State<AppState>,
    admin: Admin,
    Query(at): Query<SpotAt>,
) -> Result<StatusCode, ApiError> {
    if !map(&s)?
        .remove_spot(at.lat, at.lon)
        .await
        .map_err(ApiError::internal)?
    {
        return Err(ApiError::NotFound);
    }
    pin(&s, at.lat, at.lon, false).await?;
    admin
        .log(
            &s,
            "spot.remove",
            Some(&format!("{:.1}, {:.1}", at.lat, at.lon)),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `PUT` / `DELETE /api/admin/map/spots/pinned`: keeps a spot whoever looks there, or no longer
pub async fn pin_spot(
    State(s): State<AppState>,
    admin: Admin,
    Json(at): Json<SpotAt>,
) -> Result<StatusCode, ApiError> {
    map(&s)?;
    pin(&s, at.lat, at.lon, true).await?;
    admin
        .log(
            &s,
            "spot.pin",
            Some(&format!("{:.1}, {:.1}", at.lat, at.lon)),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn unpin_spot(
    State(s): State<AppState>,
    admin: Admin,
    Json(at): Json<SpotAt>,
) -> Result<StatusCode, ApiError> {
    map(&s)?;
    pin(&s, at.lat, at.lon, false).await?;
    admin
        .log(
            &s,
            "spot.unpin",
            Some(&format!("{:.1}, {:.1}", at.lat, at.lon)),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Adds or takes a spot from the pinned ones (snapped as spots are).
async fn pin(s: &AppState, lat: f64, lon: f64, on: bool) -> Result<(), ApiError> {
    let spot = wf_map::basemap::spots::Spot::of(lat, lon);
    let mut pinned: Vec<[f64; 2]> = s
        .settings
        .get()
        .pinned_spots
        .iter()
        .copied()
        .filter(|[a, b]| wf_map::basemap::spots::Spot::of(*a, *b) != spot)
        .collect();
    if on {
        pinned.push([spot.lat, spot.lon]);
    }
    let value = serde_json::to_value(pinned).map_err(ApiError::internal)?;
    super::settings::apply(s, "pinnedSpots", value).await
}

/// `POST /api/admin/map/leftovers`: removes what a restart left behind; returns how many
pub async fn leftovers(State(s): State<AppState>, admin: Admin) -> Result<Json<usize>, ApiError> {
    let n = map(&s)?
        .clear_leftovers()
        .await
        .map_err(ApiError::internal)?;
    admin.log(&s, "map.leftovers", Some(&n.to_string())).await?;
    s.forget_disk_usage().await;
    Ok(Json(n))
}
