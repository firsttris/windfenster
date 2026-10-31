//! Making the basemap against a stand-in for build.protomaps.com: a small archive by byte range,
//! only yesterday's build out; the world and the region in one archive, a spot in one of its
//! own, and tiles past them cut from the deepest one above.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::get;
use pmtiles::{AsyncBackend, AsyncPmTilesReader, BackendResponse, PmtResult, TileCoord};
use wf_map::basemap::{self, fetch::Cut};
use wf_map::testing;

/// 2026-10-11 12 UTC
const NOW: i64 = 1_791_720_000;
/// Baden Airpark
const SPOT: (f64, f64) = (48.78, 8.08);

fn coord(z: u8, x: u32, y: u32) -> TileCoord {
    TileCoord::new(z, x, y).unwrap()
}

const EUROPE: [f64; 4] = [-25.0, 27.0, 45.0, 72.0];

/// The world tile (a place at its top-left corner), a zoom-8 tile over the Rhine, the spot's
/// tile at 12 and 14, and a zoom-12 tile far out in the Pacific; each a vector tile with a place
/// whose x tells them apart.
fn archive() -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    let mut w = pmtiles::PmTilesWriter::new(pmtiles::TileType::Mvt)
        .tile_compression(pmtiles::Compression::None)
        .max_zoom(14)
        .metadata(r#"{"name":"Protomaps Basemap","version":"4.15.2"}"#)
        .create(&mut out)
        .unwrap();
    for (c, id) in [
        (coord(0, 0, 0), 0),
        (coord(8, 133, 88), 8),
        (coord(12, 0, 0), 120),
        (coord(12, 2139, 1410), 12),
        (coord(14, 8559, 5641), 14),
    ] {
        w.add_tile(c, &testing::vector_tile(&[(id, 0)])).unwrap();
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
    let range = format!("bytes {a}-{b}/{}", f.file.len());
    (
        StatusCode::PARTIAL_CONTENT,
        [
            (header::CONTENT_RANGE, range),
            (header::ETAG, "\"b1\"".into()),
        ],
        f.file[a..=b].to_vec(),
    )
        .into_response()
}

/// Reads the archive we made, from memory.
struct Memory(Vec<u8>);

impl AsyncBackend for Memory {
    async fn read(&self, offset: u64, length: usize) -> PmtResult<BackendResponse> {
        let offset = offset as usize;
        let end = (offset + length).min(self.0.len());
        Ok(BackendResponse::new(self.0[offset..end].to_vec().into()))
    }
}

#[tokio::test]
async fn cuts_the_world_the_region_and_the_spots_from_the_newest_build() {
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

    // Today's build is not out: yesterday's is taken. The main archive holds the world to 4 and
    // Europe to 8.
    let cut = Cut::new(4, Some(EUROPE), 8);
    let build = basemap::fetch::keep_current(dir.path(), &http, &base, &cut, NOW)
        .await
        .unwrap();
    assert_eq!(build.as_deref(), Some("20261010"));
    let made = std::fs::read(basemap::path(dir.path())).unwrap();
    let header = basemap::read_header(&made).unwrap();
    assert_eq!((header.min_zoom, header.max_zoom), (0, 8));
    assert_eq!(header.bounds[0], -180.0);
    let reader = AsyncPmTilesReader::try_from_source(Memory(made))
        .await
        .unwrap();
    let tile = async |c| {
        let t = reader.get_tile(c).await.unwrap()?;
        testing::points_in(&t)
    };
    assert_eq!(tile(coord(0, 0, 0)).await, Some(vec![(0.0, 0.0)]));
    assert_eq!(tile(coord(8, 133, 88)).await, Some(vec![(8.0, 0.0)]));
    assert_eq!(tile(coord(12, 2139, 1410)).await, None);
    assert_eq!(tile(coord(12, 0, 0)).await, None);
    assert!(reader.get_metadata().await.unwrap().contains("4.15.2"));
    let info = basemap::info(dir.path()).await.unwrap();
    assert_eq!(
        (
            info.full_zoom,
            info.region,
            info.region_zoom,
            info.detail.len()
        ),
        (4, Some(EUROPE), 8, 0)
    );

    // A spot looked at gets an archive of its own, from just past the world's zoom, served by
    // tile next to the main one.
    let spot = basemap::spots::Spot::of(SPOT.0, SPOT.1);
    let n = basemap::spots::make(dir.path(), &http, &base, spot, 5, NOW)
        .await
        .unwrap();
    assert_eq!(n, 3);
    let info = basemap::info(dir.path()).await.unwrap();
    assert_eq!(
        (info.full_zoom, info.detail.len(), info.detail_zoom),
        (4, 1, 14)
    );
    let readers = basemap::serve::Readers::default();
    let served = async |z, x, y| {
        let t = readers.tile(dir.path(), z, x, y).await?;
        Some((testing::points_in(&t.data)?, t.exact))
    };
    assert_eq!(
        served(14, 8559, 5641).await,
        Some((vec![(14.0, 0.0)], true))
    );
    assert_eq!(
        served(12, 2139, 1410).await,
        Some((vec![(12.0, 0.0)], true))
    );
    assert_eq!(served(8, 133, 88).await, Some((vec![(8.0, 0.0)], true)));
    // Past the spot's zoom: its zoom-14 tile cut down, the place 4 times as far right
    assert_eq!(
        served(16, 8559 * 4, 5641 * 4).await,
        Some((vec![(56.0, 0.0)], false))
    );
    // In Europe past the region's zoom, nobody looked: the Rhine's tile cut down
    assert_eq!(
        served(10, 133 * 4, 88 * 4).await,
        Some((vec![(32.0, 0.0)], false))
    );
    // Out in the Pacific: the world's tile cut down, never a hole
    assert_eq!(served(12, 0, 0).await, Some((vec![(0.0, 0.0)], false)));
    assert_eq!(served(12, 4095, 4095).await, Some((vec![], false)));

    // Made already: nothing to do, nothing fetched.
    let asked = fake.ranges.load(Ordering::SeqCst);
    let again = basemap::fetch::keep_current(dir.path(), &http, &base, &cut, NOW + 86_400)
        .await
        .unwrap();
    assert_eq!((again, fake.ranges.load(Ordering::SeqCst)), (None, asked));

    // A file without our note is the operator's: never replaced, and no spots added.
    std::fs::remove_file(dir.path().join("basemap.json")).unwrap();
    let theirs = basemap::fetch::keep_current(dir.path(), &http, &base, &cut, NOW)
        .await
        .unwrap();
    assert_eq!(theirs, None);
    let theirs = basemap::info(dir.path()).await.unwrap();
    assert_eq!(
        (theirs.full_zoom, theirs.region, theirs.detail.len()),
        (8, None, 0)
    );
}

#[tokio::test]
async fn downloads_the_build_whole_when_the_cut_is_all_of_it() {
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
    // The world (as WF_BASEMAP_REGION types it) to 14, the build's own deepest zoom
    let cut = Cut::new(4, Some([-180.0, -85.05, 180.0, 85.05]), 14);
    let build = basemap::fetch::keep_current(dir.path(), &reqwest::Client::new(), &base, &cut, NOW)
        .await
        .unwrap();
    assert_eq!(build.as_deref(), Some("20261010"));
    // The file is the build itself, byte for byte, and the part is gone
    assert_eq!(
        std::fs::read(basemap::path(dir.path())).unwrap(),
        *fake.file
    );
    let left: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.contains(".part"))
        .collect();
    assert!(left.is_empty(), "{left:?}");
    let info = basemap::info(dir.path()).await.unwrap();
    assert_eq!((info.max_zoom, info.region_zoom), (14, 14));
}
