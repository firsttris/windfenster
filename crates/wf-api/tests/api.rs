//! The API against stand-ins for Open-Meteo (forecast and geocoding), aviationweather.gov and
//! PEGELONLINE that answer in their formats.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Result;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::{Path, Query, State};
use axum::http::{Request, StatusCode};
use axum::routing::get;
use serde_json::{Value, json};
use tower::ServiceExt;
use wf_api::{AppState, router};
use wf_core::station::{NewStation, Station, StationSource};
use wf_ingest::forecasts::Forecasts;
use wf_ingest::geocode::Geocoder;
use wf_ingest::measurements::{Measurements, Urls};
use wf_ingest::openmeteo::{Config, OpenMeteo};

#[derive(Default)]
struct Calls {
    forecast: AtomicUsize,
    metar: AtomicUsize,
    taf: AtomicUsize,
    gauge: AtomicUsize,
}

async fn fake_forecast(
    State(calls): State<Arc<Calls>>,
    Query(q): Query<HashMap<String, String>>,
) -> String {
    calls.forecast.fetch_add(1, Ordering::SeqCst);
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

/// The METAR API with two reports of the last hour, in its JSON shape.
async fn fake_metar(
    State(calls): State<Arc<Calls>>,
    Query(q): Query<HashMap<String, String>>,
) -> String {
    calls.metar.fetch_add(1, Ordering::SeqCst);
    assert_eq!((q["ids"].as_str(), q["hours"].as_str()), ("EDSB", "24"));
    let now = wf_core::time::now();
    let half_hour = now - now % 1_800;
    json!([
        {"icaoId": "EDSB", "obsTime": half_hour, "temp": 13, "wdir": 210, "wspd": 9,
         "rawOb": "METAR EDSB 110850Z AUTO 21009KT CAVOK 13/10 Q1015"},
        {"icaoId": "EDSB", "obsTime": half_hour - 1_800, "temp": 12, "wdir": "VRB", "wspd": 3,
         "wgst": null, "rawOb": "METAR EDSB 110820Z AUTO VRB03KT CAVOK 12/10 Q1015"}
    ])
    .to_string()
}

/// The TAF API, valid from the last full hour for 24 hours, in its JSON shape.
async fn fake_taf(
    State(calls): State<Arc<Calls>>,
    Query(q): Query<HashMap<String, String>>,
) -> String {
    calls.taf.fetch_add(1, Ordering::SeqCst);
    assert_eq!(q["ids"], "EDSB");
    let now = wf_core::time::now();
    let from = now - now % 3_600;
    json!([{"icaoId": "EDSB", "issueTime": "2026-10-11T05:00:00.000Z",
    "validTimeFrom": from, "validTimeTo": from + 86_400,
    "rawTAF": "TAF EDSB 110500Z 1106/1206 22007KT CAVOK BECMG 1117/1120 VRB03KT",
    "fcsts": [
        {"timeFrom": from, "timeTo": from + 43_200, "timeBec": null, "fcstChange": null,
         "probability": null, "wdir": 220, "wspd": 7, "wgst": null, "visib": "6+",
         "wxString": "NSW", "clouds": [{"cover": "NSC", "base": null, "type": null}]},
        {"timeFrom": from + 43_200, "timeTo": from + 86_400, "timeBec": from + 54_000,
         "fcstChange": "BECMG", "probability": null, "wdir": "VRB", "wspd": 3, "wgst": null,
         "visib": "6+", "wxString": "NSW", "clouds": []}
    ]}])
    .to_string()
}

/// PEGELONLINE's level series: every 15 minutes of the last two hours, in its JSON shape.
async fn fake_levels(State(calls): State<Arc<Calls>>, Path(uuid): Path<String>) -> String {
    calls.gauge.fetch_add(1, Ordering::SeqCst);
    assert_eq!(uuid, "b02be240-1364-4c97-8bb6-675d7d842332");
    let now = wf_core::time::now();
    let last = now - now % 900;
    json!([
        {"timestamp": iso(last - last % 3_600), "value": 70.0},
        {"timestamp": iso(last - 900), "value": 63.0},
        {"timestamp": iso(last), "value": 62.0}
    ])
    .to_string()
}

/// A Unix time as PEGELONLINE writes it, in German summer time.
fn iso(unix: i64) -> String {
    let local = unix + 7_200;
    let d = wf_core::time::yyyymmdd(local);
    let s = local.rem_euclid(86_400);
    format!(
        "{}-{:02}-{:02}T{:02}:{:02}:00+02:00",
        d / 10_000,
        d / 100 % 100,
        d % 100,
        s / 3_600,
        s / 60 % 60
    )
}

async fn app() -> Result<(tempfile::TempDir, Router, Arc<Calls>)> {
    let calls = Arc::new(Calls::default());
    let fake = Router::new()
        .route("/v1/forecast", get(fake_forecast))
        .route("/v1/search", get(fake_geocode))
        .route("/api/data/metar", get(fake_metar))
        .route("/api/data/taf", get(fake_taf))
        .route("/stations/{uuid}/W/measurements.json", get(fake_levels))
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
                has_taf: true,
            },
            keywords: Some("Baden Airpark".into()),
        }],
        1,
    )
    .await?;
    wf_core::gauge::upsert(
        &db,
        &[wf_core::gauge::Gauge {
            uuid: "b02be240-1364-4c97-8bb6-675d7d842332".into(),
            number: "23500600".into(),
            name: "Iffezheim".into(),
            water: Some("Rhein".into()),
            km: Some(336.2),
            lat: 48.852149,
            lon: 8.116902,
            level_cm: Some(62.0),
            level_at: Some(1_791_710_100),
            level_state: Some("low".into()),
            shipping_state: Some("unknown".into()),
            flow_m3s: Some(358.0),
            flow_at: Some(1_791_709_200),
            water_temp_c: None,
            water_temp_at: None,
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
    let geocoder = Arc::new(Geocoder::new(http.clone(), Some(base.clone()), None));
    let measurements = Arc::new(Measurements::new(
        db.clone(),
        http,
        Urls {
            awc_base: base.clone(),
            dwd_base: base.clone(),
            pegelonline_base: base,
        },
    ));
    let state = AppState::new(
        db,
        forecasts,
        Some(geocoder),
        Some(measurements),
        None,
        None,
    );
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
async fn live_fetches_a_new_station_once() -> Result<()> {
    let (_dir, app, calls) = app().await?;
    let (status, body) = get_json(&app, "/api/stations/edsb/live").await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    let obs = body["observations"].as_array().unwrap();
    assert_eq!(obs.len(), 2);
    // Oldest first; variable wind has no direction.
    assert_eq!(
        (obs[0]["wind"].as_f64(), &obs[0]["dir"]),
        (Some(3.0), &Value::Null)
    );
    assert_eq!(obs[1]["wind"].as_f64(), Some(9.0));
    assert_eq!(obs[1]["dir"].as_f64(), Some(210.0));
    assert_eq!(obs[1]["temp"].as_f64(), Some(13.0));
    assert_eq!(
        obs[1]["raw"],
        "METAR EDSB 110850Z AUTO 21009KT CAVOK 13/10 Q1015"
    );
    assert_eq!(calls.metar.load(Ordering::SeqCst), 1);
    let taf = &body["taf"];
    assert_eq!(
        taf["raw"],
        "TAF EDSB 110500Z 1106/1206 22007KT CAVOK BECMG 1117/1120 VRB03KT"
    );
    assert_eq!(taf["groups"][1]["change"], "becoming");
    assert_eq!(taf["hours"].as_array().unwrap().len(), 24);
    assert_eq!(taf["hours"][0]["wind"].as_f64(), Some(7.0));
    assert_eq!(taf["hours"][23]["wind"].as_f64(), Some(3.0));
    assert_eq!(calls.taf.load(Ordering::SeqCst), 1);
    // The history still starts late, but the station was asked a moment ago.
    get_json(&app, "/api/stations/edsb/live").await?;
    assert_eq!(calls.metar.load(Ordering::SeqCst), 1);
    assert_eq!(calls.taf.load(Ordering::SeqCst), 1);

    let (status, _) = get_json(&app, "/api/stations/xxxx/live").await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    Ok(())
}

#[tokio::test]
async fn gauge_near_a_place() -> Result<()> {
    let (_dir, app, calls) = app().await?;
    // Baden Airpark: Iffezheim on the Rhine, 8 km
    let (status, body) = get_json(&app, "/api/gauges/near?lat=48.7794&lon=8.0805").await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        (&body["name"], &body["water"]),
        (&json!("Iffezheim"), &json!("Rhein"))
    );
    assert_eq!(body["distanceKm"].as_f64(), Some(8.5));
    assert_eq!(body["level"]["value"].as_f64(), Some(62.0));
    assert_eq!(
        (&body["levelState"], &body["shippingState"]),
        (&json!("low"), &json!("unknown"))
    );
    assert_eq!(body["flow"]["value"].as_f64(), Some(358.0));
    assert_eq!(body["waterTemp"], Value::Null);
    assert!((body["currentMs"].as_f64().unwrap() - 0.2586).abs() < 0.001);
    // The full hour and the newest value
    let history = body["history"].as_array().unwrap();
    assert_eq!(history.last().unwrap()["cm"].as_f64(), Some(62.0));
    assert_eq!(calls.gauge.load(Ordering::SeqCst), 1);
    get_json(&app, "/api/gauges/near?lat=48.78&lon=8.08").await?;
    assert_eq!(calls.gauge.load(Ordering::SeqCst), 1);

    // Rhodes: none
    let (status, body) = get_json(&app, "/api/gauges/near?lat=36.4&lon=28.1").await?;
    assert_eq!((status, body), (StatusCode::OK, Value::Null));
    let (status, _) = get_json(&app, "/api/gauges/near?lat=95&lon=8").await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
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
