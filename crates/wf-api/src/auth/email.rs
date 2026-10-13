//! Confirming an e-mail address by a link sent to it. Alarm e-mails go only to a confirmed
//! address: without that, anyone could sign up with someone else's address and have the server
//! mail them. The link opens a page in the app that confirms with a button (a POST), so a mail
//! scanner that opens every link confirms nothing. The mails it sends are limited: one per address
//! in ten minutes, and a number per day for the whole server.

use std::sync::Mutex;
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header::ACCEPT_LANGUAGE};
use serde::Deserialize;
use ts_rs::TS;
use wf_alarm::text::{self, Lang};
use wf_core::account;

use super::{SignedIn, hash, random};
use crate::{ApiError, AppState};

/// How long a link works
const LINK_HOURS: i64 = 24;
/// One mail per address in this time
const PER_ADDRESS: Duration = Duration::from_secs(600);
/// At most this many confirmation mails a day from the whole server: sign-ups with ever new
/// addresses cannot turn it into a mailer.
const PER_DAY: u32 = 200;

/// What the server has mailed lately.
pub struct Mailing {
    addresses: moka::future::Cache<String, ()>,
    /// (day number, mails that day)
    today: Mutex<(i64, u32)>,
}

impl Mailing {
    pub fn new() -> Self {
        Mailing {
            addresses: moka::future::Cache::builder()
                .max_capacity(100_000)
                .time_to_live(PER_ADDRESS)
                .build(),
            today: Mutex::new((0, 0)),
        }
    }

    /// Whether a mail to `email` may go now; counts it when so.
    async fn allow(&self, email: &str, now: i64) -> bool {
        let day = now.div_euclid(86_400);
        let full = |today: &(i64, u32)| today.0 == day && today.1 >= PER_DAY;
        if full(&self.lock()) {
            return false;
        }
        let fresh = self
            .addresses
            .entry_by_ref(email)
            .or_insert(())
            .await
            .is_fresh();
        let mut today = self.lock();
        if !fresh || full(&today) {
            return false;
        }
        if today.0 != day {
            *today = (day, 0);
        }
        today.1 += 1;
        true
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, (i64, u32)> {
        self.today.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Default for Mailing {
    fn default() -> Self {
        Self::new()
    }
}

/// The reader's language from `Accept-Language`: English when it comes before German.
pub(crate) fn lang_of(headers: &HeaderMap) -> Lang {
    let accept = headers
        .get(ACCEPT_LANGUAGE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let first = accept
        .split(',')
        .map(|t| t.trim().get(..2).unwrap_or_default().to_ascii_lowercase())
        .find(|t| t == "de" || t == "en");
    Lang::of(first.as_deref().unwrap_or("de"))
}

/// Sends a link that confirms `email` for the account. Needs e-mail set up and the public
/// address (the link points there).
pub(crate) async fn send_link(
    s: &AppState,
    account_id: i64,
    email: &str,
    lang: Lang,
) -> Result<(), ApiError> {
    let mail = s
        .alarms
        .as_deref()
        .and_then(|a| a.mail.as_ref())
        .ok_or(ApiError::EmailOff)?;
    let base = s.public_url.as_deref().ok_or(ApiError::EmailOff)?;
    let now = wf_core::time::now();
    if !s.mailing.allow(email, now).await {
        return Err(ApiError::TooSoon);
    }
    let token = hex::encode(random::<32>());
    account::add_email_confirmation(
        &s.db,
        &hash(&token),
        account_id,
        email,
        now,
        now + LINK_HOURS * 3_600,
    )
    .await?;
    let link = format!("{}/confirm-email?token={token}", base.trim_end_matches('/'));
    let (subject, body) = text::confirm_email(lang, &link, LINK_HOURS);
    mail.send(email, &subject, &body)
        .await
        .map_err(ApiError::internal)
}

/// After an address was set: sends its link when it can, without failing the request (the
/// account page offers to send it again).
pub(crate) async fn offer_link(s: &AppState, account_id: i64, email: &str, lang: Lang) {
    match send_link(s, account_id, email, lang).await {
        Ok(()) | Err(ApiError::EmailOff | ApiError::TooSoon) => {}
        Err(e) => tracing::warn!("confirmation e-mail: {e:?}"),
    }
}

/// `POST /api/me/email/confirm`: sends the signed-in account's address a link (again).
pub async fn request(
    State(s): State<AppState>,
    SignedIn(me): SignedIn,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let email = me.email.ok_or(ApiError::InvalidEmail)?;
    if !me.email_confirmed {
        send_link(&s, me.id, &email, lang_of(&headers)).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct ConfirmEmail {
    pub token: String,
}

/// `POST /api/auth/email/confirm`: the link's token; works without signing in (the mail may be
/// opened on another device).
pub async fn confirm(
    State(s): State<AppState>,
    Json(c): Json<ConfirmEmail>,
) -> Result<StatusCode, ApiError> {
    if account::confirm_email(&s.db, &hash(&c.token), wf_core::time::now()).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::InvalidLink)
    }
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    #[test]
    fn reads_the_language() {
        let of = |v: &'static str| {
            let mut h = HeaderMap::new();
            h.insert(ACCEPT_LANGUAGE, HeaderValue::from_static(v));
            lang_of(&h)
        };
        assert!(matches!(of("en-GB,en;q=0.9,de;q=0.8"), Lang::En));
        assert!(matches!(of("de-DE,de;q=0.9,en;q=0.8"), Lang::De));
        assert!(matches!(of("fr-FR,en;q=0.5"), Lang::En));
        assert!(matches!(of("fr"), Lang::De));
        assert!(matches!(lang_of(&HeaderMap::new()), Lang::De));
    }

    #[tokio::test]
    async fn mails_an_address_once_in_ten_minutes_and_few_a_day() {
        let m = Mailing::new();
        assert!(m.allow("a@x.de", 0).await);
        assert!(!m.allow("a@x.de", 1).await);
        for i in 1..PER_DAY {
            assert!(m.allow(&format!("{i}@x.de"), 2).await);
        }
        assert!(!m.allow("late@x.de", 3).await);
        // A new day starts the count again.
        assert!(m.allow("late@x.de", 86_400).await);
    }
}
