//! The ECMWF pipeline against a stand-in for data.ecmwf.int: an index per step, the step's
//! GRIB2 file answering byte ranges, steps out up to a point; and a busy portal (429) that sends
//! the round to the mirror.

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

/// U and V on a 10° grid around the world, as ECMWF sends it: from 180° (= 180°W) eastwards,
/// north row first, the last column at 170°E; U grows to the east, V is 4 everywhere, gusts 9
/// everywhere but at step 0, which has none.
fn file(step: u32) -> (Vec<u8>, String) {
    let (ni, nj) = (36, 19);
    let grid = |f: &dyn Fn(u32) -> u8| {
        testing::message(
            ni,
            nj,
            (90_000_000, 180_000_000),
            (-90_000_000, 170_000_000),
            10_000_000,
            0x00,
            &(0..ni * nj).map(|k| Some(f(k % ni))).collect::<Vec<_>>(),
        )
    };
    let (u, v, gust) = (grid(&|i| (i / 10) as u8), grid(&|_| 4), grid(&|_| 9));
    let line = |param: &str, offset: usize, length: usize| {
        format!(
            r#"{{"domain": "g", "type": "fc", "stream": "oper", "levtype": "sfc", "param": "{param}", "_offset": {offset}, "_length": {length}}}"#
        )
    };
    let mut index = vec![
        line("tp", 0, 10),
        line("10u", 10, u.len()),
        line("10v", 10 + u.len(), v.len()),
    ];
    if step > 0 {
        index.push(line("10fg", 10 + u.len() + v.len(), gust.len()));
    }
    ([vec![0; 10], u, v, gust].concat(), index.join("\n"))
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
    let step = step.parse::<u32>().unwrap();
    if step > fake.last.load(Ordering::SeqCst) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let (bytes, index) = file(step);
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
async fn fetches_the_whole_world_by_range() {
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
            ecmwf: base.clone(),
            ecmwf_mirror: base,
            ..Urls::default()
        },
    );

    // 08 UTC: the 06 UTC run is not out, the 00 UTC run has steps 0 to 6.
    assert_eq!(map.update_ecmwf(R00 + 8 * 3_600).await.unwrap(), 3);
    // U and V for each step, gusts for steps 3 and 6
    assert_eq!(fake.ranges.load(Ordering::SeqCst), 8);
    let meta: wf_map::texture::TextureMeta = serde_json::from_slice(
        &std::fs::read(dir.path().join("ecmwf_ifs025/2026101100/003.json")).unwrap(),
    )
    .unwrap();
    // The whole world at 10°, texel edges 5° out; it wraps at 180°.
    assert_eq!((meta.width, meta.height), (36, 19));
    assert_eq!(
        (meta.west, meta.south, meta.east, meta.north),
        (-185.0, -95.0, 175.0, 95.0)
    );
    assert!(meta.wraps());
    assert!((meta.v_min - 4.0 / 0.514_444).abs() < 0.01);
    assert!((meta.gust_max.unwrap() - 9.0 / 0.514_444).abs() < 0.01);
    let first: wf_map::texture::TextureMeta = serde_json::from_slice(
        &std::fs::read(dir.path().join("ecmwf_ifs025/2026101100/000.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(first.gust_max, None);

    // Two more steps out: only they are fetched.
    fake.last.store(12, Ordering::SeqCst);
    assert_eq!(map.update_ecmwf(R00 + 9 * 3_600).await.unwrap(), 2);
    assert_eq!(fake.ranges.load(Ordering::SeqCst), 14);
    let steps: Vec<u32> = map.runs(ecmwf::MODEL).await.unwrap()[0].steps.clone();
    assert_eq!(steps, [0, 3, 6, 9, 12]);
}

#[tokio::test]
async fn goes_on_from_the_mirror_when_the_portal_is_busy() {
    let fake = Fake::default();
    fake.last.store(3, Ordering::SeqCst);
    let mirror = Router::new()
        .route("/{date}/{hour}/ifs/0p25/oper/{name}", get(serve))
        .with_state(fake.clone());
    let busy_calls = Arc::new(AtomicUsize::new(0));
    let calls = busy_calls.clone();
    let busy = Router::new().fallback(move || {
        calls.fetch_add(1, Ordering::SeqCst);
        async { StatusCode::TOO_MANY_REQUESTS }
    });
    let mut bases = Vec::new();
    for app in [busy, mirror] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        bases.push(format!("http://{}", listener.local_addr().unwrap()));
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    }
    let dir = tempfile::tempdir().unwrap();
    let map = MapData::new(
        dir.path().to_path_buf(),
        reqwest::Client::new(),
        Urls {
            ecmwf: bases[0].clone(),
            ecmwf_mirror: bases[1].clone(),
            ..Urls::default()
        },
    );

    // The portal answers the first question (the 06 UTC run) busy and is not asked again; on
    // the mirror the 06 UTC run is not out, the 00 UTC run has steps 0 and 3.
    assert_eq!(map.update_ecmwf(R00 + 8 * 3_600).await.unwrap(), 2);
    assert_eq!(busy_calls.load(Ordering::SeqCst), 1);
    let steps: Vec<u32> = map.runs(ecmwf::MODEL).await.unwrap()[0].steps.clone();
    assert_eq!(steps, [0, 3]);
}
