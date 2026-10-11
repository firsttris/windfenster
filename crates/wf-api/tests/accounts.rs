//! Accounts and favourites over HTTP. The passkey ceremonies' first steps are checked as the
//! browser gets them; a signed-in session is made directly in the database (a test cannot sign
//! with a passkey), with the cookie the browser would send.

mod common;

use anyhow::Result;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use serde_json::{Value, json};
use tower::ServiceExt;

const TOKEN: &str = "abc123";

async fn call(
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
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)?
    };
    Ok((status, value))
}

/// An account with a session for `TOKEN`, as signing in leaves it
async fn signed_in(dir: &tempfile::TempDir) -> Result<i64> {
    let db = wf_core::db::connect(&dir.path().join("test.db")).await?;
    let id = wf_core::account::create(&db, "uuid", "Tristan", "cred", "{}", 0).await?;
    let hash = token_hash(TOKEN);
    wf_core::account::start_session(&db, &hash, id, wf_core::time::now()).await?;
    Ok(id)
}

/// The SHA-256 the server keeps of `TOKEN` (`printf abc123 | sha256sum`)
fn token_hash(token: &str) -> Vec<u8> {
    assert_eq!(token, "abc123");
    hex_decode("6ca13d52ca70c883e0f0bb101e425a89e8624de51db2d2392593af6a84118090")
}

fn hex_decode(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

#[tokio::test]
async fn sign_up_asks_for_a_discoverable_passkey() -> Result<()> {
    let (_dir, app, _) = common::app().await?;
    let (status, body) = call(
        &app,
        "POST",
        "/api/auth/signup",
        Some(json!({"name": "  Tristan "})),
        false,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    let pk = &body["options"]["publicKey"];
    assert_eq!(pk["rp"]["id"], "localhost");
    assert_eq!(pk["user"]["name"], "Tristan");
    assert_eq!(pk["authenticatorSelection"]["residentKey"], "required");
    assert_eq!(pk["authenticatorSelection"]["userVerification"], "required");
    assert!(body["ceremony"].as_str().unwrap().len() == 32);

    // A name that is empty is refused; an answer to no ceremony, or a made-up one, too.
    let (status, body) = call(
        &app,
        "POST",
        "/api/auth/signup",
        Some(json!({"name": " "})),
        false,
    )
    .await?;
    assert_eq!(
        (status, body),
        (StatusCode::BAD_REQUEST, json!({"code": "invalid_name"}))
    );
    let answer = json!({"ceremony": "nope", "credential": {}});
    let (status, body) = call(&app, "POST", "/api/auth/signup/finish", Some(answer), false).await?;
    assert_eq!(
        (status, body),
        (StatusCode::BAD_REQUEST, json!({"code": "passkey_rejected"}))
    );
    Ok(())
}

#[tokio::test]
async fn sign_in_is_offered_by_autofill_or_asked_by_button() -> Result<()> {
    let (_dir, app, _) = common::app().await?;
    let (_, autofill) = call(
        &app,
        "POST",
        "/api/auth/signin",
        Some(json!({"autofill": true})),
        false,
    )
    .await?;
    assert_eq!(autofill["options"]["mediation"], "conditional");
    assert_eq!(
        autofill["options"]["publicKey"]["allowCredentials"],
        json!([])
    );
    let (_, button) = call(
        &app,
        "POST",
        "/api/auth/signin",
        Some(json!({"autofill": false})),
        false,
    )
    .await?;
    assert!(button["options"].get("mediation").is_none());
    // A ceremony answers once: its state is gone after the first try.
    let answer = json!({"ceremony": button["ceremony"], "credential": {"bogus": true}});
    let (status, _) = call(
        &app,
        "POST",
        "/api/auth/signin/finish",
        Some(answer.clone()),
        false,
    )
    .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    Ok(())
}

#[tokio::test]
async fn the_signed_in_user_and_signing_out() -> Result<()> {
    let (dir, app, _) = common::app().await?;
    let (status, body) = call(&app, "GET", "/api/me", None, false).await?;
    assert_eq!((status, body), (StatusCode::OK, Value::Null));
    let id = signed_in(&dir).await?;
    let (_, me) = call(&app, "GET", "/api/me", None, true).await?;
    assert_eq!(me, json!({"id": id, "name": "Tristan", "passkeys": 1}));

    let (status, me) = call(&app, "PATCH", "/api/me", Some(json!({"name": "T."})), true).await?;
    assert_eq!((status, me["name"].clone()), (StatusCode::OK, json!("T.")));

    let (status, _) = call(&app, "POST", "/api/auth/signout", None, true).await?;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, me) = call(&app, "GET", "/api/me", None, true).await?;
    assert_eq!(me, Value::Null);
    let (status, body) = call(&app, "GET", "/api/favorites", None, true).await?;
    assert_eq!(
        (status, body),
        (StatusCode::UNAUTHORIZED, json!({"code": "signed_out"}))
    );
    Ok(())
}

#[tokio::test]
async fn favourites_of_stations_and_points() -> Result<()> {
    let (dir, app, _) = common::app().await?;
    signed_in(&dir).await?;
    // A station takes its place from the station list.
    let station = json!({"slug": "edsb", "lat": 0, "lon": 0, "name": "Baggersee",
        "sector": {"from": 200, "to": 290}});
    let (status, a) = call(&app, "POST", "/api/favorites", Some(station), true).await?;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(
        (a["lat"].clone(), a["slug"].clone()),
        (json!(48.7794), json!("edsb"))
    );
    // A point lands on the 0.01° grid.
    let point =
        json!({"slug": null, "lat": 36.4123, "lon": 28.0876, "name": "Prasonisi", "sector": null});
    let (_, b) = call(&app, "POST", "/api/favorites", Some(point), true).await?;
    assert_eq!(
        (b["lat"].clone(), b["lon"].clone()),
        (json!(36.41), json!(28.09))
    );

    let bad = json!({"slug": null, "lat": 36.4, "lon": 28.1, "name": "x", "sector": {"from": 400, "to": 10}});
    let (status, body) = call(&app, "POST", "/api/favorites", Some(bad), true).await?;
    assert_eq!(
        (status, body),
        (StatusCode::BAD_REQUEST, json!({"code": "invalid_favorite"}))
    );
    let unknown = json!({"slug": "zzzz", "lat": 0, "lon": 0, "name": "x", "sector": null});
    let (status, _) = call(&app, "POST", "/api/favorites", Some(unknown), true).await?;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let order = json!({"ids": [b["id"], a["id"]]});
    let (status, _) = call(&app, "PUT", "/api/favorites/order", Some(order), true).await?;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, list) = call(&app, "GET", "/api/favorites", None, true).await?;
    assert_eq!(list[0]["name"], "Prasonisi");

    let uri = format!("/api/favorites/{}", a["id"]);
    let (_, changed) = call(
        &app,
        "PUT",
        &uri,
        Some(json!({"name": "Rhein", "sector": null})),
        true,
    )
    .await?;
    assert_eq!(
        (changed["name"].clone(), changed["sector"].clone()),
        (json!("Rhein"), Value::Null)
    );
    let (status, _) = call(&app, "DELETE", &uri, None, true).await?;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = call(&app, "DELETE", &uri, None, true).await?;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Deleting the account takes the favourites and the session along.
    let (status, _) = call(&app, "DELETE", "/api/me", None, true).await?;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = call(&app, "GET", "/api/favorites", None, true).await?;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    Ok(())
}
