//! Push messages (RFC 8030) to the browsers that asked for them: the server signs with its own
//! VAPID key (RFC 8292), made on first start and kept in the data directory, and encrypts the
//! message for the browser (RFC 8291, aes128gcm). The web-push crate builds the request
//! (`request_builder::build_request`); reqwest sends it.

use std::path::Path;
use std::time::Duration;

use openssl::ec::{EcGroup, EcKey};
use openssl::nid::Nid;
use web_push::{
    ContentEncoding, SubscriptionInfo, Urgency, VapidSignatureBuilder, WebPushMessageBuilder,
};
use wf_core::alarm::PushSubscription;

use crate::Error;

/// How long a push service keeps a message for a browser that is offline: an alarm older than
/// half a day is no use.
const TTL_S: u32 = 12 * 3_600;

pub struct Push {
    /// SEC1 PEM of the P-256 key
    key: Vec<u8>,
    /// The public key as the browser's `applicationServerKey` wants it (base64url)
    public: String,
    /// Who runs the server, for the push services: a mailto: or https: address
    contact: String,
    /// Its own client: follows no redirect (a push service answers, it does not send on), and
    /// gives up sooner than the data sources' one
    http: reqwest::Client,
    /// Endpoints outside the known push services are refused (`known_push_service`); the tests
    /// allow a local stand-in
    any_host: bool,
}

/// What the push service said
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sent {
    Delivered,
    /// The browser unsubscribed or the subscription expired: forget it.
    Gone,
}

impl Push {
    /// The key at `path`, made there first if missing.
    pub fn load_or_create(path: &Path, contact: String) -> Result<Self, Error> {
        let key = match std::fs::read(path) {
            Ok(k) => k,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1)?;
                let pem = EcKey::generate(&group)?.private_key_to_pem()?;
                write_private(path, &pem)?;
                tracing::info!("made a VAPID key for push messages at {}", path.display());
                pem
            }
            Err(e) => return Err(e.into()),
        };
        let partial = VapidSignatureBuilder::from_pem_no_sub(&key[..])?;
        let http = reqwest::Client::builder()
            .user_agent(concat!("windfenster/", env!("CARGO_PKG_VERSION")))
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .build()?;
        Ok(Push {
            public: base64url(&partial.get_public_key()),
            key,
            contact,
            http,
            any_host: false,
        })
    }

    /// Sends to any endpoint, a local stand-in for a push service too: for tests only.
    pub fn allowing_any_host(self) -> Self {
        Push {
            any_host: true,
            ..self
        }
    }

    pub fn public_key(&self) -> &str {
        &self.public
    }

    /// Sends `payload` (JSON the service worker shows) to one browser.
    pub async fn send(&self, sub: &PushSubscription, payload: &[u8]) -> Result<Sent, Error> {
        // Stored before only known services were taken: forgotten instead of posted to
        if !self.any_host && !wf_core::alarm::known_push_service(&sub.endpoint) {
            return Ok(Sent::Gone);
        }
        let info = SubscriptionInfo::new(&sub.endpoint, &sub.p256dh, &sub.auth);
        let mut signature = VapidSignatureBuilder::from_pem(&self.key[..], &info)?;
        signature.add_claim("sub", self.contact.as_str());
        let mut message = WebPushMessageBuilder::new(&info);
        message.set_payload(ContentEncoding::Aes128Gcm, payload);
        message.set_ttl(TTL_S);
        message.set_urgency(Urgency::High);
        message.set_vapid_signature(signature.build()?);
        let (parts, body) =
            web_push::request_builder::build_request::<Vec<u8>>(message.build()?).into_parts();
        let mut request = self.http.post(parts.uri.to_string()).body(body);
        for (name, value) in &parts.headers {
            request = request.header(name.as_str(), value.as_bytes());
        }
        let status = request.send().await?.status();
        match status.as_u16() {
            200..=299 => Ok(Sent::Delivered),
            404 | 410 => Ok(Sent::Gone),
            s => Err(Error::Push(format!("push service answered {s}"))),
        }
    }
}

/// base64url without padding, as the Push API takes keys
fn base64url(bytes: &[u8]) -> String {
    openssl::base64::encode_block(bytes)
        .trim_end_matches('=')
        .replace('+', "-")
        .replace('/', "_")
}

/// Writes a private key readable by the owner only.
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)?.write_all(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn makes_a_key_once_and_keeps_it() -> Result<(), Error> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("vapid.pem");
        let a = Push::load_or_create(&path, "mailto:a@b.c".into())?;
        let b = Push::load_or_create(&path, "mailto:a@b.c".into())?;
        assert_eq!(a.public_key(), b.public_key());
        // An uncompressed P-256 point: 65 bytes, 87 characters in base64url
        assert_eq!(a.public_key().len(), 87);
        assert!(!a.public_key().contains(['+', '/', '=']));
        Ok(())
    }
}
