//! The API against a stand-in for Open-Meteo (forecast and geocoding) that answers in its format.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Result;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::{Query, State};
use axum::http::{Request, StatusCode};
use axum::routing::get;
use serde_json::{Value, json};
use tower::ServiceExt;
use wf_api::{AppState, router};
use wf_core::station::{NewStation, Station, StationSource};
use wf_ingest::forecasts::Forecasts;
use wf_ingest::geocode::Geocoder;
use wf_ingest::openmeteo::{Config, OpenMeteo};

async fn fake_forecast(
    State(calls): State<Arc<AtomicUsize>>,
    Query(q): Query<HashMap<String, String>>,
) -> String {
    calls.fetch_add(1, Ordering::SeqCst);
    let models: Vec<&str> = q["models"].split(',').collect();
    let mut hourly = serde_json::Map::new();
    hourly.insert("time".into(), json!([1791626400, 1791630000, 1791633600]));
    for m in &models {
        for var in wf_ingest::openmeteo::HOURLY {
            let key = if models.len() == 1 {
                var.to_owned()
            } else {
                format!("{var}_{m}")
            };
            hourly.insert(key, json!([10.0, 12.5, 15.0]));
        }
    }
    json!({"timezone": "Europe/Berlin", "elevation": 120.0, "hourly": hourly}).to_string()
}

async fn fake_geocode(Query(q): Query<HashMap<String, String>>) -> String {
    if q["name"].to_lowercase().starts_with("rhein") {
        json!({"results": [{"name": "Rheinmünster", "latitude": 48.77, "longitude": 8.05,
            "country_code": "DE", "admin1": "Baden-Württemberg"}]})
        .to_string()
    } else {
        json!({"generationtime_ms": 0.1}).to_string()
    }
}

async fn app() -> Result<(tempfile::TempDir, Router, Arc<AtomicUsize>)> {
    let calls = Arc::new(AtomicUsize::new(0));
    let fake = Router::new()
        .route("/v1/forecast", get(fake_forecast))
        .route("/v1/search", get(fake_geocode))
        .with_state(calls.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let base = format!("http://{}", listener.local_addr()?);
    tokio::spawn(async move { axum::serve(listener, fake).await });

    let dir = tempfile::tempdir()?;
    let db = wf_core::db::connect(&dir.path().join("test.db")).await?;
    wf_core::db::migrate(&db).await?;
    wf_core::station::upsert(
        &db,
        &[NewStation {
            station: Station {
                slug: "edsb".into(),
                source: StationSource::Metar,
                code: "EDSB".into(),
                name: "Karlsruhe Baden-Baden Airport".into(),
                country: Some("DE".into()),
                region: Some("Baden-Württemberg".into()),
                municipality: Some("Rheinmünster".into()),
                lat: 48.7794,
                lon: 8.0805,
                elevation_m: Some(124.4),
            },
            keywords: Some("Baden Airpark".into()),
        }],
        1,
    )
    .await?;
    let http = reqwest::Client::builder().no_proxy().build()?;
    let models = wf_core::model::active(&db, None).await?;
    let om = OpenMeteo::new(
        http.clone(),
        Config {
            base_url: Some(base.clone()),
            ..Config::default()
        },
    );
    let forecasts = Arc::new(Forecasts::new(db.clone(), om, models));
    let geocoder = Arc::new(Geocoder::new(http, Some(base), None));
    let state = AppState::new(db, forecasts, Some(geocoder), None, None);
    Ok((dir, router(state), calls))
}

async fn get_json(app: &Router, uri: &str) -> Result<(StatusCode, Value)> {
    let res = app
        .clone()
        .oneshot(Request::get(uri).body(Body::empty())?)
        .await?;
    let status = res.status();
    let body = to_bytes(res.into_body(), 1 << 22).await?;
    Ok((status, serde_json::from_slice(&body)?))
}

#[tokio::test]
async fn health_and_models() -> Result<()> {
    let (_dir, app, _) = app().await?;
    let (status, body) = get_json(&app, "/api/health").await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["version"], wf_api::VERSION);
    assert!(body["schemaVersion"].as_i64().unwrap() >= 2);
    assert_eq!(body["stations"], 1);
    assert_eq!(body["models"], 5);

    let (_, models) = get_json(&app, "/api/models").await?;
    let ids: Vec<&str> = models
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids[0], "icon_d2");
    assert_eq!(ids.len(), 5);
    assert!(models[0].get("metaId").is_none());
    Ok(())
}

#[tokio::test]
async fn search_finds_stations_and_places() -> Result<()> {
    let (_dir, app, _) = app().await?;
    let (status, body) = get_json(&app, "/api/stations?q=rheinm&lang=de").await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["stations"][0]["slug"], "edsb");
    assert_eq!(body["places"][0]["name"], "Rheinmünster");
    let (_, body) = get_json(&app, "/api/stations?q=airpark").await?;
    assert_eq!(body["stations"][0]["code"], "EDSB");
    assert_eq!(body["places"], json!([]));
    let (_, body) = get_json(&app, "/api/stations?q=e").await?;
    assert_eq!(body, json!({"stations": [], "places": []}));
    Ok(())
}

#[tokio::test]
async fn station_forecast_is_fetched_once_then_cached() -> Result<()> {
    let (_dir, app, calls) = app().await?;
    let (status, body) = get_json(&app, "/api/stations/edsb").await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["station"]["code"], "EDSB");
    let fc = &body["forecast"];
    assert_eq!(
        (fc["lat"].as_f64(), fc["lon"].as_f64()),
        (Some(48.78), Some(8.08))
    );
    assert_eq!(fc["timezone"], "Europe/Berlin");
    assert_eq!(fc["notice"], Value::Null);
    // The fake has values for every model it is asked for.
    assert_eq!(fc["series"].as_array().unwrap().len(), 5);
    assert_eq!(fc["series"][0]["model"], "icon_d2");
    assert_eq!(fc["series"][0]["wind"], json!([10.0, 12.5, 15.0]));
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    get_json(&app, "/api/stations/edsb").await?;
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let (status, body) = get_json(&app, "/api/stations/xxxx").await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({"code": "not_found"}));
    Ok(())
}

#[tokio::test]
async fn point_forecast_names_the_nearest_station() -> Result<()> {
    let (_dir, app, _) = app().await?;
    let (status, body) = get_json(&app, "/api/point?lat=48.80&lon=8.12").await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["nearest"]["station"]["slug"], "edsb");
    assert!(body["nearest"]["distanceKm"].as_f64().unwrap() < 5.0);
    // Rhodes: only the global models, no station nearby.
    let (_, body) = get_json(&app, "/api/point?lat=36.4&lon=28.1").await?;
    assert_eq!(body["nearest"], Value::Null);
    let ids: Vec<&str> = body["forecast"]["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["ecmwf_ifs025", "gfs_global"]);

    for bad in [
        "/api/point?lat=95&lon=8",
        "/api/point?lat=abc&lon=8",
        "/api/point",
    ] {
        let (status, body) = get_json(&app, bad).await?;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad}");
        assert_eq!(body, json!({"code": "invalid_coordinates"}));
    }
    Ok(())
}

#[tokio::test]
async fn unknown_api_path_is_a_json_404() -> Result<()> {
    let (_dir, app, _) = app().await?;
    let (status, body) = get_json(&app, "/api/nope").await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({"code": "not_found"}));
    Ok(())
}
