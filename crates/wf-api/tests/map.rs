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
    // A gauge read 20 minutes ago, one read last night
    let gauge = |uuid: &str, name: &str, at: i64| wf_core::gauge::Gauge {
        uuid: uuid.into(),
        number: "1".into(),
        name: name.into(),
        water: Some("Rhein".into()),
        km: None,
        lat: 48.85,
        lon: 8.12,
        level_cm: Some(312.0),
        level_at: Some(at),
        level_state: Some("high".into()),
        shipping_state: None,
        flow_m3s: None,
        flow_at: None,
        water_temp_c: None,
        water_temp_at: None,
    };
    wf_core::gauge::upsert(
        &db,
        &[
            gauge("a", "Iffezheim", *NOW - 1_200),
            gauge("b", "Maxau", *NOW - 12 * 3_600),
        ],
        *NOW,
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
    let (rgba, meta) = wf_map::texture::pixels(
        &grid(vec![5.0, -5.0]),
        &grid(vec![0.0, 2.0]),
        None,
        "icon_d2",
        R06,
        3,
    )?;
    let png = wf_map::texture::png(&rgba, meta.width, meta.height)?;
    wf_map::store::write_step(&root, "icon_d2", Some(&png), &rgba, &meta).await?;
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
        json!({"layers": [
            {"model": "icon_d2", "frames": [
                {"validAt": R06 + 3 * 3_600, "runAt": R06, "path": "icon_d2/2026101106/003"}
            ], "area": [9.75, 49.75, 10.75, 50.25], "inPieces": false},
            {"model": "icon_global", "frames": [], "area": null, "inPieces": true},
            {"model": "ecmwf_ifs025", "frames": [], "area": null, "inPieces": false}
        ], "basemap": null, "terrain": null})
    );

    // Without map data: no layers.
    let (_dir2, off) = app(false).await?;
    let (_, _, body) = get(&off, "/api/map").await?;
    assert_eq!(
        serde_json::from_slice::<Value>(&body)?,
        json!({"layers": [], "basemap": null, "terrain": null})
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

#[tokio::test]
async fn gauge_pins_carry_a_fresh_level() -> Result<()> {
    let (_dir, app) = app(true).await?;
    let (status, cache, body) = get(&app, "/api/map/gauges").await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cache.as_deref(), Some("public, max-age=60"));
    let pins: Value = serde_json::from_slice(&body)?;
    assert_eq!(
        pins,
        json!([
            {"uuid": "a", "name": "Iffezheim", "water": "Rhein", "lat": 48.85, "lon": 8.12,
                "levelCm": 312.0, "state": "high"},
            {"uuid": "b", "name": "Maxau", "water": "Rhein", "lat": 48.85, "lon": 8.12,
                "levelCm": null, "state": "unknown"}
        ])
    );
    Ok(())
}

/// A PMTiles v3 header: zooms 0 to 8, bounds 25° W to 45° E, 27° N to 72° N, then some bytes.
fn pmtiles() -> Vec<u8> {
    let mut b = vec![0u8; 127];
    b[..7].copy_from_slice(b"PMTiles");
    b[7] = 3;
    b[101] = 8;
    for (i, deg) in [-25.0f64, 27.0, 45.0, 72.0].into_iter().enumerate() {
        let at = 102 + i * 4;
        b[at..at + 4].copy_from_slice(&((deg * 1e7) as i32).to_le_bytes());
    }
    b.extend((0..2_000u32).map(|i| (i % 251) as u8));
    b
}

