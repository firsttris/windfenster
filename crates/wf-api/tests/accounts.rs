//! Accounts and favourites over HTTP. The passkey ceremonies' first steps are checked as the
//! browser gets them; a signed-in session is made directly in the database (a test cannot sign
//! with a passkey), with the cookie the browser would send.

mod common;

use anyhow::Result;
use axum::http::StatusCode;
use common::session::{call, signed_in};
use serde_json::{Value, json};

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
    assert_eq!(
        me,
        json!({"id": id, "name": "Tristan", "email": null, "emailConfirmed": false, "passkeys": 1, "admin": false})
    );

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
async fn passkeys_are_listed_and_removed_but_not_the_last_way_in() -> Result<()> {
    let (dir, app, _) = common::app().await?;
    let id = signed_in(&dir).await?;
    let db = wf_core::db::connect(&dir.path().join("test.db")).await?;
    wf_core::account::add_passkey(&db, id, "cred-2", "{}", 50).await?;
    let (status, list) = call(&app, "GET", "/api/me/passkeys", None, true).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        list,
        json!([
            {"id": "cred", "name": null, "browser": null, "os": null, "createdAt": 0, "lastUsedAt": null},
            {"id": "cred-2", "name": null, "browser": null, "os": null, "createdAt": 50, "lastUsedAt": null},
        ])
    );

    // A name of the user's own, too long, and back to the device
    let (status, k) = call(
        &app,
        "PATCH",
        "/api/me/passkeys/cred-2",
        Some(json!({"name": "  Pixel  8 "})),
        true,
    )
    .await?;
    assert_eq!(
        (status, k["name"].clone()),
        (StatusCode::OK, json!("Pixel 8"))
    );
    let long = json!({"name": "x".repeat(61)});
    let (status, body) = call(&app, "PATCH", "/api/me/passkeys/cred-2", Some(long), true).await?;
    assert_eq!(
        (status, body),
        (StatusCode::BAD_REQUEST, json!({"code": "invalid_name"}))
    );
    let (_, k) = call(
        &app,
        "PATCH",
        "/api/me/passkeys/cred-2",
        Some(json!({"name": " "})),
        true,
    )
    .await?;
    assert_eq!(k["name"], Value::Null);
    let (status, _) = call(
        &app,
        "PATCH",
        "/api/me/passkeys/nope",
        Some(json!({"name": "x"})),
        true,
    )
    .await?;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, me) = call(&app, "DELETE", "/api/me/passkeys/cred", None, true).await?;
    assert_eq!((status, me["passkeys"].clone()), (StatusCode::OK, json!(1)));
    // Gone already, and the last one stays while the account has no password.
    let (status, body) = call(&app, "DELETE", "/api/me/passkeys/cred", None, true).await?;
    assert_eq!(
        (status, body),
        (StatusCode::NOT_FOUND, json!({"code": "not_found"}))
    );
    let (status, body) = call(&app, "DELETE", "/api/me/passkeys/cred-2", None, true).await?;
    assert_eq!(
        (status, body),
        (StatusCode::CONFLICT, json!({"code": "last_sign_in"}))
    );
    // Signed out, nothing is listed or removed.
    let (status, _) = call(&app, "GET", "/api/me/passkeys", None, false).await?;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = call(&app, "DELETE", "/api/me/passkeys/cred-2", None, false).await?;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    Ok(())
}

