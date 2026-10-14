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
