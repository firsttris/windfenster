//! Alarms and mail on the admin page: the SMTP server, the day's mails against the limit, the
//! push key, the browsers by push service, every delivery of the last month; a test mail.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use wf_core::admin::data::{self, AlarmCounts};
use wf_core::alarm::delivery::{self, Delivery};

use super::Admin;
use super::overview::AlarmsToday;
use crate::{ApiError, AppState};

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Smtp {
    /// Scheme, host and port; never the password
    pub server: String,
    pub from: String,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AdminAlarms {
    /// None: no e-mail (no WF_SMTP_URL)
    pub smtp: Option<Smtp>,
    pub mail_per_day: u32,
    pub mails_today: u32,
    pub push: bool,
    /// When the push key (vapid.pem) was made
    #[ts(type = "number | null")]
    pub push_key_since: Option<i64>,
    pub counts: AlarmCounts,
    pub today: AlarmsToday,
    /// The last 200, newest first
    pub deliveries: Vec<Delivery>,
}

/// `POST /api/admin/alarms/test-mail`
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct TestMail {
    pub to: String,
}

pub async fn get(State(s): State<AppState>, _: Admin) -> Result<Json<AdminAlarms>, ApiError> {
    let now = wf_core::time::now();
    let alarms = s.alarms.as_deref();
    let mail = alarms.and_then(|a| a.mail.as_ref());
    let mails_today = match mail {
        Some(m) => m.sent_today().await.map_err(ApiError::internal)?,
        None => 0,
    };
    let push_key_since = match &s.data_dir {
        Some(dir) => tokio::fs::metadata(dir.join("vapid.pem"))
            .await
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64),
        None => None,
    };
    let (push, email, failed) = delivery::counts(&s.db, now - now.rem_euclid(86_400)).await?;
    Ok(Json(AdminAlarms {
        smtp: mail.map(|m| Smtp {
            server: m.server().to_owned(),
            from: m.from(),
        }),
        mail_per_day: s.settings.get().mail_per_day,
        mails_today,
        push: alarms.is_some_and(|a| a.push.is_some()),
        push_key_since,
        counts: data::alarm_counts(&s.db).await?,
        today: AlarmsToday {
            push,
            email,
            failed,
        },
        deliveries: delivery::recent(&s.db, 200).await?,
    }))
}

pub async fn test_mail(
    State(s): State<AppState>,
    admin: Admin,
    headers: HeaderMap,
    Json(t): Json<TestMail>,
) -> Result<StatusCode, ApiError> {
    let mail = s
        .alarms
        .as_deref()
        .and_then(|a| a.mail.as_ref())
        .ok_or(ApiError::EmailOff)?;
    let to = wf_core::account::clean_email(&t.to).ok_or(ApiError::InvalidEmail)?;
    let lang = crate::auth::email::lang_of(&headers);
    let text = wf_alarm::text::test(lang);
    let body = wf_alarm::text::email_body(lang, &text, None, None);
    mail.send(&to, &text.title, &body)
        .await
        .map_err(|e| match e {
            wf_alarm::Error::MailCap(_) => ApiError::TooSoon,
            e => ApiError::internal(e),
        })?;
    admin.log(&s, "mail.test", None).await?;
    Ok(StatusCode::NO_CONTENT)
}
