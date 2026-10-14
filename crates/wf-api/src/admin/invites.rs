//! Invitations on the admin page: making one (its link is shown once), the list, withdrawing.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Serialize;
use ts_rs::TS;
use wf_core::admin::invites::{self, Invite};

use super::Admin;
use crate::auth::{hash, random};
use crate::{ApiError, AppState};

/// A new invitation: its link, shown only now (only the token's hash is kept)
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NewInvite {
    /// The sign-in page with the token; relative when the server has no public address
    pub link: String,
    #[ts(type = "number")]
    pub expires_at: i64,
}

pub async fn list(State(s): State<AppState>, _: Admin) -> Result<Json<Vec<Invite>>, ApiError> {
    Ok(Json(invites::list(&s.db, wf_core::time::now()).await?))
}

pub async fn create(State(s): State<AppState>, admin: Admin) -> Result<Json<NewInvite>, ApiError> {
    let token = hex::encode(random::<16>());
    let now = wf_core::time::now();
    invites::create(&s.db, &hash(&token), admin.0.id, now).await?;
    admin.log(&s, "invite.create", None).await?;
    let base = s.public_url.as_deref().unwrap_or("").trim_end_matches('/');
    Ok(Json(NewInvite {
        link: format!("{base}/login?invite={token}"),
        expires_at: now + invites::VALID_S,
    }))
}

pub async fn withdraw(
    State(s): State<AppState>,
    admin: Admin,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if !invites::withdraw(&s.db, &id).await? {
        return Err(ApiError::NotFound);
    }
    admin.log(&s, "invite.withdraw", None).await?;
    Ok(StatusCode::NO_CONTENT)
}
