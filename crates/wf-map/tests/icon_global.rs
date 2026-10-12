//! The ICON global pipeline against a stand-in for the DWD's open-data server: folder indexes
//! as the server lists them, bz2-compressed GRIB2 files of a model with four cells (their
//! centres as CLAT and CLON), regridded, stored and cut into a piece.

use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use wf_map::crop::{self, Piece};
use wf_map::store::run_id;
use wf_map::{MapData, Urls, icon_global, testing};

/// 2026-10-12 00 UTC
const R00: i64 = 1_791_763_200;

#[derive(Clone, Default)]
struct Fake {
    files: Arc<AtomicUsize>,
    centres: Arc<AtomicUsize>,
}

async fn folder(Path((hour, var)): Path<(String, String)>) -> String {
    let mut html = format!("<h1>Index of /weather/nwp/icon/grib/{hour}/{var}/</h1><pre>\n");
    if hour == "00" {
        // The gust has no first hour; 079 is not taken (only every third hour after 78).
        let steps: &[u32] = if var == icon_global::GUST {
            &[1]
        } else {
            &[0, 1, 79]
        };
        for &step in steps {
            let name = icon_global::file_name(R00, step, &var);
            html.push_str(&format!(
                "<a href=\"{name}\">{name}</a> 12-Oct-2026 02:39 3874755\n"
            ));
        }
    }
    html + "</pre>"
}

async fn file(
    State(fake): State<Fake>,
    Path((hour, var, name)): Path<(String, String, String)>,
) -> Result<Vec<u8>, StatusCode> {
    if hour != "00" || !name.contains(&run_id(R00)) {
        return Err(StatusCode::NOT_FOUND);
    }
    // Four cells: around (10°N, 10°E), (50°N, 10°E), (50°N, 30°E) and (10°N, 30°E)
    let values: [u8; 4] = match var.as_str() {
        icon_global::CLAT => {
            fake.centres.fetch_add(1, Ordering::SeqCst);
            [10, 50, 50, 10]
        }
        icon_global::CLON => {
            fake.centres.fetch_add(1, Ordering::SeqCst);
            [10, 10, 30, 30]
        }
        // m/s, a different value in every cell
        icon_global::U => [1, 2, 3, 4],
        icon_global::V => [5, 6, 7, 8],
        _ => [9, 10, 11, 12],
    };
    fake.files.fetch_add(1, Ordering::SeqCst);
    let values: Vec<Option<u8>> = values.iter().copied().map(Some).collect();
    // The grid section does not matter: ICON global's places come from CLAT and CLON.
    let grib = testing::message(
        2,
        2,
        (0, 0),
        (1_000_000, 1_000_000),
        1_000_000,
        0x40,
        &values,
    );
    let mut bz = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
    bz.write_all(&grib).unwrap();
    Ok(bz.finish().unwrap())
}

async fn start(fake: Fake) -> String {
    let app = Router::new()
        .route("/{hour}/{var}/", get(folder))
        .route("/{hour}/{var}/{name}", get(file))
        .with_state(fake);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

#[tokio::test]
async fn regrids_stores_and_cuts_pieces() {
    let fake = Fake::default();
    let base = start(fake.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let map = MapData::new(
        dir.path().to_path_buf(),
        reqwest::Client::new(),
        Urls {
            icon_global: base,
            ..Urls::default()
        },
    );

    // 04 UTC: hours 0 and 1 of the 00 UTC run, the cells' centres once.
    assert_eq!(map.update_icon_global(R00 + 4 * 3_600).await.unwrap(), 2);
    assert_eq!(fake.centres.load(Ordering::SeqCst), 2);
    assert!(dir.path().join("icon_global-grid.u32").exists());
    let run = dir.path().join("icon_global/2026101200");
    // No world PNG: the app takes pieces.
    assert!(run.join("001.rgba").exists() && !run.join("001.png").exists());
    let world: wf_map::texture::TextureMeta =
        serde_json::from_slice(&std::fs::read(run.join("001.json")).unwrap()).unwrap();
    assert_eq!((world.width, world.height), (2_880, 1_441));
    assert!(world.wraps());
    assert!(world.gust_max.is_some());

    let index = map.index().await.unwrap();
    let layer = index
        .layers
        .iter()
        .find(|l| l.model == icon_global::MODEL)
        .unwrap();
    assert_eq!(layer.frames.len(), 2);
    assert!(layer.in_pieces);
    assert_eq!(layer.area, Some([-180.0625, -90.0625, 179.9375, 90.0625]));

    // A piece over central Europe: every texel takes the cell at 50°N 10°E or 50°N 30°E.
    let piece = Piece {
        west: 0,
        south: 40,
        east: 40,
        north: 60,
    };
    let (png, meta) = crop::cut(&run.join("001.rgba"), &world, piece).unwrap();
    assert_eq!((meta.width, meta.height), (320, 160));
    assert!((meta.west - (-0.0625)).abs() < 1e-9 && (meta.north - 60.0625).abs() < 1e-9);
    let mut reader = png::Decoder::new(std::io::Cursor::new(png))
        .read_info()
        .unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    reader.next_frame(&mut buf).unwrap();
    // Texels next to a cell take its value, those far from every cell none.
    let at = |lat: f64, lon: f64| {
        let col = ((lon - meta.west) / 0.125) as usize;
        let row = ((meta.north - lat) / 0.125) as usize;
        let k = (row * meta.width as usize + col) * 4;
        let u = meta.u_min + f32::from(buf[k]) / 255.0 * (meta.u_max - meta.u_min);
        (buf[k + 3] == 255).then_some(u)
    };
    let kn = 1.0 / 0.514_444;
    assert!((at(50.0, 10.0).unwrap() - 2.0 * kn).abs() < 0.1);
    assert!((at(50.1, 29.9).unwrap() - 3.0 * kn).abs() < 0.1);
    assert_eq!(at(59.0, 1.0), None);

    // Nothing new: nothing fetched, the table read from disk by a new server.
    let before = fake.files.load(Ordering::SeqCst);
    assert_eq!(map.update_icon_global(R00 + 5 * 3_600).await.unwrap(), 0);
    assert_eq!(fake.files.load(Ordering::SeqCst), before);
}
