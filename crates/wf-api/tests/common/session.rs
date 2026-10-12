//! Requests as a signed-in browser makes them: a session made directly in the database (a test
//! cannot sign with a passkey) and the cookie the browser would send.

use anyhow::Result;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use serde_json::Value;
use tower::ServiceExt;

pub const TOKEN: &str = "abc123";

pub async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    signed_in: bool,
) -> Result<(StatusCode, Value)> {
    let mut req = Request::builder().method(method).uri(uri);
    if signed_in {
        req = req.header(header::COOKIE, format!("wf_session={TOKEN}"));
    }
    let req = match body {
        Some(b) => req
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(b.to_string()))?,
        None => req.body(Body::empty())?,
    };
    let res = app.clone().oneshot(req).await?;
    let status = res.status();
    let bytes = to_bytes(res.into_body(), 1 << 20).await?;
    // axum's own rejections (a body it cannot read) are plain text
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()))
    };
    Ok((status, value))
}

/// An account with a session for `TOKEN`, as signing in leaves it
pub async fn signed_in(dir: &tempfile::TempDir) -> Result<i64> {
    let db = wf_core::db::connect(&dir.path().join("test.db")).await?;
    let id = wf_core::account::create(&db, "uuid", "Tristan", "cred", "{}", 0).await?;
    let hash = token_hash(TOKEN);
    wf_core::account::start_session(&db, &hash, id, wf_core::time::now()).await?;
    Ok(id)
}

/// The SHA-256 the server keeps of `TOKEN` (`printf abc123 | sha256sum`)
pub fn token_hash(token: &str) -> Vec<u8> {
    assert_eq!(token, "abc123");
    hex_decode("6ca13d52ca70c883e0f0bb101e425a89e8624de51db2d2392593af6a84118090")
}

pub fn hex_decode(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}
