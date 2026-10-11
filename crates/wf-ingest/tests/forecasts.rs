//! The forecast pipeline against a stand-in for Open-Meteo that answers in its format.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use std::collections::HashMap;
use wf_core::forecast::Notice;
use wf_core::geo::GridPoint;
use wf_ingest::forecasts::Forecasts;
use wf_ingest::openmeteo::{Config, OpenMeteo};

#[derive(Clone, Default)]
struct Fake {
    calls: Arc<AtomicUsize>,
    down: Arc<std::sync::atomic::AtomicBool>,
}

/// Answers like Open-Meteo: per requested model six suffixed columns over 48 hours; the
/// regional models only have values inside their domain, the run metadata says 06 UTC.
async fn forecast(
    State(fake): State<Fake>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<String, (StatusCode, [(&'static str, &'static str); 1])> {
    if fake.down.load(Ordering::SeqCst) {
        // Retry-After: 0 keeps the client's retries fast.
        return Err((StatusCode::BAD_GATEWAY, [("retry-after", "0")]));
    }
    fake.calls.fetch_add(1, Ordering::SeqCst);
    let lat: f64 = q["latitude"].parse().unwrap();
    let models: Vec<&str> = q["models"].split(',').collect();
    let start = 1_791_669_600_i64; // 2026-10-10 22:00 UTC = local midnight in Berlin
    let times: Vec<i64> = (0..48).map(|h| start + h * 3600).collect();
    let mut hourly = serde_json::Map::new();
    hourly.insert("time".into(), times.into());
    for m in &models {
        let inside = *m != "icon_d2" || lat > 43.5;
        for (var, base) in [
            ("wind_speed_10m", 12.0),
            ("wind_gusts_10m", 18.0),
            ("wind_direction_10m", 230.0),
            ("temperature_2m", 15.0),
            ("precipitation", 0.0),
            ("cloud_cover", 30.0),
        ] {
            let vals: Vec<serde_json::Value> = (0..48)
                .map(|h| {
                    if inside {
                        (base + h as f64 * 0.1).into()
                    } else {
                        serde_json::Value::Null
                    }
                })
                .collect();
            let key = if models.len() == 1 {
                var.to_owned()
            } else {
                format!("{var}_{m}")
            };
            hourly.insert(key, vals.into());
        }
    }
    Ok(serde_json::json!({
        "latitude": lat, "longitude": q["longitude"], "timezone": "Europe/Berlin",
        "elevation": 121.0, "hourly": hourly
    })
    .to_string())
}

async fn meta() -> String {
    r#"{"last_run_initialisation_time": 1791633600}"#.into()
}

async fn setup() -> anyhow::Result<(tempfile::TempDir, Fake, Forecasts, sqlx::SqlitePool)> {
    let fake = Fake::default();
    let app = Router::new()
        .route("/v1/forecast", get(forecast))
        .route("/data/{id}/static/meta.json", get(meta))
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let base = format!("http://{}", listener.local_addr()?);
    tokio::spawn(async move { axum::serve(listener, app).await });

    let dir = tempfile::tempdir()?;
    let db = wf_core::db::connect(&dir.path().join("t.db")).await?;
    wf_core::db::migrate(&db).await?;
    let models = wf_core::model::active(
        &db,
        Some(&["icon_d2".to_owned(), "ecmwf_ifs025".to_owned()]),
    )
    .await?;
    let http = reqwest::Client::builder().no_proxy().build()?;
    let om = OpenMeteo::new(
        http,
        Config {
            base_url: Some(base),
            ..Config::default()
        },
    );
    Ok((dir, fake, Forecasts::new(db.clone(), om, models), db))
}

#[tokio::test]
async fn fetches_once_per_run_and_serves_from_the_database() -> anyhow::Result<()> {
    let (_dir, fake, f, _db) = setup().await?;
    let edsb = GridPoint::new(48.7794, 8.0805).unwrap();
    let mut updates = f.subscribe();
    // As the scheduler does at start: the runs from the metadata.
    assert_eq!(f.detect_runs(wf_core::time::now()).await?, 2);

    assert_eq!(f.ensure(edsb, true).await?, None);
    assert_eq!(updates.try_recv()?, edsb);
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
    // The same run is not fetched again.
    assert_eq!(f.ensure(edsb, true).await?, None);
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);

    let fc = f.load(edsb, None).await?;
    assert_eq!(fc.timezone.as_deref(), Some("Europe/Berlin"));
    let ids: Vec<&str> = fc.series.iter().map(|s| s.model.as_str()).collect();
    assert_eq!(ids, ["icon_d2", "ecmwf_ifs025"]);
    let icon = &fc.series[0];
    assert_eq!(icon.run_at, 1_791_633_600);
    assert_eq!((icon.step, icon.wind.len()), (3600, 48));
    assert_eq!(icon.wind[0], Some(12.0));
    assert_eq!(icon.wind[10], Some(13.0));
    Ok(())
}

