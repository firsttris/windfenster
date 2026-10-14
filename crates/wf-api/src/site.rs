//! What every visitor's app needs from the admin page: whether accounts can be made, the notice
//! above every page, and the legal notice and privacy policy.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::header::CACHE_CONTROL;
use axum::response::IntoResponse;
use serde::Serialize;
use ts_rs::TS;
use wf_core::admin::settings::SignupMode;

use crate::{ApiError, AppState};

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Site {
    pub signup: SignupMode,
    /// A notice above every page; null: none
    pub banner: Option<String>,
    /// Whether there is a legal notice, a privacy policy
    pub imprint: bool,
    pub privacy: bool,
}

/// `GET /api/site`
pub async fn site(State(s): State<AppState>) -> impl IntoResponse {
    let st = s.settings.get();
    (
        [(CACHE_CONTROL, "no-cache")],
        Json(Site {
            signup: st.signup,
            banner: Some(st.banner.trim().to_owned()).filter(|b| !b.is_empty()),
            imprint: !st.imprint.trim().is_empty(),
            privacy: !st.privacy.trim().is_empty(),
        }),
    )
}

/// `GET /api/site/{imprint|privacy}`: the text as Markdown
pub async fn legal(
    State(s): State<AppState>,
    Path(page): Path<String>,
) -> Result<Json<String>, ApiError> {
    let st = s.settings.get();
    let text = match page.as_str() {
        "imprint" => &st.imprint,
        "privacy" => &st.privacy,
        _ => return Err(ApiError::NotFound),
    };
    if text.trim().is_empty() {
        return Err(ApiError::NotFound);
    }
    Ok(Json(text.clone()))
}
