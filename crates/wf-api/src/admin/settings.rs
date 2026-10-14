//! Settings on the admin page: all of them with which the environment fixes, and changing one.

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use wf_core::admin::settings::{SettingError, Settings};

use super::Admin;
use crate::{ApiError, AppState};

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct AdminSettings {
    pub settings: Settings,
    /// Settings an environment variable fixes, with its name
    pub locked: BTreeMap<String, String>,
    /// Every model the server knows (id, name), for picking those in use
    pub models: Vec<(String, String)>,
}

/// `PUT /api/admin/settings/{key}`
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct SettingChange {
    #[ts(type = "unknown")]
    pub value: serde_json::Value,
}

fn view(s: &AppState) -> AdminSettings {
    AdminSettings {
        settings: (*s.settings.get()).clone(),
        locked: s.settings.locked(),
        models: s
            .forecasts
            .all_models()
            .iter()
            .map(|m| (m.id.clone(), m.name.clone()))
            .collect(),
    }
}

pub async fn get(State(s): State<AppState>, _: Admin) -> Json<AdminSettings> {
    Json(view(&s))
}

pub async fn set(
    State(s): State<AppState>,
    admin: Admin,
    Path(key): Path<String>,
    Json(c): Json<SettingChange>,
) -> Result<Json<AdminSettings>, ApiError> {
    if key == "models"
        && let Some(ids) = c.value.as_array()
    {
        let known = s.forecasts.all_models();
        let ok = !ids.is_empty()
            && ids
                .iter()
                .all(|id| known.iter().any(|m| Some(m.id.as_str()) == id.as_str()));
        if !ok {
            return Err(ApiError::InvalidSetting);
        }
    }
    apply(&s, &key, c.value).await?;
    admin.log(&s, "setting.set", Some(&key)).await?;
    Ok(Json(view(&s)))
}

/// Stores a setting and passes it on to what does not read the settings each time.
pub(super) async fn apply(
    s: &AppState,
    key: &str,
    value: serde_json::Value,
) -> Result<(), ApiError> {
    let next = s
        .settings
        .set(key, value, wf_core::time::now())
        .await
        .map_err(|e| match e {
            SettingError::Unknown(_) => ApiError::NotFound,
            SettingError::Locked(..) | SettingError::Invalid(_) => ApiError::InvalidSetting,
        })?;
    s.forecasts.settings_changed();
    if let Some(mail) = s.alarms.as_deref().and_then(|a| a.mail.as_ref()) {
        mail.set_per_day(next.mail_per_day);
    }
    Ok(())
}
