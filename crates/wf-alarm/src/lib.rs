//! Alarms: every ten minutes each favourite with an alarm is checked against the wind map's
//! forecast for the spot (ahead: a window today or tomorrow, a change of it, its end) and against
//! the station's latest measurements (live), and the account is told by push message and e-mail.
//! Nothing is sent at night (21–7 local time); what waits is told in the morning if still true.

mod check;
pub mod mail;
pub mod push;
pub mod text;

use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use sqlx::SqlitePool;
use wf_core::alarm::{self, Job};
use wf_map::service::MapData;

pub use mail::Mail;
pub use push::Push;
use text::{Lang, Text};

/// How often the alarms are checked
const EVERY: Duration = Duration::from_secs(600);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("key: {0}")]
    Key(#[from] openssl::error::ErrorStack),
    #[error("push: {0}")]
    WebPush(#[from] web_push::WebPushError),
    #[error("push: {0}")]
    Http(#[from] reqwest::Error),
    #[error("{0}")]
    Push(String),
    #[error("e-mail: {0}")]
    Mail(String),
}

pub struct Alarms {
    pub db: SqlitePool,
    /// The forecast comes from the wind map's fields; without them only live alarms work.
    pub map: Option<Arc<MapData>>,
    pub push: Option<Push>,
    pub mail: Option<Mail>,
    /// For links in e-mails and push messages
    pub public_url: Option<String>,
}

/// What a test message reached
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Reached {
    /// Browsers the push message reached
    #[ts(type = "number")]
    pub browsers: usize,
    pub email: bool,
}

/// What a push message carries to the service worker (`web/public/sw.js`)
#[derive(Serialize)]
struct Payload<'a> {
    title: &'a str,
    body: &'a str,
    /// Opened on a tap, relative to the site
    url: &'a str,
    /// A newer message with the same tag replaces the older one.
    tag: &'a str,
}

impl Alarms {
    /// Checks the alarms every ten minutes, for good.
    pub async fn run(self: Arc<Self>) {
        let mut tick = tokio::time::interval(EVERY);
        loop {
            tick.tick().await;
            let now = wf_core::time::now();
            if let Err(e) = self.check(now).await {
                tracing::error!("alarms: {e}");
            }
        }
    }

    /// One round over every alarm.
    pub async fn check(&self, now: i64) -> Result<(), Error> {
        let jobs = alarm::jobs(&self.db).await?;
        if jobs.is_empty() {
            return Ok(());
        }
        let index = match &self.map {
            Some(m) => Some(m.index().await?),
            None => None,
        };
        for job in &jobs {
            if let Err(e) = check::job(self, index.as_ref(), job, now).await {
                tracing::warn!("alarm {}: {e}", job.favorite.id);
            }
        }
        // What was told about days long past is no longer needed.
        alarm::forget(&self.db, check::yyyymmdd(now - 3 * 86_400)).await?;
        Ok(())
    }

    /// Sends a message by the channels the alarm asks for; true when any reached the user.
    async fn deliver(&self, job: &Job, text: &Text, tag: &str) -> bool {
        let url = spot_path(job);
        let mut reached = false;
        if job.settings.push {
            reached |= self.push_to(job.account_id, text, &url, tag).await > 0;
        }
        if job.settings.email
            && let (Some(mail), Some(to)) = (&self.mail, &job.email)
        {
            let link = self.public_url.as_ref().map(|u| format!("{u}{url}"));
            let account = self.public_url.as_ref().map(|u| format!("{u}/account"));
            let body = text::email_body(
                Lang::of(&job.locale),
                text,
                link.as_deref(),
                account.as_deref(),
            );
            match mail.send(to, &text.title, &body).await {
                Ok(()) => reached = true,
                Err(e) => tracing::warn!("alarm e-mail: {e}"),
            }
        }
        reached
    }

    /// Sends to every browser of the account; forgets those the push service says are gone.
    /// Returns how many took it.
    async fn push_to(&self, account_id: i64, text: &Text, url: &str, tag: &str) -> usize {
        let Some(push) = &self.push else { return 0 };
        let subs = match alarm::subscriptions(&self.db, account_id).await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("push targets: {e}");
                return 0;
            }
        };
        let payload = serde_json::to_vec(&Payload {
            title: &text.title,
            body: &text.body,
            url,
            tag,
        })
        .unwrap_or_default();
        let mut reached = 0;
        for sub in subs {
            match push.send(&sub, &payload).await {
                Ok(push::Sent::Delivered) => reached += 1,
                Ok(push::Sent::Gone) => {
                    if let Err(e) = alarm::unsubscribe(&self.db, &sub.endpoint).await {
                        tracing::warn!("forgetting a push target: {e}");
                    }
                }
                Err(e) => tracing::warn!("push: {e}"),
            }
        }
        reached
    }

    /// A test message to all of the account's browsers and its address.
    pub async fn test(&self, account_id: i64, email: Option<&str>, locale: &str) -> Reached {
        let lang = Lang::of(locale);
        let t = text::test(lang);
        let browsers = self.push_to(account_id, &t, "/account", "test").await;
        let email = match (&self.mail, email) {
            (Some(mail), Some(to)) => mail
                .send(to, &t.title, &text::email_body(lang, &t, None, None))
                .await
                .inspect_err(|e| tracing::warn!("test e-mail: {e}"))
                .is_ok(),
            _ => false,
        };
        Reached { browsers, email }
    }
}

/// The spot's page in the app
fn spot_path(job: &Job) -> String {
    let f = &job.favorite;
    match &f.slug {
        Some(slug) => format!("/station/{slug}"),
        None => format!("/point?lat={:.2}&lon={:.2}", f.lat, f.lon),
    }
}
