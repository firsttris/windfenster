//! Wind statistics against stand-ins for the IEM's METAR archive and the DWD's climate archive,
//! in their formats: made on the first request, in the background, then served from the database.

use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::routing::get;
use std::collections::HashMap;
use wf_core::station::{NewStation, Station, StationSource};
use wf_ingest::climate::{Climate, Status, Urls};

/// 2026-10-12: the last whole year is 2025, the statistics cover 2016 to 2025.
const NOW: i64 = 1_791_763_200;

#[derive(Clone, Default)]
struct Fake {
    iem: Arc<AtomicUsize>,
}

/// The IEM: a year of hourly reports, 15 kn from 250° in daylight, calm at night
async fn iem(State(fake): State<Fake>, Query(q): Query<HashMap<String, String>>) -> String {
    fake.iem.fetch_add(1, Ordering::SeqCst);
    assert_eq!(q["station"], "LGRP");
    assert_eq!(
        (
            q["tz"].as_str(),
            q["format"].as_str(),
            q["report_type"].as_str()
        ),
        ("UTC", "onlycomma", "3")
    );
    let year: i64 = q["sts"][..4].parse().unwrap();
    let mut out = String::from("station,valid,sknt,drct\n");
    let start = wf_core::time::unix(year, 1, 1, 0, 0, 0);
    let end = wf_core::time::unix(year + 1, 1, 1, 0, 0, 0);
    for t in (start..end).step_by(3_600) {
        let hour = (t % 86_400) / 3_600;
        let (kn, dir) = if (8..17).contains(&hour) {
            ("15.00", "250.00")
        } else {
            ("0.00", "M")
        };
        let d = wf_core::time::yyyymmdd(t);
        out.push_str(&format!(
            "LGRP,{}-{:02}-{:02} {hour:02}:50,{kn},{dir}\n",
            d / 10_000,
            d / 100 % 100,
            d % 100
        ));
    }
    out
}

fn zip(name: &str, text: &str) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    w.start_file::<_, ()>(
        "Metadaten_Geographie_04177.txt",
        zip::write::FileOptions::default(),
    )
    .unwrap();
    w.write_all(b"metadata").unwrap();
    w.start_file::<_, ()>(name, zip::write::FileOptions::default())
        .unwrap();
    w.write_all(text.as_bytes()).unwrap();
    w.finish().unwrap().into_inner()
}

/// The DWD: the folder listing, the long file to the end of 2025 (8 m/s from 220° every hour of
/// 2020 to 2025) and a recent file that adds nothing in those years
async fn dwd(Path((dir, name)): Path<(String, String)>) -> Result<Vec<u8>, axum::http::StatusCode> {
    match (dir.as_str(), name.as_str()) {
        ("historical", "") => Ok(
            r#"<a href="stundenwerte_FF_04177_20081101_20251231_hist.zip">stundenwerte_FF_04177_20081101_20251231_hist.zip</a>"#
                .into(),
        ),
        ("historical", "stundenwerte_FF_04177_20081101_20251231_hist.zip") => {
            let mut text = String::from("STATIONS_ID;MESS_DATUM;QN_3;   F;   D;eor\n");
            let (start, end) = (wf_core::time::unix(2020, 1, 1, 0, 0, 0), wf_core::time::unix(2026, 1, 1, 0, 0, 0));
            for t in (start..end).step_by(3_600) {
                text.push_str(&format!("4177;{};10;8.0;220;eor\n", wf_core::time::yyyymmdd(t) as i64 * 100 + (t % 86_400) / 3_600));
            }
            Ok(zip("produkt_ff_stunde_20081101_20251231_04177.txt", &text))
        }
        ("recent", "stundenwerte_FF_04177_akt.zip") => Ok(zip(
            "produkt_ff_stunde_20250101_20261011_04177.txt",
            "STATIONS_ID;MESS_DATUM;QN_3;   F;   D;eor\n4177;2026101100;10;9.0;230;eor\n",
        )),
        _ => Err(axum::http::StatusCode::NOT_FOUND),
    }
}

