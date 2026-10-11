//! The signed-in user's favourites: stations or points, own name, the wind directions that work.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;
use ts_rs::TS;
use wf_core::favorite::{self, Favorite, FavoriteInput, Sector};
use wf_core::geo::GridPoint;

use crate::auth::SignedIn;
use crate::{ApiError, AppState};

/// A favourite's new name and directions.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct FavoriteChange {
    pub name: String,
    pub sector: Option<Sector>,
}

/// The favourites in their new order.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct FavoriteOrder {
    #[ts(type = "number[]")]
    pub ids: Vec<i64>,
}

fn checked(name: &str, sector: Option<Sector>) -> Result<String, ApiError> {
    if sector.is_some_and(|s| !s.valid()) {
        return Err(ApiError::InvalidFavorite);
    }
    wf_core::account::clean_name(name).ok_or(ApiError::InvalidName)
}

/// `GET /api/favorites`
pub async fn list(
    State(s): State<AppState>,
    SignedIn(me): SignedIn,
) -> Result<Json<Vec<Favorite>>, ApiError> {
    Ok(Json(favorite::list(&s.db, me.id).await?))
}

/// `POST /api/favorites`: a station (its place from the station list) or a point (on the
/// 0.01° grid, as forecasts are kept).
pub async fn add(
    State(s): State<AppState>,
    SignedIn(me): SignedIn,
    Json(f): Json<FavoriteInput>,
) -> Result<(StatusCode, Json<Favorite>), ApiError> {
    let name = checked(&f.name, f.sector)?;
    let (lat, lon) = match &f.slug {
        Some(slug) => {
            let st = wf_core::station::by_slug(&s.db, slug)
                .await?
                .ok_or(ApiError::NotFound)?;
            (st.lat, st.lon)
        }
        None => {
            let p = GridPoint::new(f.lat, f.lon).ok_or(ApiError::InvalidCoordinates)?;
            (p.lat(), p.lon())
        }
    };
    let input = FavoriteInput {
        lat,
        lon,
        name,
        ..f
    };
    let added = favorite::add(&s.db, me.id, &input, wf_core::time::now())
        .await?
        .ok_or(ApiError::TooManyFavorites)?;
    Ok((StatusCode::CREATED, Json(added)))
}

/// `PUT /api/favorites/{id}`
pub async fn change(
    State(s): State<AppState>,
    SignedIn(me): SignedIn,
    Path(id): Path<i64>,
    Json(c): Json<FavoriteChange>,
) -> Result<Json<Favorite>, ApiError> {
    let name = checked(&c.name, c.sector)?;
    favorite::update(&s.db, me.id, id, &name, c.sector)
        .await?
        .map(Json)
        .ok_or(ApiError::NotFound)
}

/// `DELETE /api/favorites/{id}`
pub async fn remove(
    State(s): State<AppState>,
    SignedIn(me): SignedIn,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    if favorite::remove(&s.db, me.id, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

/// `PUT /api/favorites/order`
pub async fn reorder(
    State(s): State<AppState>,
    SignedIn(me): SignedIn,
    Json(o): Json<FavoriteOrder>,
) -> Result<StatusCode, ApiError> {
    favorite::reorder(&s.db, me.id, &o.ids).await?;
    Ok(StatusCode::NO_CONTENT)
}
