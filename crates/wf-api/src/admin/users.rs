//! Accounts on the admin page: the list, one in detail, the admin role, disabling, signing out
//! everywhere, deleting, and an account's data as JSON for its owner.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use wf_core::admin::log::AdminLogEntry;
use wf_core::admin::users::{self, AdminUser, AdminUserDetail, UserSummary};

use super::Admin;
use crate::{ApiError, AppState};

/// `GET /api/admin/users`
#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct AdminUsers {
    pub summary: UserSummary,
    pub users: Vec<AdminUser>,
}

/// `PUT /api/admin/users/{id}/role`
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct RoleChange {
    pub admin: bool,
}

pub async fn list(State(s): State<AppState>, _: Admin) -> Result<Json<AdminUsers>, ApiError> {
    Ok(Json(AdminUsers {
        summary: users::summary(&s.db, wf_core::time::now()).await?,
        users: users::list(&s.db).await?,
    }))
}

pub async fn detail(
    State(s): State<AppState>,
    _: Admin,
    Path(id): Path<i64>,
) -> Result<Json<AdminUserDetail>, ApiError> {
    users::detail(&s.db, id, wf_core::time::now())
        .await?
        .map(Json)
        .ok_or(ApiError::NotFound)
}

async fn named(s: &AppState, id: i64) -> Result<AdminUser, ApiError> {
    users::get(&s.db, id).await?.ok_or(ApiError::NotFound)
}

pub async fn role(
    State(s): State<AppState>,
    admin: Admin,
    Path(id): Path<i64>,
    Json(r): Json<RoleChange>,
) -> Result<StatusCode, ApiError> {
    let user = named(&s, id).await?;
    if !users::set_admin(&s.db, id, r.admin).await? {
        return Err(ApiError::LastAdmin);
    }
    let action = if r.admin { "role.grant" } else { "role.revoke" };
    admin.log(&s, action, Some(&user.name)).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn disable(
    State(s): State<AppState>,
    admin: Admin,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let user = named(&s, id).await?;
    if !users::disable(&s.db, id, wf_core::time::now()).await? {
        return Err(ApiError::LastAdmin);
    }
    admin.log(&s, "user.disable", Some(&user.name)).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn enable(
    State(s): State<AppState>,
    admin: Admin,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let user = named(&s, id).await?;
    users::enable(&s.db, id).await?;
    admin.log(&s, "user.enable", Some(&user.name)).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn sign_out(
    State(s): State<AppState>,
    admin: Admin,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let user = named(&s, id).await?;
    users::sign_out_everywhere(&s.db, id).await?;
    admin.log(&s, "user.signout", Some(&user.name)).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete(
    State(s): State<AppState>,
    admin: Admin,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let user = named(&s, id).await?;
    if !users::delete(&s.db, id).await? {
        return Err(ApiError::LastAdmin);
    }
    admin.log(&s, "user.delete", Some(&user.name)).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Sends the account's address its confirmation link again.
pub async fn confirm_email(
    State(s): State<AppState>,
    admin: Admin,
    Path(id): Path<i64>,
    headers: axum::http::HeaderMap,
) -> Result<StatusCode, ApiError> {
    let user = named(&s, id).await?;
    let email = user.email.as_deref().ok_or(ApiError::InvalidEmail)?;
    crate::auth::email::send_link(&s, id, email, crate::auth::email::lang_of(&headers)).await?;
    admin
        .log(&s, "user.confirm_email", Some(&user.name))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The account's data as a JSON file, for its owner (GDPR).
pub async fn export(
    State(s): State<AppState>,
    admin: Admin,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    let user = named(&s, id).await?;
    let data = users::export(&s.db, id).await?.ok_or(ApiError::NotFound)?;
    admin.log(&s, "user.export", Some(&user.name)).await?;
    let body = serde_json::to_vec_pretty(&data).map_err(ApiError::internal)?;
    let disposition = format!("attachment; filename=\"windfenster-account-{id}.json\"");
    Ok((
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            ),
            (
                header::CONTENT_DISPOSITION,
                HeaderValue::from_str(&disposition).map_err(ApiError::internal)?,
            ),
        ],
        body,
    )
        .into_response())
}

/// `GET /api/admin/log`: the last 200 admin actions
pub async fn log(
    State(s): State<AppState>,
    _: Admin,
) -> Result<Json<Vec<AdminLogEntry>>, ApiError> {
    Ok(Json(wf_core::admin::log::recent(&s.db, 200).await?))
}
