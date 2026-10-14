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
    #[error("e-mail: the day's {0} mails are sent")]
    MailCap(u32),
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
        let board = wf_core::status::board();
        board.register("alarms", EVERY.as_secs() as i64);
        let mut tick = tokio::time::interval(EVERY);
        loop {
            tokio::select! {
                _ = tick.tick() => {}
                () = board.asked() => if !board.take("alarms") { continue },
            }
            let now = wf_core::time::now();
            let report = board.start("alarms");
            match self.check(now).await {
                Ok(n) => report.ok(Some(n as i64)),
                Err(e) => {
                    tracing::error!("alarms: {e}");
                    report.failed(&e);
                }
            }
        }
    }

    /// Notes what a channel made of a message, for the admin page.
    async fn record(
        &self,
        job_or_account: (i64, Option<i64>),
        channel: &str,
        kind: &str,
        title: &str,
        outcome: Result<(), String>,
    ) {
        let (account_id, favorite_id) = job_or_account;
        let detail = outcome.as_ref().err().map(String::as_str);
        let r = alarm::delivery::Record {
            account_id,
            favorite_id,
            channel,
            kind,
            title,
            ok: outcome.is_ok(),
            detail,
        };
        if let Err(e) = alarm::delivery::record(&self.db, wf_core::time::now(), &r).await {
            tracing::warn!("recording an alarm delivery: {e}");
        }
    }

    /// One round over every alarm.
    /// Checks every alarm once; returns how many there were.
    pub async fn check(&self, now: i64) -> Result<usize, Error> {
        let jobs = alarm::jobs(&self.db).await?;
        if jobs.is_empty() {
            return Ok(0);
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
        Ok(jobs.len())
    }

    /// Sends a message by the channels the alarm asks for; true when any reached the user.
    async fn deliver(&self, job: &Job, text: &Text, tag: &str) -> bool {
        let url = spot_path(job);
        let who = (job.account_id, Some(job.favorite.id));
        let kind = if tag.starts_with("live-") {
            "live"
        } else {
            "window"
        };
        let mut reached = false;
        if job.settings.push {
            let sent = self.push_to(job.account_id, text, &url, tag).await;
            reached |= sent.reached > 0;
            self.record(who, "push", kind, &text.title, sent.outcome())
                .await;
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
            let sent = mail.send(to, &text.title, &body).await;
            match &sent {
                Ok(()) => reached = true,
                Err(e) => tracing::warn!("alarm e-mail: {e}"),
            }
            let outcome = sent.map_err(|e| e.to_string());
            self.record(who, "email", kind, &text.title, outcome).await;
        }
        reached
    }

    /// Sends to every browser of the account; forgets those the push service says are gone.
    async fn push_to(&self, account_id: i64, text: &Text, url: &str, tag: &str) -> Pushed {
        let mut out = Pushed::default();
        let Some(push) = &self.push else { return out };
        let subs = match alarm::subscriptions(&self.db, account_id).await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("push targets: {e}");
                out.error = Some(e.to_string());
                return out;
            }
        };
        let payload = serde_json::to_vec(&Payload {
            title: &text.title,
            body: &text.body,
            url,
            tag,
        })
        .unwrap_or_default();
        out.devices = subs.len();
        for sub in subs {
            match push.send(&sub, &payload).await {
                Ok(push::Sent::Delivered) => out.reached += 1,
                Ok(push::Sent::Gone) => {
                    out.gone += 1;
                    if let Err(e) = alarm::unsubscribe(&self.db, &sub.endpoint).await {
                        tracing::warn!("forgetting a push target: {e}");
                    }
                }
                Err(e) => {
                    tracing::warn!("push: {e}");
                    out.error = Some(e.to_string());
                }
            }
        }
        out
    }

    /// A test message to all of the account's browsers and its address.
    pub async fn test(&self, account_id: i64, email: Option<&str>, locale: &str) -> Reached {
        let lang = Lang::of(locale);
        let t = text::test(lang);
        let who = (account_id, None);
        let pushed = self.push_to(account_id, &t, "/account", "test").await;
        if pushed.devices > 0 || pushed.error.is_some() {
            self.record(who, "push", "test", &t.title, pushed.outcome())
                .await;
        }
        let email = match (&self.mail, email) {
            (Some(mail), Some(to)) => {
                let sent = mail
                    .send(to, &t.title, &text::email_body(lang, &t, None, None))
                    .await
                    .inspect_err(|e| tracing::warn!("test e-mail: {e}"));
                let ok = sent.is_ok();
                self.record(
                    who,
                    "email",
                    "test",
                    &t.title,
                    sent.map_err(|e| e.to_string()),
                )
                .await;
                ok
            }
            _ => false,
        };
        Reached {
            browsers: pushed.reached,
            email,
        }
    }
}

/// What a push message met at the account's browsers
#[derive(Default)]
struct Pushed {
    devices: usize,
    reached: usize,
    /// Browsers the push service says are gone (unsubscribed)
    gone: usize,
    error: Option<String>,
}

impl Pushed {
    fn outcome(&self) -> Result<(), String> {
        if self.reached > 0 {
            Ok(())
        } else if let Some(e) = &self.error {
            Err(e.clone())
        } else if self.gone > 0 {
            Err("push: gone".into())
        } else {
            Err("push: no device".into())
        }
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
