//! The live alarm end to end against a local stand-in for a browser's push service: the message
//! goes out once a day, encrypted (aes128gcm) and signed (VAPID), and a browser the service says
//! is gone is forgotten.

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use openssl::bn::BigNumContext;
use openssl::ec::{EcGroup, EcKey, PointConversionForm};
use openssl::nid::Nid;
use wf_alarm::{Alarms, Push};
use wf_core::alarm::{self, AlarmSettings, PushSubscription};
use wf_core::favorite::{self, FavoriteInput, Sector};
use wf_core::observation::{self, Observation};
use wf_core::station::{self, NewStation, Station, StationSource};

/// 2026-10-12 10:00 UTC, noon in Germany
const NOW: i64 = 1_791_799_200;

type Seen = Arc<Mutex<Vec<(String, HeaderMap, usize)>>>;

/// A push service: `/ok/*` takes messages, `/gone/*` answers 410.
async fn push_service() -> (String, Seen) {
    let seen: Seen = Arc::default();
    let (ok, gone) = (seen.clone(), seen.clone());
    let app = Router::new()
        .route(
            "/ok/{id}",
            post(
                move |headers: HeaderMap, body: axum::body::Bytes| async move {
                    ok.lock().unwrap().push(("ok".into(), headers, body.len()));
                    StatusCode::CREATED
                },
            ),
        )
        .route(
            "/gone/{id}",
            post(
                move |headers: HeaderMap, body: axum::body::Bytes| async move {
                    gone.lock()
                        .unwrap()
                        .push(("gone".into(), headers, body.len()));
                    StatusCode::GONE
                },
            ),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, seen)
}

/// A browser's keys, as `PushSubscription.toJSON()` gives them
fn browser(endpoint: String) -> PushSubscription {
    let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1).unwrap();
    let key = EcKey::generate(&group).unwrap();
    let mut ctx = BigNumContext::new().unwrap();
    let public = key
        .public_key()
        .to_bytes(&group, PointConversionForm::UNCOMPRESSED, &mut ctx)
        .unwrap();
    let b64 = |b: &[u8]| {
        openssl::base64::encode_block(b)
            .trim_end_matches('=')
            .replace('+', "-")
            .replace('/', "_")
    };
    PushSubscription {
        endpoint,
        p256dh: b64(&public),
        auth: b64(&[7u8; 16]),
    }
}

#[tokio::test]
async fn tells_once_when_the_station_measures_it() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let db = wf_core::db::connect(&dir.path().join("t.db")).await?;
    wf_core::db::migrate(&db).await?;
    let st = Station {
        slug: "edsb".into(),
        source: StationSource::Metar,
        code: "EDSB".into(),
        name: "Baden".into(),
        country: None,
        region: None,
        municipality: None,
        lat: 48.78,
        lon: 8.08,
        elevation_m: None,
        has_taf: false,
    };
    station::upsert(
        &db,
        &[NewStation {
            station: st,
            keywords: None,
        }],
        1,
    )
    .await?;
    let id = station::id_of(&db, "edsb").await?.unwrap();
    let me = wf_core::account::create(&db, "u1", "Ana", "c1", "{}", 0).await?;
    let input = FavoriteInput {
        slug: Some("edsb".into()),
        lat: 48.78,
        lon: 8.08,
        name: "Airpark".into(),
        sector: Some(Sector { from: 180, to: 300 }),
    };
    let fav = favorite::add(&db, me, &input, 1).await?.unwrap();
    let settings = AlarmSettings {
        min_kn: 14.0,
        gear: false,
        forecast: false,
        live: true,
        push: true,
        email: false,
    };
    alarm::set(&db, me, fav.id, &settings, 1).await?;

    let (url, seen) = push_service().await;
    alarm::subscribe(&db, me, &browser(format!("{url}/ok/1")), 1).await?;
    alarm::subscribe(&db, me, &browser(format!("{url}/gone/2")), 1).await?;
    let push = Push::load_or_create(
        &dir.path().join("vapid.pem"),
        "mailto:a@b.c".into(),
        reqwest::Client::new(),
    )?;
    let alarms = Alarms {
        db: db.clone(),
        map: None,
        push: Some(push),
        mail: None,
        public_url: None,
    };

    let measured = |at: i64, wind: f32, dir: f32| Observation {
        at,
        wind: Some(wind),
        gust: Some(wind + 6.0),
        dir: Some(dir),
        temp: None,
        raw: None,
    };
    // Only one measurement with the wind: no alarm yet
    observation::store(
        &db,
        &[
            (id, measured(NOW - 1_800, 10.0, 240.0)),
            (id, measured(NOW - 600, 17.0, 240.0)),
        ],
    )
    .await?;
    alarms.check(NOW).await?;
    assert!(seen.lock().unwrap().is_empty());

    // Two in a row, from the west: told, encrypted and signed
    observation::store(&db, &[(id, measured(NOW - 1_800, 15.0, 250.0))]).await?;
    alarms.check(NOW).await?;
    {
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        let (_, headers, len) = seen.iter().find(|(k, ..)| k == "ok").unwrap();
        assert_eq!(headers["content-encoding"], "aes128gcm");
        assert!(headers["authorization"].to_str()?.starts_with("vapid t="));
        assert!(*len > 100);
    }
    // The browser that is gone is forgotten; the alarm is not told twice that day.
    assert_eq!(alarm::subscriptions(&db, me).await?.len(), 1);
    alarms.check(NOW + 600).await?;
    assert_eq!(seen.lock().unwrap().len(), 2);
    Ok(())
}

