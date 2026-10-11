//! Health, models, search and forecasts, against the stand-ins in `common`.

mod common;

use anyhow::Result;
use axum::http::StatusCode;
use serde_json::{Value, json};
use std::sync::atomic::Ordering;

use common::{app, get_json};

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
    // The fake has values for every model it is asked for; the mix comes first.
    assert_eq!(fc["series"].as_array().unwrap().len(), 6);
    assert_eq!(fc["models"][0]["class"], "mix");
    assert_eq!(fc["series"][0]["model"], "mix");
    assert_eq!(fc["series"][0]["wind"], json!([10.0, 12.5, 15.0]));
    assert_eq!(fc["series"][1]["model"], "icon_d2");
    // Nothing verified yet
    assert_eq!(body["ranking"]["days"], 0);
    assert_eq!(body["ranking"]["scores"], json!([]));
    assert_eq!(body["ranking"]["bestSuper"], Value::Null);
    assert_eq!(calls.forecast.load(Ordering::SeqCst), 1);

    get_json(&app, "/api/stations/edsb").await?;
    assert_eq!(calls.forecast.load(Ordering::SeqCst), 1);

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
    // Within 5 km the point takes the station's ranking.
    assert_eq!(body["ranking"]["days"], 0);
    // Rhodes: only the global models, no station nearby.
    let (_, body) = get_json(&app, "/api/point?lat=36.4&lon=28.1").await?;
    assert_eq!(body["nearest"], Value::Null);
    assert_eq!(body["ranking"], Value::Null);
    let ids: Vec<&str> = body["forecast"]["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["mix", "ecmwf_ifs025", "gfs_global"]);

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
