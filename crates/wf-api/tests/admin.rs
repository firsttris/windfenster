//! The admin page's API: only for admins, the last admin stays one, disabled accounts are out.

mod common;

use anyhow::Result;
use axum::http::StatusCode;
use common::session::{call, signed_in};
use serde_json::{Value, json};

async fn db(dir: &tempfile::TempDir) -> Result<wf_core::db::Db> {
    wf_core::db::connect(&dir.path().join("test.db")).await
}

#[tokio::test]
async fn only_admins_get_in() -> Result<()> {
    let (dir, app, _) = common::app().await?;
    let (status, body) = call(&app, "GET", "/api/admin/users", None, false).await?;
    assert_eq!(
        (status, body),
        (StatusCode::UNAUTHORIZED, json!({"code": "signed_out"}))
    );
    let me = signed_in(&dir).await?;
    let (status, body) = call(&app, "GET", "/api/admin/users", None, true).await?;
    assert_eq!(
        (status, body),
        (StatusCode::FORBIDDEN, json!({"code": "forbidden"}))
    );
    let (_, account) = call(&app, "GET", "/api/me", None, true).await?;
    assert_eq!(account["admin"], false);

    wf_core::admin::users::set_admin(&db(&dir).await?, me, true).await?;
    let (_, account) = call(&app, "GET", "/api/me", None, true).await?;
    assert_eq!(account["admin"], true);
    let (status, body) = call(&app, "GET", "/api/admin/users", None, true).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["summary"]["total"], 1);
    assert_eq!(body["users"][0]["name"], "Tristan");
    Ok(())
}

#[tokio::test]
async fn manages_accounts_and_logs_it() -> Result<()> {
    let (dir, app, _) = common::app().await?;
    let me = signed_in(&dir).await?;
    let db = db(&dir).await?;
    wf_core::admin::users::set_admin(&db, me, true).await?;
    let other = wf_core::account::create(&db, "u2", "Ben", "k2", "{}", 0).await?;
    wf_core::account::start_session(&db, b"other", other, wf_core::time::now()).await?;

    // The last admin keeps the role and cannot be disabled or deleted
    let (status, body) = call(
        &app,
        "PUT",
        &format!("/api/admin/users/{me}/role"),
        Some(json!({"admin": false})),
        true,
    )
    .await?;
    assert_eq!(
        (status, body),
        (StatusCode::CONFLICT, json!({"code": "last_admin"}))
    );
    let (status, _) = call(
        &app,
        "POST",
        &format!("/api/admin/users/{me}/disable"),
        None,
        true,
    )
    .await?;
    assert_eq!(status, StatusCode::CONFLICT);

    let (status, _) = call(
        &app,
        "POST",
        &format!("/api/admin/users/{other}/disable"),
        None,
        true,
    )
    .await?;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(
        wf_core::account::of_session(&db, b"other", wf_core::time::now())
            .await?
            .is_none()
    );
    let (_, d) = call(
        &app,
        "GET",
        &format!("/api/admin/users/{other}"),
        None,
        true,
    )
    .await?;
    assert!(d["user"]["disabledAt"].is_number());
    assert_eq!(d["sessions"], 0);
    let (status, _) = call(
        &app,
        "POST",
        &format!("/api/admin/users/{other}/enable"),
        None,
        true,
    )
    .await?;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, data) = call(
        &app,
        "GET",
        &format!("/api/admin/users/{other}/export"),
        None,
        true,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(data["account"]["name"], "Ben");

    let (status, _) = call(
        &app,
        "DELETE",
        &format!("/api/admin/users/{other}"),
        None,
        true,
    )
    .await?;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = call(
        &app,
        "GET",
        &format!("/api/admin/users/{other}"),
        None,
        true,
    )
    .await?;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (_, log) = call(&app, "GET", "/api/admin/log", None, true).await?;
    let actions: Vec<&str> = log
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e: &Value| e["action"].as_str())
        .collect();
    assert_eq!(
        actions,
        ["user.delete", "user.export", "user.enable", "user.disable"]
    );
    assert_eq!(log[0]["who"], "Tristan");
    assert_eq!(log[0]["target"], "Ben");
    Ok(())
}

#[tokio::test]
async fn the_overview_and_running_a_task() -> Result<()> {
    let (dir, app, _) = common::app().await?;
    let me = signed_in(&dir).await?;
    wf_core::admin::users::set_admin(&db(&dir).await?, me, true).await?;
    wf_core::status::board().register("test-overview", 60);
    let (status, o) = call(&app, "GET", "/api/admin/overview", None, true).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(o["users"]["admins"], 1);
    assert!(
        o["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["id"] == "test-overview")
    );
    assert!(o["budget"]["limit"].is_number());
    let (status, _) = call(
        &app,
        "POST",
        "/api/admin/tasks/test-overview/run",
        None,
        true,
    )
    .await?;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(wf_core::status::board().take("test-overview"));
    let (status, _) = call(&app, "POST", "/api/admin/tasks/nope/run", None, true).await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, lines) = call(&app, "GET", "/api/admin/log/lines", None, true).await?;
    assert_eq!(status, StatusCode::OK);
    assert!(lines.is_array());
    Ok(())
}