#[tokio::test]
async fn regional_model_outside_its_values_is_left_out() -> anyhow::Result<()> {
    let (_dir, fake, f, _db) = setup().await?;
    // Not in ICON-D2's box at all: only the global model is requested.
    let rhodes = GridPoint::new(36.4, 28.09).unwrap();
    assert_eq!(f.ensure(rhodes, false).await?, None);
    let fc = f.load(rhodes, None).await?;
    assert_eq!(fc.models.len(), 1);
    assert_eq!(fc.series.len(), 1);

    // Inside ICON-D2's box, but without values there (as at a domain edge): stored as
    // empty, left out of the forecast and not requested again.
    let low = GridPoint::new(43.2, 10.0).unwrap();
    assert_eq!(f.ensure(low, false).await?, None);
    let fc = f.load(low, None).await?;
    assert_eq!((fc.models.len(), fc.series.len()), (1, 1));
    assert_eq!(f.ensure(low, false).await?, None);
    assert_eq!(fake.calls.load(Ordering::SeqCst), 2);
    Ok(())
}

#[tokio::test]
async fn reports_an_unreachable_source() -> anyhow::Result<()> {
    let (_dir, fake, f, _db) = setup().await?;
    fake.down.store(true, Ordering::SeqCst);
    let p = GridPoint::new(50.0, 8.0).unwrap();
    assert_eq!(f.ensure(p, false).await?, Some(Notice::SourceUnavailable));
    assert!(f.load(p, None).await?.series.is_empty());
    Ok(())
}

#[tokio::test]
async fn refresh_fetches_new_runs_for_hot_places() -> anyhow::Result<()> {
    let (_dir, fake, f, db) = setup().await?;
    let p = GridPoint::new(48.78, 8.08).unwrap();
    let now = wf_core::time::now();
    assert_eq!(f.detect_runs(now).await?, 2);
    f.ensure(p, true).await?;
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
    // The fake's metadata reports the run already fetched: nothing new.
    assert_eq!(f.detect_runs(now).await?, 0);
    f.refresh(now).await?;
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);

    // A newer ECMWF run: one request for the hot place, with that model only.
    wf_core::forecast::add_run(&db, "ecmwf_ifs025", 1_791_655_200, false).await?;
    f.refresh(now).await?;
    assert_eq!(fake.calls.load(Ordering::SeqCst), 2);
    let fc = f.load(p, None).await?;
    assert_eq!(fc.series[1].run_at, 1_791_655_200);
    assert_eq!(fc.series[0].run_at, 1_791_633_600);
    // Done: the run is complete and not fetched again.
    f.refresh(now).await?;
    assert_eq!(fake.calls.load(Ordering::SeqCst), 2);
    Ok(())
}

#[tokio::test]
async fn an_estimated_run_takes_the_reported_time() -> anyhow::Result<()> {
    let (_dir, _fake, f, db) = setup().await?;
    // A page before the scheduler looked: the run comes from the schedule.
    let p = GridPoint::new(48.78, 8.08).unwrap();
    f.ensure(p, true).await?;
    let guess = wf_core::forecast::latest_run(&db, "icon_d2")
        .await?
        .unwrap();
    assert!(guess.estimated);
    // The metadata reports an older run: the guess takes its time, nothing is new.
    assert_eq!(f.detect_runs(wf_core::time::now()).await?, 0);
    let run = wf_core::forecast::latest_run(&db, "icon_d2")
        .await?
        .unwrap();
    assert_eq!(
        (run.id, run.run_at, run.estimated),
        (guess.id, 1_791_633_600, false)
    );
    assert_eq!(f.load(p, None).await?.series[0].run_at, 1_791_633_600);
    Ok(())
}
