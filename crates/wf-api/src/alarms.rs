//! The signed-in user's alarms and the browsers that take their push messages.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use wf_alarm::Reached;
use wf_core::alarm::{self, Alarm, AlarmInput, PushSubscription};

use crate::auth::SignedIn;
use crate::{ApiError, AppState};

/// `GET /api/push`: what the server can send.
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PushInfo {
    /// The server's VAPID public key for `PushManager.subscribe`; null without push
    pub key: Option<String>,
    /// Whether alarms can go by e-mail (SMTP set up)
    pub email: bool,
}

pub async fn info(State(s): State<AppState>) -> Json<PushInfo> {
    let alarms = s.alarms.as_deref();
    Json(PushInfo {
        key: alarms
            .and_then(|a| a.push.as_ref())
            .map(|p| p.public_key().to_owned()),
        email: alarms.is_some_and(|a| a.mail.is_some()),
    })
}

/// `GET /api/alarms`
pub async fn list(
    State(s): State<AppState>,
    SignedIn(me): SignedIn,
) -> Result<Json<Vec<Alarm>>, ApiError> {
    Ok(Json(alarm::list(&s.db, me.id).await?))
}

/// `PUT /api/alarms/{favorite}`: sets the favourite's alarm, and how the browser shows things.
pub async fn set(
    State(s): State<AppState>,
    SignedIn(me): SignedIn,
    Path(favorite_id): Path<i64>,
    Json(input): Json<AlarmInput>,
) -> Result<Json<Alarm>, ApiError> {
    if !input.settings.valid()
        || !input.client.valid()
        || input.client.time_zone.parse::<chrono_tz::Tz>().is_err()
    {
        return Err(ApiError::InvalidAlarm);
    }
    if !alarm::set(
        &s.db,
        me.id,
        favorite_id,
        &input.settings,
        wf_core::time::now(),
    )
    .await?
    {
        return Err(ApiError::NotFound);
    }
    alarm::set_client(&s.db, me.id, &input.client).await?;
    Ok(Json(Alarm {
        favorite_id,
        settings: input.settings,
    }))
}

/// `DELETE /api/alarms/{favorite}`
pub async fn remove(
    State(s): State<AppState>,
    SignedIn(me): SignedIn,
    Path(favorite_id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    if alarm::remove(&s.db, me.id, favorite_id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

/// `POST /api/push/subscriptions`: this browser takes the account's push messages.
pub async fn subscribe(
    State(s): State<AppState>,
    SignedIn(me): SignedIn,
    Json(sub): Json<PushSubscription>,
) -> Result<StatusCode, ApiError> {
    if !sub.valid() {
        return Err(ApiError::InvalidSubscription);
    }
    alarm::subscribe(&s.db, me.id, &sub, wf_core::time::now()).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct Unsubscribe {
    pub endpoint: String,
}

/// `DELETE /api/push/subscriptions`: this browser no longer does.
pub async fn unsubscribe(
    State(s): State<AppState>,
    SignedIn(me): SignedIn,
    Json(u): Json<Unsubscribe>,
) -> Result<StatusCode, ApiError> {
    alarm::unsubscribe_own(&s.db, me.id, &u.endpoint).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/alarms/test`: a message to every browser of the account and its address.
pub async fn test(
    State(s): State<AppState>,
    SignedIn(me): SignedIn,
) -> Result<Json<Reached>, ApiError> {
    let Some(alarms) = s.alarms.as_deref() else {
        return Ok(Json(Reached::default()));
    };
    let locale = alarm::client(&s.db, me.id)
        .await?
        .map(|c| c.locale)
        .unwrap_or_else(|| "de".into());
    Ok(Json(alarms.test(me.id, me.email.as_deref(), &locale).await))
}