async fn admin_app(
    with: impl FnOnce(&std::path::Path, wf_api::AppState) -> wf_api::AppState,
) -> Result<(tempfile::TempDir, axum::Router)> {
    let (dir, app, _) = common::app_with(with).await?;
    let me = signed_in(&dir).await?;
    wf_core::admin::users::set_admin(&db(&dir).await?, me, true).await?;
    Ok((dir, app))
}

#[tokio::test]
async fn settings_change_and_stay() -> Result<()> {
    let (dir, app) = admin_app(|_, s| {
        let env = std::collections::BTreeMap::from([(
            "basemapZoom".to_owned(),
            (json!(8), "WF_BASEMAP_ZOOM".to_owned()),
        )]);
        let store = wf_core::admin::settings::SettingsStore::fresh(s.db.clone(), env).unwrap();
        s.with_settings(store)
    })
    .await?;
    let (status, body) = call(&app, "GET", "/api/admin/settings", None, true).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["settings"]["hotDays"], 7);
    assert_eq!(body["locked"]["basemapZoom"], "WF_BASEMAP_ZOOM");
    assert!(body["models"].as_array().unwrap().len() >= 5);

    let set = |key: &str, value: Value| {
        let app = app.clone();
        let uri = format!("/api/admin/settings/{key}");
        async move { call(&app, "PUT", &uri, Some(json!({ "value": value })), true).await }
    };
    let (status, body) = set("hotDays", json!(3)).await?;
    assert_eq!(
        (status, &body["settings"]["hotDays"]),
        (StatusCode::OK, &json!(3))
    );
    assert_eq!(
        set("hotDays", json!(0)).await?.1,
        json!({"code": "invalid_setting"})
    );
    assert_eq!(
        set("basemapZoom", json!(9)).await?.1,
        json!({"code": "invalid_setting"})
    );
    assert_eq!(
        set("models", json!(["nope"])).await?.1,
        json!({"code": "invalid_setting"})
    );
    let (status, body) = set("models", json!(["icon_d2"])).await?;
    assert_eq!(
        (status, &body["settings"]["models"]),
        (StatusCode::OK, &json!(["icon_d2"]))
    );
    assert_eq!(set("nope", json!(1)).await?.0, StatusCode::NOT_FOUND);
    // Stored: after a restart the same
    let again = wf_core::admin::settings::SettingsStore::load(
        db(&dir).await?,
        std::collections::BTreeMap::new(),
    )
    .await?;
    assert_eq!(again.get().hot_days, 3);
    Ok(())
}

#[tokio::test]
async fn forecasts_measurements_and_the_map() -> Result<()> {
    let (_dir, app) = admin_app(|dir, s| {
        let map = wf_map::MapData::new(dir.join("map"), reqwest::Client::new(), Default::default());
        s.with_map(std::sync::Arc::new(map))
    })
    .await?;
    let (status, f) = call(&app, "GET", "/api/admin/forecasts", None, true).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(f["days"].as_array().unwrap().len(), 14);
    assert!(f["runs"].as_array().unwrap().len() >= 5);
    let (status, m) = call(&app, "GET", "/api/admin/measurements", None, true).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(m["tracked"][0]["slug"], "edsb");

    let (status, map) = call(&app, "GET", "/api/admin/map", None, true).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(map["basemap"]["due"], "missing");
    let (status, _) = call(&app, "POST", "/api/admin/map/renew", None, true).await?;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, map) = call(&app, "GET", "/api/admin/map", None, true).await?;
    assert_eq!(map["basemap"]["approved"], true);
    let (status, _) = call(&app, "POST", "/api/admin/map/cancel", None, true).await?;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = call(
        &app,
        "POST",
        "/api/admin/map/postpone",
        Some(json!({"days": 30})),
        true,
    )
    .await?;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, st) = call(&app, "GET", "/api/admin/settings", None, true).await?;
    assert!(st["settings"]["basemapPostponedUntil"].as_i64().unwrap() > 0);
    let pin = json!({"lat": 48.78, "lon": 8.08});
    let (status, _) = call(
        &app,
        "PUT",
        "/api/admin/map/spots/pinned",
        Some(pin.clone()),
        true,
    )
    .await?;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, st) = call(&app, "GET", "/api/admin/settings", None, true).await?;
    assert_eq!(st["settings"]["pinnedSpots"], json!([[48.8, 8.0]]));
    let (status, _) = call(
        &app,
        "DELETE",
        "/api/admin/map/spots/pinned",
        Some(pin),
        true,
    )
    .await?;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = call(
        &app,
        "DELETE",
        "/api/admin/map/spots?lat=1&lon=1",
        None,
        true,
    )
    .await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, n) = call(&app, "POST", "/api/admin/map/leftovers", None, true).await?;
    assert_eq!((status, n), (StatusCode::OK, json!(0)));
    Ok(())
}

