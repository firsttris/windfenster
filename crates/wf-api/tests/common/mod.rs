//! Stand-ins for Open-Meteo (forecast and geocoding), aviationweather.gov and PEGELONLINE that
//! answer in their formats, and the API wired to them.
#![allow(dead_code)] // each test file uses part of it

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
pub struct Calls {
    pub forecast: AtomicUsize,
    pub metar: AtomicUsize,
    pub taf: AtomicUsize,
    pub gauge: AtomicUsize,
    pub metar_history: AtomicUsize,
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

/// The METAR API in its JSON shape: two reports of the last hour for `hours=24`; for the
/// ten days (`hours=240`) also every half hour of the UTC day three days back.
async fn fake_metar(
    State(calls): State<Arc<Calls>>,
    Query(q): Query<HashMap<String, String>>,
) -> String {
    assert_eq!(q["ids"], "EDSB");
    let now = wf_core::time::now();
    let half_hour = now - now % 1_800;
    let mut reports = vec![
        json!({"icaoId": "EDSB", "obsTime": half_hour, "temp": 13, "wdir": 210, "wspd": 9,
         "rawOb": "METAR EDSB 110850Z AUTO 21009KT CAVOK 13/10 Q1015"}),
        json!({"icaoId": "EDSB", "obsTime": half_hour - 1_800, "temp": 12, "wdir": "VRB", "wspd": 3,
         "wgst": null, "rawOb": "METAR EDSB 110820Z AUTO VRB03KT CAVOK 12/10 Q1015"}),
    ];
    match q["hours"].as_str() {
        "24" => calls.metar.fetch_add(1, Ordering::SeqCst),
        "240" => {
            let day = now - now % 86_400 - 3 * 86_400;
            reports.extend((0..48).map(|i| {
                json!({"icaoId": "EDSB", "obsTime": day + i * 1_800, "wdir": 230, "wspd": 12,
                    "wgst": 20, "rawOb": "METAR EDSB 080850Z 23012G20KT 9999 FEW030 14/08 Q1012"})
            }));
            calls.metar_history.fetch_add(1, Ordering::SeqCst)
        }
        other => panic!("hours={other}"),
    };
    json!(reports).to_string()
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

pub async fn app() -> Result<(tempfile::TempDir, Router, Arc<Calls>)> {
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

pub async fn get_json(app: &Router, uri: &str) -> Result<(StatusCode, Value)> {
    let res = app
        .clone()
        .oneshot(Request::get(uri).body(Body::empty())?)
        .await?;
    let status = res.status();
    let body = to_bytes(res.into_body(), 1 << 22).await?;
    Ok((status, serde_json::from_slice(&body)?))
}
