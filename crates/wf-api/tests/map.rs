//! The wind map's index and textures, from a data directory with one hour written.

use std::sync::Arc;

use anyhow::Result;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use serde_json::{Value, json};
use tower::ServiceExt;
use wf_api::{AppState, router};
use wf_ingest::forecasts::Forecasts;
use wf_ingest::openmeteo::{Config, OpenMeteo};
use wf_map::MapData;
use wf_map::grid::LatLonGrid;

/// 2026-10-11 06 UTC
const R06: i64 = 1_791_698_400;

/// Measurement times are relative to the clock: pins show the last two hours.
static NOW: std::sync::LazyLock<i64> = std::sync::LazyLock::new(wf_core::time::now);

async fn app(with_map: bool) -> Result<(tempfile::TempDir, Router)> {
    let dir = tempfile::tempdir()?;
    let db = wf_core::db::connect(&dir.path().join("test.db")).await?;
    wf_core::db::migrate(&db).await?;
    let http = reqwest::Client::builder().no_proxy().build()?;
    let om = OpenMeteo::new(http.clone(), Config::default());
    let forecasts = Arc::new(Forecasts::new(db.clone(), om, vec![]));
    let station = wf_core::station::Station {
        slug: "edsb".into(),
        source: wf_core::station::StationSource::Metar,
        code: "EDSB".into(),
        name: "Baden Airpark".into(),
        country: Some("DE".into()),
        region: None,
        municipality: None,
        lat: 48.78,
        lon: 8.08,
        elevation_m: None,
        has_taf: true,
    };
    wf_core::station::upsert(
        &db,
        &[wf_core::station::NewStation {
            station,
            keywords: None,
        }],
        1,
    )
    .await?;
    let id = wf_core::station::id_of(&db, "edsb").await?.unwrap();
    let obs = |at: i64, wind| wf_core::observation::Observation {
        at,
        wind: Some(wind),
        gust: Some(22.0),
        dir: Some(240.0),
        temp: None,
        raw: None,
    };
    // Three hours ago is too old for a pin; ten minutes ago is the newest.
    wf_core::observation::store(
        &db,
        &[
            (id, obs(*NOW - 3 * 3_600, 5.0)),
            (id, obs(*NOW - 600, 12.0)),
        ],
    )
    .await?;
    let root = dir.path().join("map");
    let grid = |values| LatLonGrid {
        lon0: 10.0,
        lat0: 50.0,
        dlon: 0.5,
        dlat: 0.5,
        nx: 2,
        ny: 1,
        values,
    };
    let (png, meta) = wf_map::texture::encode(
        &grid(vec![5.0, -5.0]),
        &grid(vec![0.0, 2.0]),
        "icon_d2",
        R06,
        3,
    )?;
    wf_map::store::write_step(&root, "icon_d2", &png, &meta).await?;
    let mut state = AppState::new(db, forecasts, None, None, None, None);
    if with_map {
        state = state.with_map(Arc::new(MapData::new(root, http, wf_map::Urls::default())));
    }
    Ok((dir, router(state)))
}

async fn get(app: &Router, uri: &str) -> Result<(StatusCode, Option<String>, Vec<u8>)> {
    let res = app
        .clone()
        .oneshot(Request::get(uri).body(Body::empty())?)
        .await?;
    let status = res.status();
    let cache = res
        .headers()
        .get(header::CACHE_CONTROL)
        .map(|v| v.to_str().unwrap().to_owned());
    Ok((
        status,
        cache,
        to_bytes(res.into_body(), 1 << 22).await?.to_vec(),
    ))
}

#[tokio::test]
async fn index_lists_the_hours_on_disk() -> Result<()> {
    let (_dir, on) = app(true).await?;
    let (status, _, body) = get(&on, "/api/map").await?;
    assert_eq!(status, StatusCode::OK);
    let body: Value = serde_json::from_slice(&body)?;
    assert_eq!(
        body,
        json!({"layers": [{"model": "icon_d2", "frames": [
            {"validAt": R06 + 3 * 3_600, "runAt": R06, "path": "icon_d2/2026101106/003"}
        ]}]})
    );

    // Without map data: no layers.
    let (_dir2, off) = app(false).await?;
    let (_, _, body) = get(&off, "/api/map").await?;
    assert_eq!(
        serde_json::from_slice::<Value>(&body)?,
        json!({"layers": []})
    );
    Ok(())
}

#[tokio::test]
async fn textures_are_served_and_cached_for_good() -> Result<()> {
    let (_dir, app) = app(true).await?;
    let (status, cache, body) = get(&app, "/api/map/icon_d2/2026101106/003.png").await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        cache.as_deref(),
        Some("public, max-age=31536000, immutable")
    );
    assert_eq!(&body[1..4], b"PNG");
    let (status, _, body) = get(&app, "/api/map/icon_d2/2026101106/003.json").await?;
    assert_eq!(status, StatusCode::OK);
    let meta: Value = serde_json::from_slice(&body)?;
    assert_eq!(
        (meta["width"].as_u64(), meta["step"].as_u64()),
        (Some(2), Some(3))
    );

    for uri in [
        "/api/map/icon_d2/2026101106/004.png",
        "/api/map/icon_d2/2026101106/..%2F..%2Ftest.db",
        "/api/map/other/2026101106/003.png",
    ] {
        let (status, _, _) = get(&app, uri).await?;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
    }
    // A font range the app does not ship is a 404 for MapLibre, not the app's page.
    let (status, _, _) = get(&app, "/glyphs/Noto%20Sans%20Regular/65024-65279.pbf").await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    Ok(())
}

#[tokio::test]
async fn pins_carry_the_newest_measurement() -> Result<()> {
    let (_dir, app) = app(true).await?;
    let (status, cache, body) = get(&app, "/api/map/stations").await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cache.as_deref(), Some("public, max-age=60"));
    let pins: Value = serde_json::from_slice(&body)?;
    assert_eq!(
        pins,
        json!([{"slug": "edsb", "source": "metar", "code": "EDSB", "name": "Baden Airpark",
            "lat": 48.78, "lon": 8.08, "at": *NOW - 600, "wind": 12.0, "gust": 22.0, "dir": 240.0}])
    );
    Ok(())
}
