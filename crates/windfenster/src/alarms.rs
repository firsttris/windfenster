//! Alarms as the server runs them: push messages with the server's own VAPID key (made in the
//! data directory on first start), e-mail when SMTP is set up.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Args;
use sqlx::SqlitePool;
use wf_alarm::{Alarms, Mail, Push};
use wf_map::MapData;

#[derive(Args)]
pub struct MailArgs {
    /// SMTP server for alarm e-mails, as `smtps://user:password@host:465` (TLS) or
    /// `smtp://user:password@host:587?tls=required` (STARTTLS); without, alarms go by push only.
    #[arg(long, env = "WF_SMTP_URL", hide_env_values = true)]
    pub smtp_url: Option<String>,
    /// Sender of alarm e-mails, e.g. `Windfenster <alarm@example.org>`.
    #[arg(long, env = "WF_MAIL_FROM")]
    pub mail_from: Option<String>,
}

/// The alarms, ready to run.
pub fn build(
    db: SqlitePool,
    data_dir: &Path,
    http: reqwest::Client,
    map: Option<Arc<MapData>>,
    mail: &MailArgs,
    public_url: Option<String>,
) -> Result<Alarms> {
    let from = crate::non_empty(&mail.mail_from);
    // Push services want to know whom to reach about the messages: the site or the sender.
    let contact = match (&public_url, &from) {
        (Some(u), _) if u.starts_with("https://") => u.clone(),
        (_, Some(f)) => format!("mailto:{}", address(f)),
        _ => "https://github.com/firsttris/windfenster".into(),
    };
    let push = Push::load_or_create(&data_dir.join("vapid.pem"), contact, http)
        .context("the push key (vapid.pem)")?;
    let mail = match (crate::non_empty(&mail.smtp_url), from) {
        (Some(url), Some(from)) => {
            Some(Mail::new(&url, &from).context("WF_SMTP_URL / WF_MAIL_FROM")?)
        }
        (Some(_), None) => anyhow::bail!("WF_SMTP_URL needs WF_MAIL_FROM"),
        _ => None,
    };
    tracing::info!(
        "alarms: push{}",
        if mail.is_some() {
            " and e-mail"
        } else {
            " only (no WF_SMTP_URL)"
        }
    );
    Ok(Alarms {
        db,
        map,
        push: Some(push),
        mail,
        public_url,
    })
}

/// The address in `Name <address>`
fn address(from: &str) -> &str {
    match (from.find('<'), from.rfind('>')) {
        (Some(a), Some(b)) if a < b => &from[a + 1..b],
        _ => from.trim(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn finds_the_address() {
        assert_eq!(
            super::address("Windfenster <alarm@example.org>"),
            "alarm@example.org"
        );
        assert_eq!(super::address(" alarm@example.org "), "alarm@example.org");
    }
}
