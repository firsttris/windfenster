//! The ICON-D2 pipeline against a stand-in for the DWD's open-data server: folder indexes as
//! the server lists them, bz2-compressed GRIB2 files.

use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use wf_map::store::run_id;
use wf_map::{MapData, Urls, icon_d2, testing};

/// 2026-10-11 06 UTC
const R06: i64 = 1_791_698_400;
const R09: i64 = R06 + 3 * 3_600;

#[derive(Clone, Default)]
struct Fake {
    /// The newest run out; each run lists its first two hours.
    newest: Arc<AtomicI64>,
    files: Arc<AtomicUsize>,
}

/// The run a folder of the given hour holds, if it is out.
fn listed_run(fake: &Fake, hour: &str) -> Option<i64> {
    let newest = fake.newest.load(Ordering::SeqCst);
    [R06, R09]
        .into_iter()
        .filter(|r| *r <= newest)
        .find(|r| format!("{:02}", r.rem_euclid(86_400) / 3_600) == hour)
}

async fn folder(State(fake): State<Fake>, Path((hour, var)): Path<(String, String)>) -> String {
    let mut html = format!("<h1>Index of /weather/nwp/icon-d2/grib/{hour}/{var}/</h1><pre>\n");
    if let Some(run) = listed_run(&fake, &hour) {
        for step in 0..2 {
            let name = icon_d2::file_name(run, step, &var);
            html.push_str(&format!(
                "<a href=\"{name}\">{name}</a> 11-Oct-2026 06:44 1057692\n"
            ));
        }
    }
    html + "</pre>"
}

async fn file(
    State(fake): State<Fake>,
    Path((hour, var, name)): Path<(String, String, String)>,
) -> Result<Vec<u8>, StatusCode> {
    let run = listed_run(&fake, &hour).ok_or(StatusCode::NOT_FOUND)?;
    if !name.contains(&run_id(run)) {
        return Err(StatusCode::NOT_FOUND);
    }
    fake.files.fetch_add(1, Ordering::SeqCst);
    // 3 × 2 points from 10°E 50°N; V misses one point.
    let values: Vec<Option<u8>> = if var == icon_d2::U {
        vec![Some(1), Some(2), Some(3), Some(4), Some(5), Some(6)]
    } else {
        vec![Some(6), Some(5), Some(4), None, Some(2), Some(1)]
    };
    let grib = testing::message(
        3,
        2,
        (50_000_000, 10_000_000),
        (50_500_000, 11_000_000),
        500_000,
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
async fn fetches_new_hours_and_fills_in_from_the_run_before() {
    let fake = Fake::default();
    fake.newest.store(R06, Ordering::SeqCst);
    let base = start(fake.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let map = MapData::new(
        dir.path().to_path_buf(),
        reqwest::Client::new(),
        Urls {
            icon_d2: base,
            ..Urls::default()
        },
    );

    // 08 UTC: the 06 UTC run has two hours out, U and V each.
    assert_eq!(map.update_icon_d2(R06 + 2 * 3_600).await.unwrap(), 2);
    assert_eq!(fake.files.load(Ordering::SeqCst), 4);
    let meta: wf_map::texture::TextureMeta = serde_json::from_slice(
        &std::fs::read(dir.path().join("icon_d2/2026101106/001.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        (meta.width, meta.height, meta.valid_at),
        (3, 2, R06 + 3_600)
    );
    assert_eq!((meta.west, meta.north), (9.75, 50.75));
    assert!(dir.path().join("icon_d2/2026101106/001.png").exists());

    // Nothing new: nothing fetched.
    assert_eq!(map.update_icon_d2(R06 + 2 * 3_600).await.unwrap(), 0);
    assert_eq!(fake.files.load(Ordering::SeqCst), 4);

    // 09 UTC is out: its hours come first, 06 UTC fills in the hours before.
    fake.newest.store(R09, Ordering::SeqCst);
    assert_eq!(map.update_icon_d2(R09 + 3_600).await.unwrap(), 2);
    let index = map.index().await.unwrap();
    let frames: Vec<&str> = index.layers[0]
        .frames
        .iter()
        .map(|f| f.path.as_str())
        .collect();
    assert_eq!(
        frames,
        [
            "icon_d2/2026101106/000",
            "icon_d2/2026101106/001",
            "icon_d2/2026101109/000",
            "icon_d2/2026101109/001",
        ]
    );
}