#[tokio::test]
async fn basemap_goes_out_in_ranges() -> Result<()> {
    let (dir, app) = app(true).await?;
    // None yet: the app falls back to its own.
    let (status, _, _) = get(&app, "/api/map/basemap.pmtiles").await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, _, body) = get(&app, "/api/map").await?;
    assert_eq!(
        serde_json::from_slice::<Value>(&body)?["basemap"],
        Value::Null
    );

    let file = pmtiles();
    std::fs::write(dir.path().join("map/basemap.pmtiles"), &file)?;
    let (_, _, body) = get(&app, "/api/map").await?;
    assert_eq!(
        serde_json::from_slice::<Value>(&body)?["basemap"],
        json!({"minZoom": 0, "maxZoom": 8, "bounds": [-25.0, 27.0, 45.0, 72.0], "fullZoom": 8, "region": null, "regionZoom": 8, "detail": [], "detailZoom": 8})
    );
    let res = app
        .clone()
        .oneshot(
            Request::get("/api/map/basemap.pmtiles")
                .header(header::RANGE, "bytes=100-1099")
                .header(header::ACCEPT_ENCODING, "gzip, br")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        res.headers()[header::CONTENT_TYPE],
        "application/vnd.pmtiles"
    );
    assert!(res.headers().get(header::CONTENT_ENCODING).is_none());
    assert!(
        res.headers().get(header::ETAG).is_some()
            || res.headers().get(header::LAST_MODIFIED).is_some()
    );
    let body = to_bytes(res.into_body(), 1 << 22).await?;
    assert_eq!(&body[..], &file[100..1100]);
    Ok(())
}

#[tokio::test]
async fn series_reads_a_place_over_the_hours() -> Result<()> {
    let (_dir, app) = app(true).await?;
    // The western grid point: 5 m/s towards the east
    let (status, _, body) = get(&app, "/api/map/series?lat=50&lon=10").await?;
    assert_eq!(status, StatusCode::OK);
    let v: Value = serde_json::from_slice(&body)?;
    let point = &v["layers"][0]["points"][0];
    assert_eq!(v["layers"][0]["model"], "icon_d2");
    assert_eq!(point["validAt"], R06 + 3 * 3_600);
    assert!((point["wind"].as_f64().unwrap() - 5.0 / 0.514_444).abs() < 0.15);
    assert!((point["dir"].as_f64().unwrap() - 270.0).abs() < 1.0);
    assert_eq!(point["gust"], Value::Null);
    // Outside the texture: no hours
    let (_, _, body) = get(&app, "/api/map/series?lat=40&lon=10").await?;
    assert_eq!(
        serde_json::from_slice::<Value>(&body)?["layers"][0]["points"],
        json!([])
    );
    let (status, _, _) = get(&app, "/api/map/series?lat=95&lon=10").await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    Ok(())
}

#[tokio::test]
async fn basemap_tiles_come_from_the_spots_or_from_above() -> Result<()> {
    let (dir, app) = app(true).await?;
    let map = dir.path().join("map");
    let world = wf_map::testing::vector_tile(&[(0, 0)]);
    std::fs::write(
        map.join("basemap.pmtiles"),
        wf_map::testing::archive(&[(0, 0, 0, &world), (8, 133, 88, b"rhine")]),
    )?;
    std::fs::create_dir_all(map.join("basemap-spots"))?;
    std::fs::write(
        map.join("basemap-spots/48.8_8.0.pmtiles"),
        wf_map::testing::archive(&[(14, 8559, 5641, b"runway")]),
    )?;
    let (status, cache, body) = get(&app, "/api/map/basemap/14/8559/5641").await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cache.as_deref(), Some("public, max-age=86400"));
    assert_eq!(body, b"runway");
    let (_, _, body) = get(&app, "/api/map/basemap/8/133/88").await?;
    assert_eq!(body, b"rhine");
    // Where nobody looked: the world's tile cut down, for a short while
    let (status, cache, body) = get(&app, "/api/map/basemap/14/0/0").await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cache.as_deref(), Some("public, max-age=600"));
    assert_eq!(wf_map::testing::points_in(&body), Some(vec![(0.0, 0.0)]));
    let (status, _, _) = get(&app, "/api/map/basemap/17/0/0").await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    Ok(())
}
