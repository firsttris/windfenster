//! Storage on the admin page: the data folder by part, rows per table, what a cleanup would
//! remove; a backup to download (the database as a consistent copy, the push key), compacting the
//! database, removing what ran out.

use axum::Json;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use tower::ServiceExt;
use ts_rs::TS;
use wf_core::admin::data::{self, Stale, TableRows};
use wf_map::disk::DiskUsage;

use super::Admin;
use crate::{ApiError, AppState};

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AdminStorage {
    pub disk: Option<DiskUsage>,
    pub tables: Vec<TableRows>,
    pub stale: Stale,
}

pub async fn get(State(s): State<AppState>, _: Admin) -> Result<Json<AdminStorage>, ApiError> {
    Ok(Json(AdminStorage {
        disk: s.disk_usage().await,
        tables: data::table_rows(&s.db).await?,
        stale: data::stale(&s.db, wf_core::time::now()).await?,
    }))
}

/// `POST /api/admin/storage/vacuum`: gives the room of deleted rows back to the disk.
pub async fn vacuum(State(s): State<AppState>, admin: Admin) -> Result<StatusCode, ApiError> {
    sqlx::query("VACUUM").execute(&s.db).await?;
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&s.db)
        .await?;
    s.forget_disk_usage().await;
    admin.log(&s, "db.vacuum", None).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /api/admin/storage/prune`: sessions and links that ran out, now instead of hourly.
pub async fn prune(State(s): State<AppState>, admin: Admin) -> Result<Json<u64>, ApiError> {
    let now = wf_core::time::now();
    let n = wf_core::account::prune_sessions(&s.db, now).await?
        + wf_core::account::prune_email_confirmations(&s.db, now).await?;
    admin.log(&s, "db.prune", Some(&n.to_string())).await?;
    Ok(Json(n))
}

/// `GET /api/admin/storage/backup`: the database as a consistent copy (VACUUM INTO), made
/// beside it, sent, then removed.
pub async fn backup(
    State(s): State<AppState>,
    admin: Admin,
    request: Request,
) -> Result<Response, ApiError> {
    let dir = s.data_dir.clone().ok_or(ApiError::NotFound)?;
    let path = dir.join(format!("windfenster-backup-{}.db", wf_core::time::now()));
    sqlx::query("VACUUM INTO ?")
        .bind(path.to_string_lossy().into_owned())
        .execute(&s.db)
        .await?;
    admin.log(&s, "db.backup", None).await?;
    // The file stays open while it is sent; its name goes at once.
    let served = tower_http::services::ServeFile::new(&path)
        .oneshot(request)
        .await
        .map_err(ApiError::internal)?;
    let _ = tokio::fs::remove_file(&path).await;
    let mut res = served.map(Body::new).into_response();
    let name = format!(
        "attachment; filename=\"windfenster-{}.db\"",
        wf_core::time::now()
    );
    res.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&name).map_err(ApiError::internal)?,
    );
    Ok(res)
}

/// `GET /api/admin/storage/push-key`: vapid.pem, which belongs with the backup.
pub async fn push_key(State(s): State<AppState>, admin: Admin) -> Result<Response, ApiError> {
    let dir = s.data_dir.clone().ok_or(ApiError::NotFound)?;
    let pem = tokio::fs::read(dir.join("vapid.pem"))
        .await
        .map_err(|_| ApiError::NotFound)?;
    admin.log(&s, "push.key_download", None).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/x-pem-file"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"vapid.pem\"",
            ),
        ],
        pem,
    )
        .into_response())
}