async fn start(fake: Fake) -> String {
    let app = Router::new()
        .route("/iem/cgi-bin/request/asos.py", get(iem))
        .route(
            "/dwd/{dir}/",
            get(|Path(dir): Path<String>| async move { dwd(Path((dir, String::new()))).await }),
        )
        .route("/dwd/{dir}/{name}", get(dwd))
        .with_state(fake);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

fn station(slug: &str, source: StationSource, code: &str, lon: f64) -> NewStation {
    NewStation {
        station: Station {
            slug: slug.into(),
            source,
            code: code.into(),
            name: slug.into(),
            country: None,
            region: None,
            municipality: None,
            lat: 40.0,
            lon,
            elevation_m: None,
            has_taf: false,
        },
        keywords: None,
    }
}

async fn ready(climate: &Arc<Climate>, id: i64, st: &Station) -> wf_core::climate::WindStats {
    for _ in 0..200 {
        if let Status::Ready(s) = climate.status(id, st, NOW).await.unwrap() {
            return s;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!("no statistics for {}", st.slug);
}

#[tokio::test]
async fn makes_the_statistics_once_from_each_archive() {
    let fake = Fake::default();
    let base = start(fake.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let db = wf_core::db::connect(&dir.path().join("t.db"))
        .await
        .unwrap();
    wf_core::db::migrate(&db).await.unwrap();
    let lgrp = station("lgrp", StationSource::Metar, "LGRP", 28.09);
    let dwd_st = station("dwd-04177", StationSource::Dwd, "04177", 8.33);
    wf_core::station::upsert(&db, &[lgrp.clone(), dwd_st.clone()], 1)
        .await
        .unwrap();
    let id = |slug: &'static str| {
        let db = db.clone();
        async move { wf_core::station::id_of(&db, slug).await.unwrap().unwrap() }
    };
    let climate = Arc::new(
        Climate::new(
            db.clone(),
            reqwest::Client::new(),
            Urls {
                iem: format!("{base}/iem"),
                dwd_hourly: format!("{base}/dwd"),
            },
        )
        .with_gap(std::time::Duration::ZERO),
    );

    // First request: being made
    assert_eq!(
        climate
            .status(id("lgrp").await, &lgrp.station, NOW)
            .await
            .unwrap(),
        Status::Pending
    );
    let stats = ready(&climate, id("lgrp").await, &lgrp.station).await;
    assert_eq!(
        (stats.source.as_str(), stats.first_year, stats.last_year),
        ("iem", 2016, 2025)
    );
    // A year per request
    assert_eq!(fake.iem.load(Ordering::SeqCst), 10);
    let july = &stats.months[6];
    assert_eq!(july.hours, 31 * 24 * 10);
    // Daylight by sun time at 28° E (UTC + 1 h 52 min): the windy hours 8–16 UTC are 9:52–18:52
    assert_eq!((july.days, july.windy_days), (310, 310));
    let west = wf_core::climate::sector(250.0);
    assert_eq!(july.rose[west][wf_core::climate::band(15.0)], 31 * 9 * 10);
    assert_eq!(july.calm, 31 * 15 * 10);

    // Served from the database: no more requests
    assert!(matches!(
        climate
            .status(id("lgrp").await, &lgrp.station, NOW)
            .await
            .unwrap(),
        Status::Ready(_)
    ));
    assert_eq!(fake.iem.load(Ordering::SeqCst), 10);

    // The DWD station: only 2020 to 2025 in its archive, the recent file's 2026 left out
    let stats = ready(&climate, id("dwd-04177").await, &dwd_st.station).await;
    assert_eq!(
        (stats.source.as_str(), stats.months[0].hours),
        ("dwd", 31 * 24 * 6)
    );
    assert_eq!(
        stats.months[0].rose[wf_core::climate::sector(220.0)]
            [wf_core::climate::band(8.0 / 0.514_444)],
        31 * 24 * 6
    );
}
