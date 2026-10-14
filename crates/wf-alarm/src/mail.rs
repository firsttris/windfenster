//! Alarm e-mails over SMTP, to the account's sign-in address. Every mail the server sends goes
//! through here and counts against one limit per UTC day, kept in the database (`usage_daily`):
//! however many accounts and alarms there are, the server cannot turn into a mailer.

use std::sync::atomic::{AtomicU32, Ordering};

use lettre::message::Mailbox;
use lettre::message::header::ContentType;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use wf_core::usage;

use crate::Error;

/// Mails a day by default
pub const PER_DAY: u32 = 500;

pub struct Mail {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
    /// Where the day's mails are counted; without, nothing is counted or limited
    store: Option<sqlx::SqlitePool>,
    per_day: AtomicU32,
}

impl Mail {
    /// `url` as lettre reads it: `smtps://user:password@host:465` (TLS) or
    /// `smtp://user:password@host:587?tls=required` (STARTTLS); `from` like
    /// `Windfenster <alarm@example.org>`.
    pub fn new(url: &str, from: &str) -> Result<Self, Error> {
        Ok(Mail {
            transport: AsyncSmtpTransport::<Tokio1Executor>::from_url(url)
                .map_err(|e| Error::Mail(e.to_string()))?
                .build(),
            from: from
                .parse()
                .map_err(|e: lettre::address::AddressError| Error::Mail(e.to_string()))?,
            store: None,
            per_day: AtomicU32::new(PER_DAY),
        })
    }

    /// Counts the mails per day in the database and sends no more than [`Self::per_day`].
    pub fn with_store(mut self, db: sqlx::SqlitePool) -> Self {
        self.store = Some(db);
        self
    }

    /// Mails a day at most
    pub fn per_day(&self) -> u32 {
        self.per_day.load(Ordering::Relaxed)
    }

    pub fn set_per_day(&self, n: u32) {
        self.per_day.store(n, Ordering::Relaxed);
    }

    /// Mails sent today
    pub async fn sent_today(&self) -> Result<u32, Error> {
        let Some(db) = &self.store else { return Ok(0) };
        let n = usage::get(db, usage::day(wf_core::time::now()), usage::MAIL).await?;
        Ok(u32::try_from(n).unwrap_or(u32::MAX))
    }

    /// Sends a plain-text mail, unless the day's limit is reached ([`Error::MailCap`]).
    pub async fn send(&self, to: &str, subject: &str, body: &str) -> Result<(), Error> {
        let to: Mailbox = to
            .parse()
            .map_err(|e: lettre::address::AddressError| Error::Mail(e.to_string()))?;
        let message = Message::builder()
            .from(self.from.clone())
            .to(to)
            .subject(subject)
            .header(ContentType::TEXT_PLAIN)
            .body(body.to_owned())
            .map_err(|e| Error::Mail(e.to_string()))?;
        let limit = self.per_day();
        if self.sent_today().await? >= limit {
            return Err(Error::MailCap(limit));
        }
        self.transport
            .send(message)
            .await
            .map_err(|e| Error::Mail(e.to_string()))?;
        if let Some(db) = &self.store {
            usage::add(db, usage::day(wf_core::time::now()), usage::MAIL, 1).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stops_at_the_days_limit() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db = wf_core::db::connect(&dir.path().join("t.db")).await?;
        wf_core::db::migrate(&db).await?;
        // nothing listens there: a mail that is let through fails to connect
        let mail = Mail::new("smtp://127.0.0.1:9", "Windfenster <alarm@example.org>")?
            .with_store(db.clone());
        mail.set_per_day(2);
        usage::add(&db, usage::day(wf_core::time::now()), usage::MAIL, 2).await?;
        assert_eq!(mail.sent_today().await?, 2);
        assert!(matches!(
            mail.send("a@example.org", "s", "b").await,
            Err(Error::MailCap(2))
        ));
        mail.set_per_day(3);
        assert!(matches!(
            mail.send("a@example.org", "s", "b").await,
            Err(Error::Mail(_))
        ));
        // a mail that did not go out does not count
        assert_eq!(mail.sent_today().await?, 2);
        Ok(())
    }
}
