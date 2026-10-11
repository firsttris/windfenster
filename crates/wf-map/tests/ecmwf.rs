//! The ECMWF pipeline against a stand-in for data.ecmwf.int: an index per step, the step's
//! GRIB2 file answering byte ranges, steps out up to a point.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::get;
use wf_map::{MapData, Urls, ecmwf, testing};

/// 2026-10-11 00 UTC
const R00: i64 = 1_791_676_800;

#[derive(Clone, Default)]
struct Fake {
    /// Steps out so far: 0 up to this one
    last: Arc<AtomicU32>,
    ranges: Arc<AtomicUsize>,
}

/// U and V on a 1° grid from 30°W to 50°E and 75°N to 25°N, north row first as ECMWF sends it;
/// U grows to the east, V is 4 everywhere.
fn file() -> (Vec<u8>, String) {
    let (ni, nj) = (81, 51);
    let grid = |f: &dyn Fn(u32) -> u8| {
        testing::message(
            ni,
            nj,
            (75_000_000, 330_000_000),
            (25_000_000, 50_000_000),
            1_000_000,
            0x00,
            &(0..ni * nj).map(|k| Some(f(k % ni))).collect::<Vec<_>>(),
        )
    };
    let (u, v) = (grid(&|i| (i / 10) as u8), grid(&|_| 4));
    let line = |param: &str, offset: usize, length: usize| {
        format!(
            r#"{{"domain": "g", "type": "fc", "stream": "oper", "levtype": "sfc", "param": "{param}", "_offset": {offset}, "_length": {length}}}"#
        )
    };
    let index = [
        line("tp", 0, 10),
        line("10u", 10, u.len()),
        line("10v", 10 + u.len(), v.len()),
    ]
    .join("\n");
    ([vec![0; 10], u, v].concat(), index)
}

async fn serve(
    State(fake): State<Fake>,
    Path((date, hour, name)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> axum::response::Response {
    // Only the 00 UTC run is out.
    if (date.as_str(), hour.as_str()) != ("20261011", "00z") {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Some(rest) = name.strip_prefix("20261011000000-") else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (step, ext) = rest.split_once("h-oper-fc.").unwrap();
    if step.parse::<u32>().unwrap() > fake.last.load(Ordering::SeqCst) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let (bytes, index) = file();
    match ext {
        "index" => index.into_response(),
        "grib2" => {
            fake.ranges.fetch_add(1, Ordering::SeqCst);
            let range = headers[header::RANGE].to_str().unwrap();
            let (a, b) = range
                .strip_prefix("bytes=")
                .unwrap()
                .split_once('-')
                .unwrap();
            let (a, b): (usize, usize) = (a.parse().unwrap(), b.parse().unwrap());
            (StatusCode::PARTIAL_CONTENT, bytes[a..=b].to_vec()).into_response()
        }
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

#[tokio::test]
async fn fetches_the_wind_by_range_and_cuts_it_to_the_region() {
    let fake = Fake::default();
    fake.last.store(6, Ordering::SeqCst);
    let app = Router::new()
        .route("/{date}/{hour}/ifs/0p25/oper/{name}", get(serve))
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let dir = tempfile::tempdir().unwrap();
    let map = MapData::new(
        dir.path().to_path_buf(),
        reqwest::Client::new(),
        Urls {
            ecmwf: base,
            ..Urls::default()
        },
    );

    // 08 UTC: the 06 UTC run is not out, the 00 UTC run has steps 0 to 6.
    assert_eq!(map.update_ecmwf(R00 + 8 * 3_600).await.unwrap(), 3);
    assert_eq!(fake.ranges.load(Ordering::SeqCst), 6);
    let meta: wf_map::texture::TextureMeta = serde_json::from_slice(
        &std::fs::read(dir.path().join("ecmwf_ifs025/2026101100/003.json")).unwrap(),
    )
    .unwrap();
    // 25°W to 45°E and 27°N to 72°N at 1°, texel edges half a degree out
    assert_eq!((meta.width, meta.height), (71, 46));
    assert_eq!(
        (meta.west, meta.south, meta.east, meta.north),
        (-25.5, 26.5, 45.5, 72.5)
    );
    assert!((meta.v_min - 4.0 / 0.514_444).abs() < 0.01);

    // Two more steps out: only they are fetched.
    fake.last.store(12, Ordering::SeqCst);
    assert_eq!(map.update_ecmwf(R00 + 9 * 3_600).await.unwrap(), 2);
    assert_eq!(fake.ranges.load(Ordering::SeqCst), 10);
    let steps: Vec<u32> = map.runs(ecmwf::MODEL).await.unwrap()[0].steps.clone();
    assert_eq!(steps, [0, 3, 6, 9, 12]);
}
