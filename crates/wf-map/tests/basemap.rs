//! Making the basemap against a stand-in for build.protomaps.com: a small archive by byte range,
//! only yesterday's build out; the region in one archive, a spot in one of its own.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::get;
use pmtiles::{AsyncBackend, AsyncPmTilesReader, BackendResponse, PmtResult, TileCoord};
use wf_map::basemap;

/// 2026-10-11 12 UTC
const NOW: i64 = 1_791_720_000;
/// Baden Airpark
const SPOT: (f64, f64) = (48.78, 8.08);

fn coord(z: u8, x: u32, y: u32) -> TileCoord {
    TileCoord::new(z, x, y).unwrap()
}

/// The world tile, a zoom-8 tile over the Rhine, the spot's tile at 12 and 14, and a zoom-12
/// tile far out in the Pacific.
fn archive() -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    let mut w = pmtiles::PmTilesWriter::new(pmtiles::TileType::Mvt)
        .tile_compression(pmtiles::Compression::None)
        .metadata(r#"{"name":"Protomaps Basemap","version":"4.15.2"}"#)
        .create(&mut out)
        .unwrap();
    for (c, data) in [
        (coord(0, 0, 0), b"world".as_slice()),
        (coord(8, 133, 88), b"rhine"),
        (coord(12, 0, 0), b"pacific"),
        (coord(12, 2139, 1410), b"airpark"),
        (coord(14, 8559, 5641), b"runway"),
    ] {
        w.add_raw_tile(c, data).unwrap();
    }
    w.finalize().unwrap();
    out.into_inner()
}

#[derive(Clone)]
struct Fake {
    file: Arc<Vec<u8>>,
    ranges: Arc<AtomicUsize>,
}

async fn serve(
    State(f): State<Fake>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> axum::response::Response {
    if name != "20261010.pmtiles" {
        return StatusCode::NOT_FOUND.into_response();
    }
    f.ranges.fetch_add(1, Ordering::SeqCst);
    let range = headers[header::RANGE].to_str().unwrap();
    let (a, b) = range
        .strip_prefix("bytes=")
        .unwrap()
        .split_once('-')
        .unwrap();
    let a: usize = a.parse().unwrap();
    let b = b.parse::<usize>().unwrap().min(f.file.len() - 1);
    (StatusCode::PARTIAL_CONTENT, f.file[a..=b].to_vec()).into_response()
}

/// Reads the archive we made, from memory.
struct Memory(Vec<u8>);

impl AsyncBackend for Memory {
    async fn read(&self, offset: usize, length: usize) -> PmtResult<BackendResponse> {
        let end = (offset + length).min(self.0.len());
        Ok(BackendResponse::new(self.0[offset..end].to_vec().into()))
    }
}

#[tokio::test]
async fn cuts_the_region_and_the_spots_from_the_newest_build() {
    let fake = Fake {
        file: Arc::new(archive()),
        ranges: Arc::default(),
    };
    let app = Router::new()
        .route("/{name}", get(serve))
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let dir = tempfile::tempdir().unwrap();
    let http = reqwest::Client::new();

    // Today's build is not out: yesterday's is taken. The main archive holds the region only.
    let build = basemap::fetch::keep_current(dir.path(), &http, &base, 8, NOW)
        .await
        .unwrap();
    assert_eq!(build.as_deref(), Some("20261010"));
    let made = std::fs::read(basemap::path(dir.path())).unwrap();
    let header = basemap::read_header(&made).unwrap();
    assert_eq!((header.min_zoom, header.max_zoom), (0, 8));
    let reader = AsyncPmTilesReader::try_from_source(Memory(made))
        .await
        .unwrap();
    let tile = async |c| reader.get_tile(c).await.unwrap().map(|b| b.to_vec());
    assert_eq!(
        tile(coord(0, 0, 0)).await.as_deref(),
        Some(b"world".as_slice())
    );
    assert_eq!(
        tile(coord(8, 133, 88)).await.as_deref(),
        Some(b"rhine".as_slice())
    );
    assert_eq!(tile(coord(12, 2139, 1410)).await, None);
    assert!(reader.get_metadata().await.unwrap().contains("4.15.2"));
    let info = basemap::info(dir.path()).await.unwrap();
    assert_eq!((info.full_zoom, info.detail.len()), (8, 0));

    // A spot looked at gets an archive of its own, served by tile next to the region's.
    let spot = basemap::spots::Spot::of(SPOT.0, SPOT.1);
    let n = basemap::spots::make(dir.path(), &http, &base, spot, 9, NOW)
        .await
        .unwrap();
    assert_eq!(n, 2);
    let info = basemap::info(dir.path()).await.unwrap();
    assert_eq!(
        (info.full_zoom, info.detail.len(), info.detail_zoom),
        (8, 1, 14)
    );
    let readers = basemap::serve::Readers::default();
    let served = async |z, x, y| readers.tile(dir.path(), z, x, y).await.map(|t| t.data);
    assert_eq!(
        served(14, 8559, 5641).await.as_deref(),
        Some(b"runway".as_slice())
    );
    assert_eq!(
        served(12, 2139, 1410).await.as_deref(),
        Some(b"airpark".as_slice())
    );
    assert_eq!(
        served(8, 133, 88).await.as_deref(),
        Some(b"rhine".as_slice())
    );
    // Out in the Pacific: nobody looked there.
    assert_eq!(served(12, 0, 0).await, None);

    // Made already: nothing to do, nothing fetched.
    let asked = fake.ranges.load(Ordering::SeqCst);
    let again = basemap::fetch::keep_current(dir.path(), &http, &base, 8, NOW + 86_400)
        .await
        .unwrap();
    assert_eq!((again, fake.ranges.load(Ordering::SeqCst)), (None, asked));

    // A file without our note is the operator's: never replaced, and no spots added.
    std::fs::remove_file(dir.path().join("basemap.json")).unwrap();
    let theirs = basemap::fetch::keep_current(dir.path(), &http, &base, 8, NOW)
        .await
        .unwrap();
    assert_eq!(theirs, None);
    let theirs = basemap::info(dir.path()).await.unwrap();
    assert_eq!((theirs.full_zoom, theirs.detail.len()), (8, 0));
}
