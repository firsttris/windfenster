//! Signing in with passkeys (WebAuthn, webauthn-rs) and the session cookie. A ceremony has two
//! steps: the server sends options (with a fresh challenge) under a ceremony id, the browser
//! answers with the credential; between the two the server keeps the state in memory for five
//! minutes. Passkeys are made discoverable, so the browser can offer them in the name field
//! (conditional UI, as in firsttris/haben's login) without the user typing anything.

pub mod email;
pub mod password;
mod routes;
pub(crate) mod signup;

use std::sync::Arc;
use std::time::Duration;

use axum::extract::FromRequestParts;
use axum::http::header::{COOKIE, SET_COOKIE};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue};
use webauthn_rs::prelude::{
    DiscoverableAuthentication, PasskeyRegistration, Url, Uuid, Webauthn, WebauthnBuilder,
};
use wf_core::account::{self, Account};

use crate::{ApiError, AppState};

pub use routes::{
    PasskeyAnswer, PasskeyOptions, SignIn, SignUp, add_passkey, delete_me, finish_add_passkey,
    finish_sign_in, finish_sign_up, me, rename_me, sign_in, sign_out, sign_up,
};

const COOKIE_NAME: &str = "wf_session";
/// How long the browser has to answer a ceremony
const CEREMONY_S: u64 = 300;

/// A ceremony between its two steps.
enum Pending {
    SignUp {
        handle: Uuid,
        name: String,
        state: PasskeyRegistration,
        /// The invitation it came with, used once the account is made
        invite: Option<String>,
    },
    AddPasskey {
        account_id: i64,
        state: PasskeyRegistration,
    },
    SignIn(DiscoverableAuthentication),
}

/// WebAuthn for this server's address and the ceremonies under way.
pub struct Passkeys {
    webauthn: Webauthn,
    pending: moka::future::Cache<String, Arc<Pending>>,
    /// Cookies only over HTTPS
    secure: bool,
}

impl Passkeys {
    /// For the public address (`WF_PUBLIC_URL`), else for http://localhost:8080; the Vite dev
    /// server's address (`WF_DEV_UI`) is accepted too. None when the address is no URL with a
    /// host (passkeys then stay off and the API says so).
    pub fn new(public_url: Option<&str>, dev_ui: Option<&str>) -> Option<Self> {
        let origin = Url::parse(public_url.unwrap_or("http://localhost:8080")).ok()?;
        let rp_id = origin.host_str()?.to_owned();
        let mut builder = WebauthnBuilder::new(&rp_id, &origin)
            .ok()?
            .rp_name("Windfenster");
        if let Some(dev) = dev_ui.and_then(|d| Url::parse(d).ok()) {
            builder = builder.append_allowed_origin(&dev);
        }
        Some(Passkeys {
            webauthn: builder.build().ok()?,
            pending: moka::future::Cache::builder()
                .max_capacity(10_000)
                .time_to_live(Duration::from_secs(CEREMONY_S))
                .build(),
            secure: origin.scheme() == "https",
        })
    }

    async fn begin(&self, p: Pending) -> String {
        let id = hex::encode(random::<16>());
        self.pending.insert(id.clone(), Arc::new(p)).await;
        id
    }

    /// The ceremony's state, once: an answer cannot be replayed.
    async fn take(&self, id: &str) -> Option<Arc<Pending>> {
        self.pending.remove(id).await
    }
}

pub(crate) fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    openssl::rand::rand_bytes(&mut b).expect("the system's random source");
    b
}

pub(crate) fn hash(token: &str) -> [u8; 32] {
    openssl::sha::sha256(token.as_bytes())
}

/// The session token from the request's cookies
fn token(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|c| {
            c.trim()
                .strip_prefix(&format!("{COOKIE_NAME}="))
                .map(str::to_owned)
        })
}

fn cookie(value: &str, max_age: i64, secure: bool) -> HeaderValue {
    let secure = if secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{COOKIE_NAME}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}"
    ))
    .expect("a cookie of hex and ASCII")
}

/// A new session for the account: stored by its hash, the token goes to the browser.
async fn start_session(s: &AppState, account_id: i64) -> Result<HeaderMap, ApiError> {
    if account::disabled(&s.db, account_id).await? {
        return Err(ApiError::AccountDisabled);
    }
    let token = hex::encode(random::<32>());
    let now = wf_core::time::now();
    // Sessions that ran out go along the way: there are few, and a sign-in is rare.
    account::prune_sessions(&s.db, now).await?;
    account::start_session(&s.db, &hash(&token), account_id, now).await?;
    let secure = s.passkeys.as_ref().is_some_and(|p| p.secure);
    let mut h = HeaderMap::new();
    h.insert(SET_COOKIE, cookie(&token, account::SESSION_S, secure));
    Ok(h)
}

fn clear_cookie(s: &AppState) -> HeaderMap {
    let secure = s.passkeys.as_ref().is_some_and(|p| p.secure);
    let mut h = HeaderMap::new();
    h.insert(SET_COOKIE, cookie("", 0, secure));
    h
}

/// The signed-in account; a request without a valid session is refused (`signed_out`).
pub struct SignedIn(pub Account);

impl FromRequestParts<AppState> for SignedIn {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, s: &AppState) -> Result<Self, ApiError> {
        let token = token(&parts.headers).ok_or(ApiError::SignedOut)?;
        account::of_session(&s.db, &hash(&token), wf_core::time::now())
            .await?
            .map(SignedIn)
            .ok_or(ApiError::SignedOut)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_session_cookie() {
        let mut h = HeaderMap::new();
        h.insert(
            COOKIE,
            HeaderValue::from_static("a=1; wf_session=abc123; b=2"),
        );
        assert_eq!(token(&h).as_deref(), Some("abc123"));
        assert_eq!(token(&HeaderMap::new()), None);
        let c = cookie("abc", 60, true);
        assert_eq!(
            c.to_str().unwrap(),
            "wf_session=abc; Path=/; HttpOnly; SameSite=Lax; Max-Age=60; Secure"
        );
    }

    #[test]
    fn knows_its_address() {
        let p = Passkeys::new(Some("https://windfenster.example"), None).unwrap();
        assert!(p.secure);
        let dev = Passkeys::new(None, Some("http://localhost:5173")).unwrap();
        assert!(!dev.secure);
        assert!(Passkeys::new(Some("not a url"), None).is_none());
    }
}
