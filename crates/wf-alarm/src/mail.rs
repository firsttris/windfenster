//! Alarm e-mails over SMTP, to the account's sign-in address.

use lettre::message::Mailbox;
use lettre::message::header::ContentType;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use crate::Error;

pub struct Mail {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
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
        })
    }

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
        self.transport
            .send(message)
            .await
            .map_err(|e| Error::Mail(e.to_string()))?;
        Ok(())
    }
}