#[tokio::test]
async fn signing_out_on_every_other_device() -> Result<()> {
    let (dir, app, _) = common::app().await?;
    let id = signed_in(&dir).await?;
    let db = wf_core::db::connect(&dir.path().join("test.db")).await?;
    let now = wf_core::time::now();
    wf_core::account::start_session(&db, b"phone", id, now).await?;
    wf_core::account::start_session(&db, b"laptop", id, now).await?;
    let (status, body) = call(&app, "DELETE", "/api/me/sessions", None, true).await?;
    assert_eq!((status, body), (StatusCode::OK, json!({"ended": 2})));
    // This one stays signed in, the others are out.
    let (_, me) = call(&app, "GET", "/api/me", None, true).await?;
    assert_eq!(me["id"], json!(id));
    assert!(
        wf_core::account::of_session(&db, b"phone", now)
            .await?
            .is_none()
    );
    let (_, body) = call(&app, "DELETE", "/api/me/sessions", None, true).await?;
    assert_eq!(body, json!({"ended": 0}));
    let (status, _) = call(&app, "DELETE", "/api/me/sessions", None, false).await?;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
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
    // The model the forecast opens with: left out is automatic, the mix or a known model ...
    assert_eq!(changed["model"], Value::Null);
    for model in ["mix", "icon_d2"] {
        let body = json!({"name": "Rhein", "sector": null, "model": model});
        let (_, changed) = call(&app, "PUT", &uri, Some(body), true).await?;
        assert_eq!(changed["model"], json!(model));
    }
    // ... and no other
    let body = json!({"name": "Rhein", "sector": null, "model": "nope"});
    let (status, body) = call(&app, "PUT", &uri, Some(body), true).await?;
    assert_eq!(
        (status, body),
        (StatusCode::BAD_REQUEST, json!({"code": "invalid_favorite"}))
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

#[tokio::test]
async fn sign_up_and_in_with_e_mail_and_password() -> Result<()> {
    let (_dir, app, _) = common::app().await?;
    let new = json!({"name": "Tris", "email": " Tris@Example.org", "password": "long enough!"});
    let (status, me) = call(
        &app,
        "POST",
        "/api/auth/signup/password",
        Some(new.clone()),
        false,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        (me["email"].clone(), me["passkeys"].clone()),
        (json!("tris@example.org"), json!(0))
    );
    let (status, body) = call(&app, "POST", "/api/auth/signup/password", Some(new), false).await?;
    assert_eq!(
        (status, body),
        (StatusCode::CONFLICT, json!({"code": "email_taken"}))
    );
    let short = json!({"name": "X", "email": "x@y.de", "password": "short"});
    let (_, body) = call(
        &app,
        "POST",
        "/api/auth/signup/password",
        Some(short),
        false,
    )
    .await?;
    assert_eq!(body, json!({"code": "weak_password"}));

    let right = json!({"email": "tris@example.org", "password": "long enough!"});
    let (status, me) = call(
        &app,
        "POST",
        "/api/auth/signin/password",
        Some(right.clone()),
        false,
    )
    .await?;
    assert_eq!(
        (status, me["name"].clone()),
        (StatusCode::OK, json!("Tris"))
    );
    // A wrong password and an unknown address answer alike.
    let wrong = json!({"email": "tris@example.org", "password": "not the one"});
    let unknown = json!({"email": "nobody@example.org", "password": "long enough!"});
    for req in [wrong.clone(), unknown] {
        let (status, body) =
            call(&app, "POST", "/api/auth/signin/password", Some(req), false).await?;
        assert_eq!(
            (status, body),
            (StatusCode::UNAUTHORIZED, json!({"code": "wrong_password"}))
        );
    }
    // Ten failures for an address and it waits, even for the right password.
    for _ in 0..9 {
        call(
            &app,
            "POST",
            "/api/auth/signin/password",
            Some(wrong.clone()),
            false,
        )
        .await?;
    }
    let (status, body) = call(
        &app,
        "POST",
        "/api/auth/signin/password",
        Some(right),
        false,
    )
    .await?;
    assert_eq!(
        (status, body),
        (
            StatusCode::TOO_MANY_REQUESTS,
            json!({"code": "too_many_attempts"})
        )
    );
    Ok(())
}

#[tokio::test]
async fn a_passkey_account_adds_a_password() -> Result<()> {
    let (dir, app, _) = common::app().await?;
    signed_in(&dir).await?;
    let set = json!({"email": "t@x.de", "password": "long enough!", "current": null});
    let (status, me) = call(&app, "PUT", "/api/me/password", Some(set), true).await?;
    assert_eq!(
        (status, me["email"].clone()),
        (StatusCode::OK, json!("t@x.de"))
    );
    // Changing it asks for the one it has.
    let change =
        json!({"email": "t@x.de", "password": "another long one", "current": "wrong one!"});
    let (status, _) = call(&app, "PUT", "/api/me/password", Some(change), true).await?;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let change =
        json!({"email": "t@x.de", "password": "another long one", "current": "long enough!"});
    let (status, _) = call(&app, "PUT", "/api/me/password", Some(change), true).await?;
    assert_eq!(status, StatusCode::OK);
    let signin = json!({"email": "t@x.de", "password": "another long one"});
    let (status, _) = call(
        &app,
        "POST",
        "/api/auth/signin/password",
        Some(signin),
        false,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    Ok(())
}

#[tokio::test]
async fn an_address_is_confirmed_by_its_link() -> Result<()> {
    let (dir, app, _) = common::app().await?;
    let id = signed_in(&dir).await?;
    // Without an address nothing is sent; without e-mail set up neither.
    let (status, body) = call(&app, "POST", "/api/me/email/confirm", None, true).await?;
    assert_eq!(
        (status, body),
        (StatusCode::BAD_REQUEST, json!({"code": "invalid_email"}))
    );
    let set = json!({"email": "t@x.de", "password": "long enough!", "current": null});
    let (_, me) = call(&app, "PUT", "/api/me/password", Some(set), true).await?;
    assert_eq!(me["emailConfirmed"], json!(false));
    let (status, body) = call(&app, "POST", "/api/me/email/confirm", None, true).await?;
    assert_eq!(
        (status, body),
        (
            StatusCode::SERVICE_UNAVAILABLE,
            json!({"code": "email_off"})
        )
    );
    // A made-up link confirms nothing; the one sent does, once, signed in or not.
    let made_up = json!({"token": "nope"});
    let (status, body) = call(
        &app,
        "POST",
        "/api/auth/email/confirm",
        Some(made_up),
        false,
    )
    .await?;
    assert_eq!(
        (status, body),
        (StatusCode::BAD_REQUEST, json!({"code": "invalid_link"}))
    );
    let db = wf_core::db::connect(&dir.path().join("test.db")).await?;
    let now = wf_core::time::now();
    let hash = common::session::token_hash("abc123");
    wf_core::account::add_email_confirmation(&db, &hash, id, "t@x.de", now, now + 60).await?;
    let link = json!({"token": "abc123"});
    let (status, _) = call(
        &app,
        "POST",
        "/api/auth/email/confirm",
        Some(link.clone()),
        false,
    )
    .await?;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, me) = call(&app, "GET", "/api/me", None, true).await?;
    assert_eq!(me["emailConfirmed"], json!(true));
    let (status, _) = call(&app, "POST", "/api/auth/email/confirm", Some(link), false).await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    // Asking again for a confirmed address sends nothing and is fine.
    let (status, _) = call(&app, "POST", "/api/me/email/confirm", None, true).await?;
    assert_eq!(status, StatusCode::NO_CONTENT);
    // A new address is unconfirmed again.
    let change =
        json!({"email": "new@x.de", "password": "long enough!", "current": "long enough!"});
    let (_, me) = call(&app, "PUT", "/api/me/password", Some(change), true).await?;
    assert_eq!(me["emailConfirmed"], json!(false));
    Ok(())
}

#[tokio::test]
async fn gear_is_kept_as_a_list() -> Result<()> {
    let (dir, app, _) = common::app().await?;
    let (status, _) = call(&app, "GET", "/api/gear", None, true).await?;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    signed_in(&dir).await?;
    let list = json!([
        {"kind": "wing", "size": 4.0, "minKn": 14.0, "maxKn": 30.0},
        {"kind": "kite", "size": 12.0, "minKn": 13.0, "maxKn": 20.0}
    ]);
    let (status, _) = call(&app, "PUT", "/api/gear", Some(list.clone()), true).await?;
    assert_eq!(status, StatusCode::OK);
    let (_, got) = call(&app, "GET", "/api/gear", None, true).await?;
    assert_eq!(got, list);
    let bad = json!([{"kind": "wing", "size": 4.0, "minKn": 20.0, "maxKn": 10.0}]);
    let (status, body) = call(&app, "PUT", "/api/gear", Some(bad), true).await?;
    assert_eq!(
        (status, body),
        (StatusCode::BAD_REQUEST, json!({"code": "invalid_gear"}))
    );
    let (status, _) = call(
        &app,
        "PUT",
        "/api/gear",
        Some(json!([{"kind": "boat", "size": 1, "minKn": 1, "maxKn": 2}])),
        true,
    )
    .await?;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    Ok(())
}
