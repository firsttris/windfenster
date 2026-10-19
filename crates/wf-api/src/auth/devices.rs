//! The account's passkeys and sessions on the account page: list, name and remove passkeys
//! (never the last way in), sign out on every other device (a lost phone stays signed in
//! otherwise, even after its passkey is removed).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::http::header::USER_AGENT;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use wf_core::account::passkeys::{self as keys, PasskeySummary, Removal};
use wf_core::account::{self, Account};

use super::{SignedIn, hash, token};
use crate::{ApiError, AppState};

/// The user's own name for a passkey; empty or null goes back to the browser and system.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct PasskeyName {
    pub name: Option<String>,
}

/// How many other sessions signing out everywhere else ended.
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct SessionsEnded {
    #[ts(type = "number")]
    pub ended: u64,
}

/// Notes the browser and system a passkey is added with, from the request's User-Agent.
pub(super) async fn note_device(
    s: &AppState,
    passkey_id: &str,
    request: &HeaderMap,
) -> Result<(), ApiError> {
    let ua = request
        .get(USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    keys::set_device(&s.db, passkey_id, keys::device_of(ua)).await?;
    Ok(())
}

/// `GET /api/me/passkeys`: the signed-in account's passkeys, oldest first.
pub async fn list_passkeys(
    State(s): State<AppState>,
    SignedIn(me): SignedIn,
) -> Result<Json<Vec<PasskeySummary>>, ApiError> {
    Ok(Json(keys::list(&s.db, me.id).await?))
}

/// `PATCH /api/me/passkeys/{id}`: the user's own name for a passkey; the passkey.
pub async fn rename_passkey(
    State(s): State<AppState>,
    SignedIn(me): SignedIn,
    Path(id): Path<String>,
    Json(req): Json<PasskeyName>,
) -> Result<Json<PasskeySummary>, ApiError> {
    let name = match req.name.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(n) => Some(account::clean_name(n).ok_or(ApiError::InvalidName)?),
    };
    keys::rename(&s.db, me.id, &id, name.as_deref())
        .await?
        .map(Json)
        .ok_or(ApiError::NotFound)
}

/// `DELETE /api/me/passkeys/{id}`: one passkey less; the account. The last way to sign in stays
/// (`last_sign_in`). The passkey stays in its device, it no longer signs in here; sessions it
/// started stay too (`DELETE /api/me/sessions` ends them).
pub async fn remove_passkey(
    State(s): State<AppState>,
    SignedIn(me): SignedIn,
    Path(id): Path<String>,
) -> Result<Json<Account>, ApiError> {
    match keys::remove(&s.db, me.id, &id).await? {
        Removal::Removed => Ok(Json(Account {
            passkeys: me.passkeys - 1,
            ..me
        })),
        Removal::NotFound => Err(ApiError::NotFound),
        Removal::LastSignIn => Err(ApiError::LastSignIn),
    }
}

/// `DELETE /api/me/sessions`: signs out on every device but this one.
pub async fn end_other_sessions(
    State(s): State<AppState>,
    SignedIn(me): SignedIn,
    headers: HeaderMap,
) -> Result<Json<SessionsEnded>, ApiError> {
    // SignedIn found the session by this token, so it is there.
    let keep = hash(&token(&headers).ok_or(ApiError::SignedOut)?);
    let ended = account::end_other_sessions(&s.db, me.id, &keep).await?;
    Ok(Json(SessionsEnded { ended }))
}
