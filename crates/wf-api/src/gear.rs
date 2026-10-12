//! The signed-in user's gear (kites, wings, sails), as a whole list.

use axum::Json;
use axum::extract::State;
use wf_core::gear::{self, GearItem, MAX_GEAR};

use crate::auth::SignedIn;
use crate::{ApiError, AppState};

/// `GET /api/gear`
pub async fn list(
    State(s): State<AppState>,
    SignedIn(me): SignedIn,
) -> Result<Json<Vec<GearItem>>, ApiError> {
    Ok(Json(gear::list(&s.db, me.id).await?))
}

/// `PUT /api/gear`: the whole list, in its order.
pub async fn replace(
    State(s): State<AppState>,
    SignedIn(me): SignedIn,
    Json(items): Json<Vec<GearItem>>,
) -> Result<Json<Vec<GearItem>>, ApiError> {
    if items.len() > MAX_GEAR || !items.iter().all(GearItem::valid) {
        return Err(ApiError::InvalidGear);
    }
    gear::replace(&s.db, me.id, &items).await?;
    Ok(Json(items))
}