#[tokio::test]
async fn sign_up_open_by_invitation_or_closed() -> Result<()> {
    let (_dir, app) = admin_app(|_, s| {
        let store =
            wf_core::admin::settings::SettingsStore::fresh(s.db.clone(), Default::default())
                .unwrap();
        s.with_settings(store)
    })
    .await?;
    let signup = |invite: Option<&str>, email: &str| {
        let mut body = json!({"name": "Neu", "email": email, "password": "lang genug 123"});
        if let Some(i) = invite {
            body["invite"] = json!(i);
        }
        let app = app.clone();
        async move { call(&app, "POST", "/api/auth/signup/password", Some(body), false).await }
    };
    let (_, site) = call(&app, "GET", "/api/site", None, false).await?;
    assert_eq!(
        site,
        json!({"signup": "open", "banner": null, "imprint": false, "privacy": false})
    );
    assert_eq!(signup(None, "a@x.de").await?.0, StatusCode::OK);

    let set = |value: Value| {
        let app = app.clone();
        async move {
            call(
                &app,
                "PUT",
                "/api/admin/settings/signup",
                Some(json!({"value": value})),
                true,
            )
            .await
        }
    };
    set(json!("closed")).await?;
    assert_eq!(
        signup(None, "b@x.de").await?.1,
        json!({"code": "signup_closed"})
    );
    let (status, body) = call(
        &app,
        "POST",
        "/api/auth/signup",
        Some(json!({"name": "P"})),
        false,
    )
    .await?;
    assert_eq!(
        (status, body),
        (StatusCode::FORBIDDEN, json!({"code": "signup_closed"}))
    );

    set(json!("invite")).await?;
    assert_eq!(
        signup(None, "c@x.de").await?.1,
        json!({"code": "signup_closed"})
    );
    let (status, inv) = call(&app, "POST", "/api/admin/invites", None, true).await?;
    assert_eq!(status, StatusCode::OK);
    let link = inv["link"].as_str().unwrap();
    let token = link.rsplit("invite=").next().unwrap().to_owned();
    assert_eq!(
        signup(Some("nope"), "c@x.de").await?.1,
        json!({"code": "signup_closed"})
    );
    assert_eq!(signup(Some(&token), "c@x.de").await?.0, StatusCode::OK);
    // Once only
    assert_eq!(
        signup(Some(&token), "d@x.de").await?.1,
        json!({"code": "signup_closed"})
    );
    let (_, list) = call(&app, "GET", "/api/admin/invites", None, true).await?;
    assert_eq!(list[0]["usedBy"], "Neu");
    let (_, inv) = call(&app, "POST", "/api/admin/invites", None, true).await?;
    let (_, list) = call(&app, "GET", "/api/admin/invites", None, true).await?;
    let open = list
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["usedAt"].is_null())
        .unwrap();
    let id = open["id"].as_str().unwrap();
    let (status, _) = call(
        &app,
        "DELETE",
        &format!("/api/admin/invites/{id}"),
        None,
        true,
    )
    .await?;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let token = inv["link"]
        .as_str()
        .unwrap()
        .rsplit("invite=")
        .next()
        .unwrap()
        .to_owned();
    assert_eq!(
        signup(Some(&token), "e@x.de").await?.1,
        json!({"code": "signup_closed"})
    );
    Ok(())
}

#[tokio::test]
async fn the_site_says_the_banner_and_the_legal_pages() -> Result<()> {
    let (_dir, app) = admin_app(|_, s| {
        let store =
            wf_core::admin::settings::SettingsStore::fresh(s.db.clone(), Default::default())
                .unwrap();
        s.with_settings(store)
    })
    .await?;
    let (status, _) = call(&app, "GET", "/api/site/imprint", None, false).await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    for (key, value) in [
        ("banner", "Wartung Sonntag"),
        ("imprint", "# Impressum\n\nTristan"),
    ] {
        let uri = format!("/api/admin/settings/{key}");
        let (status, _) = call(&app, "PUT", &uri, Some(json!({"value": value})), true).await?;
        assert_eq!(status, StatusCode::OK);
    }
    let (_, site) = call(&app, "GET", "/api/site", None, false).await?;
    assert_eq!(
        (&site["banner"], &site["imprint"], &site["privacy"]),
        (&json!("Wartung Sonntag"), &json!(true), &json!(false))
    );
    let (_, text) = call(&app, "GET", "/api/site/imprint", None, false).await?;
    assert_eq!(text, "# Impressum\n\nTristan");
    Ok(())
}
