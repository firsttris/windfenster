//! Signing in with e-mail and password, next to passkeys: Argon2id hashes (PHC strings), at
//! most ten failed attempts per address in 15 minutes, and the same work done for an unknown
//! address as for a wrong password, so the answer's time does not tell which addresses exist.

use std::sync::LazyLock;
use std::time::Duration;

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use serde::Deserialize;
use ts_rs::TS;
use webauthn_rs::prelude::Uuid;
use wf_core::account::{self, Account};

use super::{SignedIn, random, start_session};
use crate::{ApiError, AppState};

/// Shortest password accepted, characters
const MIN_PASSWORD: usize = 10;
const MAX_PASSWORD: usize = 200;
/// Failed attempts per address before it waits
const MAX_FAILURES: u32 = 10;

/// Failed attempts per address, forgotten after 15 minutes.
pub type Failures = moka::future::Cache<String, u32>;

pub fn new_failures() -> Failures {
    moka::future::Cache::builder()
        .max_capacity(100_000)
        .time_to_live(Duration::from_secs(15 * 60))
        .build()
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct PasswordSignUp {
    pub name: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct PasswordSignIn {
    pub email: String,
    pub password: String,
}

/// A new address and password for the signed-in account; `current` is the password it has, if
/// it has one.
#[derive(Debug, Deserialize, TS)]
#[ts(export)]
pub struct PasswordChange {
    pub email: String,
    pub password: String,
    pub current: Option<String>,
}

fn strong_enough(p: &str) -> Result<(), ApiError> {
    let n = p.chars().count();
    if (MIN_PASSWORD..=MAX_PASSWORD).contains(&n) {
        Ok(())
    } else {
        Err(ApiError::WeakPassword)
    }
}

/// At most this many Argon2 hashes at once, each some 19 MiB and a core for a moment: a flood of
/// sign-ins with made-up addresses (the failure limit counts per address) waits in line instead
/// of taking every core and gigabytes of memory.
static HASHING: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);

async fn hash(password: String) -> Result<String, ApiError> {
    let _turn = HASHING.acquire().await.map_err(ApiError::internal)?;
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::encode_b64(&random::<16>()).expect("16 bytes of salt");
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
    })
    .await
    .map_err(ApiError::internal)?
    .map_err(ApiError::internal)
}

/// Checked against a stored hash, or against a stand-in when there is none (same work).
async fn verify(password: String, stored: Option<String>) -> Result<bool, ApiError> {
    static STAND_IN: LazyLock<String> = LazyLock::new(|| {
        let salt = SaltString::encode_b64(&random::<16>()).expect("16 bytes of salt");
        Argon2::default()
            .hash_password(b"no account has this password", &salt)
            .map(|h| h.to_string())
            .expect("hashing a constant")
    });
    let _turn = HASHING.acquire().await.map_err(ApiError::internal)?;
    tokio::task::spawn_blocking(move || {
        let known = stored.is_some();
        let phc = stored.unwrap_or_else(|| STAND_IN.clone());
        let ok = PasswordHash::new(&phc)
            .map(|h| {
                Argon2::default()
                    .verify_password(password.as_bytes(), &h)
                    .is_ok()
            })
            .unwrap_or(false);
        known && ok
    })
    .await
    .map_err(ApiError::internal)
}

/// `POST /api/auth/signup/password`: a new account with e-mail and password, signed in.
pub async fn sign_up(
    State(s): State<AppState>,
    Json(req): Json<PasswordSignUp>,
) -> Result<(HeaderMap, Json<Account>), ApiError> {
    let name = account::clean_name(&req.name).ok_or(ApiError::InvalidName)?;
    let email = account::clean_email(&req.email).ok_or(ApiError::InvalidEmail)?;
    strong_enough(&req.password)?;
    if account::email_taken(&s.db, &email, None).await? {
        return Err(ApiError::EmailTaken);
    }
    let hashed = hash(req.password).await?;
    let handle = Uuid::new_v4().to_string();
    let now = wf_core::time::now();
    let id = account::create_with_password(&s.db, &handle, &name, &email, &hashed, now)
        .await
        // Taken between the check and now
        .map_err(|_| ApiError::EmailTaken)?;
    let headers = start_session(&s, id).await?;
    let me = account::get(&s.db, id)
        .await?
        .ok_or_else(|| ApiError::internal("account just made is gone"))?;
    Ok((headers, Json(me)))
}

/// `POST /api/auth/signin/password`
pub async fn sign_in(
    State(s): State<AppState>,
    Json(req): Json<PasswordSignIn>,
) -> Result<(HeaderMap, Json<Account>), ApiError> {
    let email = account::clean_email(&req.email).ok_or(ApiError::WrongPassword)?;
    let failures = s.password_failures.get(&email).await.unwrap_or(0);
    if failures >= MAX_FAILURES {
        return Err(ApiError::TooManyAttempts);
    }
    let found = account::by_email(&s.db, &email).await?;
    let (id, stored) = match found {
        Some((id, stored)) => (Some(id), stored),
        None => (None, None),
    };
    let ok = verify(req.password, stored).await?;
    let Some(id) = id.filter(|_| ok) else {
        s.password_failures.insert(email, failures + 1).await;
        return Err(ApiError::WrongPassword);
    };
    s.password_failures.invalidate(&email).await;
    let headers = start_session(&s, id).await?;
    let me = account::get(&s.db, id)
        .await?
        .ok_or(ApiError::WrongPassword)?;
    Ok((headers, Json(me)))
}

/// `PUT /api/me/password`: sets or changes the address and password.
pub async fn change(
    State(s): State<AppState>,
    SignedIn(me): SignedIn,
    Json(req): Json<PasswordChange>,
) -> Result<Json<Account>, ApiError> {
    let email = account::clean_email(&req.email).ok_or(ApiError::InvalidEmail)?;
    strong_enough(&req.password)?;
    if let Some(stored) = account::password_hash(&s.db, me.id).await? {
        let current = req.current.unwrap_or_default();
        if !verify(current, Some(stored)).await? {
            return Err(ApiError::WrongPassword);
        }
    }
    if account::email_taken(&s.db, &email, Some(me.id)).await? {
        return Err(ApiError::EmailTaken);
    }
    let hashed = hash(req.password).await?;
    account::set_password(&s.db, me.id, &email, &hashed).await?;
    Ok(Json(Account {
        email: Some(email),
        ..me
    }))
}