#[tokio::test]
async fn sleeps_at_night_and_needs_the_direction() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let db = wf_core::db::connect(&dir.path().join("t.db")).await?;
    wf_core::db::migrate(&db).await?;
    let me = wf_core::account::create(&db, "u1", "Ana", "c1", "{}", 0).await?;
    let st = Station {
        slug: "edsb".into(),
        source: StationSource::Metar,
        code: "EDSB".into(),
        name: "Baden".into(),
        country: None,
        region: None,
        municipality: None,
        lat: 48.78,
        lon: 8.08,
        elevation_m: None,
        has_taf: false,
    };
    station::upsert(
        &db,
        &[NewStation {
            station: st,
            keywords: None,
        }],
        1,
    )
    .await?;
    let id = station::id_of(&db, "edsb").await?.unwrap();
    let input = FavoriteInput {
        slug: Some("edsb".into()),
        lat: 48.78,
        lon: 8.08,
        name: "Airpark".into(),
        sector: Some(Sector { from: 180, to: 300 }),
    };
    let fav = favorite::add(&db, me, &input, 1).await?.unwrap();
    let settings = AlarmSettings {
        min_kn: 14.0,
        gear: false,
        forecast: false,
        live: true,
        push: true,
        email: false,
    };
    alarm::set(&db, me, fav.id, &settings, 1).await?;
    let (url, seen) = push_service().await;
    alarm::subscribe(&db, me, &browser(format!("{url}/ok/1")), 1).await?;
    let push = Push::load_or_create(
        &dir.path().join("vapid.pem"),
        "mailto:a@b.c".into(),
        reqwest::Client::new(),
    )?;
    let alarms = Alarms {
        db: db.clone(),
        map: None,
        push: Some(push),
        mail: None,
        public_url: None,
    };
    let at = |t: i64, dir: f32| Observation {
        at: t,
        wind: Some(20.0),
        gust: None,
        dir: Some(dir),
        temp: None,
        raw: None,
    };

    // From the east, which does not work there
    observation::store(
        &db,
        &[(id, at(NOW - 1_800, 90.0)), (id, at(NOW - 600, 90.0))],
    )
    .await?;
    alarms.check(NOW).await?;
    // From the west, but at 23:00 local time
    let night = NOW + 11 * 3_600;
    observation::store(
        &db,
        &[(id, at(night - 1_800, 250.0)), (id, at(night - 600, 250.0))],
    )
    .await?;
    alarms.check(night).await?;
    assert!(seen.lock().unwrap().is_empty());
    Ok(())
}
