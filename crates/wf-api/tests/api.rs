use anyhow::Result;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;
use wf_api::{AppState, router};

async fn app() -> Result<(tempfile::TempDir, axum::Router)> {
    let dir = tempfile::tempdir()?;
    let db = wf_core::db::connect(&dir.path().join("test.db")).await?;
    wf_core::db::migrate(&db).await?;
    Ok((dir, router(AppState { db, dev_ui: None })))
}

async fn get(app: &axum::Router, uri: &str) -> Result<(StatusCode, Value)> {
    let res = app
        .clone()
        .oneshot(Request::get(uri).body(Body::empty())?)
        .await?;
    let status = res.status();
    let body = to_bytes(res.into_body(), 1 << 20).await?;
    Ok((status, serde_json::from_slice(&body)?))
}

#[tokio::test]
async fn health_reports_schema_and_counts() -> Result<()> {
    let (_dir, app) = app().await?;
    let (status, body) = get(&app, "/api/health").await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["version"], wf_api::VERSION);
    assert!(body["schemaVersion"].as_i64().unwrap() >= 1);
    assert_eq!(body["stations"], 0);
    assert_eq!(body["modelRuns"], 0);
    Ok(())
}

#[tokio::test]
async fn models_lists_the_model_table() -> Result<()> {
    let (_dir, app) = app().await?;
    let (status, body) = get(&app, "/api/models").await?;
    assert_eq!(status, StatusCode::OK);
    assert!(body.is_array());
    Ok(())
}

#[tokio::test]
async fn unknown_api_path_is_a_json_404() -> Result<()> {
    let (_dir, app) = app().await?;
    let (status, body) = get(&app, "/api/nope").await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "not found");
    Ok(())
}
