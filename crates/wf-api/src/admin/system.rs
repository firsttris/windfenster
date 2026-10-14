//! The system on the admin page: the version and whether a newer one is out, the environment
//! (secrets only as "set"), the caches with their sizes and a way to empty them.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Serialize;
use ts_rs::TS;
use wf_core::status::{self, Release};

use super::Admin;
use crate::{ApiError, AppState};

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EnvVar {
    pub name: String,
    /// The value; for a secret only "set"; null when not set
    pub value: Option<String>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AdminSystem {
    pub version: String,
    /// The newest release on GitHub, as last looked up
    pub release: Option<Release>,
    pub newer: bool,
    pub env: Vec<EnvVar>,
    /// Entries per in-memory cache
    #[ts(type = "Array<[string, number]>")]
    pub caches: Vec<(String, u64)>,
}

/// The variables shown, and whether each is a secret
const ENV: [(&str, bool); 15] = [
    ("WF_PUBLIC_URL", false),
    ("WF_LISTEN", false),
    ("WF_DATA_DIR", false),
    ("WF_CLIENT_IP_HEADER", false),
    ("WF_SCHEDULER", false),
    ("WF_MAP", false),
    ("WF_BASEMAP", false),
    ("WF_MODELS", false),
    ("WF_TRACKED_STATIONS", false),
    ("OPEN_METEO_BASE_URL", false),
    ("OPEN_METEO_API_KEY", true),
    ("WF_SMTP_URL", true),
    ("WF_MAIL_FROM", false),
    ("WF_UID", false),
    ("RUST_LOG", false),
];

pub async fn get(State(s): State<AppState>, _: Admin) -> Json<AdminSystem> {
    let release = status::board().release();
    Json(AdminSystem {
        version: crate::VERSION.to_owned(),
        newer: release
            .as_ref()
            .is_some_and(|r| status::newer(&r.version, crate::VERSION)),
        release,
        env: ENV
            .iter()
            .map(|(name, secret)| EnvVar {
                name: (*name).to_owned(),
                value: std::env::var(name)
                    .ok()
                    .filter(|v| !v.is_empty())
                    .map(|v| if *secret { "set".to_owned() } else { v }),
            })
            .collect(),
        caches: s
            .cache_sizes()
            .into_iter()
            .map(|(n, c)| (n.to_owned(), c))
            .collect(),
    })
}

/// `POST /api/admin/system/caches/clear`
pub async fn clear_caches(State(s): State<AppState>, admin: Admin) -> Result<StatusCode, ApiError> {
    s.clear_caches();
    admin.log(&s, "caches.clear", None).await?;
    Ok(StatusCode::NO_CONTENT)
}
