//! Measurements, TAF, history and water gauges, against the stand-ins in `common`.

mod common;

use anyhow::Result;
use axum::http::StatusCode;
use serde_json::{Value, json};
use std::sync::atomic::Ordering;

use common::{app, get_json};

#[tokio::test]
async fn live_fetches_a_new_station_once() -> Result<()> {
    let (_dir, app, calls) = app().await?;
    let (status, body) = get_json(&app, "/api/stations/edsb/live").await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    let obs = body["observations"].as_array().unwrap();
    assert_eq!(obs.len(), 2);
    // Oldest first; variable wind has no direction.
    assert_eq!(
        (obs[0]["wind"].as_f64(), &obs[0]["dir"]),
        (Some(3.0), &Value::Null)
    );
    assert_eq!(obs[1]["wind"].as_f64(), Some(9.0));
    assert_eq!(obs[1]["dir"].as_f64(), Some(210.0));
    assert_eq!(obs[1]["temp"].as_f64(), Some(13.0));
    assert_eq!(
        obs[1]["raw"],
        "METAR EDSB 110850Z AUTO 21009KT CAVOK 13/10 Q1015"
    );
    assert_eq!(calls.metar.load(Ordering::SeqCst), 1);
    let taf = &body["taf"];
    assert_eq!(
        taf["raw"],
        "TAF EDSB 110500Z 1106/1206 22007KT CAVOK BECMG 1117/1120 VRB03KT"
    );
    assert_eq!(taf["groups"][1]["change"], "becoming");
    assert_eq!(taf["hours"].as_array().unwrap().len(), 24);
    assert_eq!(taf["hours"][0]["wind"].as_f64(), Some(7.0));
    assert_eq!(taf["hours"][23]["wind"].as_f64(), Some(3.0));
    assert_eq!(calls.taf.load(Ordering::SeqCst), 1);
    // The history still starts late, but the station was asked a moment ago.
    get_json(&app, "/api/stations/edsb/live").await?;
    assert_eq!(calls.metar.load(Ordering::SeqCst), 1);
    assert_eq!(calls.taf.load(Ordering::SeqCst), 1);

    let (status, _) = get_json(&app, "/api/stations/xxxx/live").await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    Ok(())
}

#[tokio::test]
async fn history_fills_in_past_days() -> Result<()> {
    let (_dir, app, calls) = app().await?;
    let now = wf_core::time::now();
    let day = now - now % 86_400 - 3 * 86_400;
    let uri = format!("/api/stations/edsb/history?from={day}&to={}", day + 86_400);
    let (status, body) = get_json(&app, &uri).await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    let obs = body["observations"].as_array().unwrap();
    assert_eq!(obs.len(), 48);
    assert_eq!(
        (obs[0]["wind"].as_f64(), obs[0]["gust"].as_f64()),
        (Some(12.0), Some(20.0))
    );
    assert_eq!(body["oldest"].as_i64(), Some(day));
    assert_eq!(calls.metar_history.load(Ordering::SeqCst), 1);
    // A day without reports even after the backfill is not asked again within hours.
    let empty = day - 86_400;
    let (_, body) = get_json(
        &app,
        &format!("/api/stations/edsb/history?from={empty}&to={day}"),
    )
    .await?;
    assert_eq!(body["observations"], json!([]));
    assert_eq!(calls.metar_history.load(Ordering::SeqCst), 1);

    for bad in [
        format!("/api/stations/edsb/history?from={day}&to={day}"),
        format!(
            "/api/stations/edsb/history?from={day}&to={}",
            day + 3 * 86_400
        ),
        "/api/stations/edsb/history".to_owned(),
    ] {
        let (status, body) = get_json(&app, &bad).await?;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad}");
        assert_eq!(body, json!({"code": "invalid_range"}));
    }
    let (status, _) = get_json(
        &app,
        &format!("/api/stations/xxxx/history?from={day}&to={}", day + 1),
    )
    .await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    Ok(())
}

#[tokio::test]
async fn gauge_near_a_place() -> Result<()> {
    let (_dir, app, calls) = app().await?;
    // Baden Airpark: Iffezheim on the Rhine, 8 km
    let (status, body) = get_json(&app, "/api/gauges/near?lat=48.7794&lon=8.0805").await?;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        (&body["name"], &body["water"]),
        (&json!("Iffezheim"), &json!("Rhein"))
    );
    assert_eq!(body["distanceKm"].as_f64(), Some(8.5));
    assert_eq!(body["level"]["value"].as_f64(), Some(62.0));
    assert_eq!(
        (&body["levelState"], &body["shippingState"]),
        (&json!("low"), &json!("unknown"))
    );
    assert_eq!(body["flow"]["value"].as_f64(), Some(358.0));
    assert_eq!(body["waterTemp"], Value::Null);
    assert!((body["currentMs"].as_f64().unwrap() - 0.2586).abs() < 0.001);
    // The full hour and the newest value
    let history = body["history"].as_array().unwrap();
    assert_eq!(history.last().unwrap()["cm"].as_f64(), Some(62.0));
    assert_eq!(calls.gauge.load(Ordering::SeqCst), 1);
    get_json(&app, "/api/gauges/near?lat=48.78&lon=8.08").await?;
    assert_eq!(calls.gauge.load(Ordering::SeqCst), 1);

    // Rhodes: none
    let (status, body) = get_json(&app, "/api/gauges/near?lat=36.4&lon=28.1").await?;
    assert_eq!((status, body), (StatusCode::OK, Value::Null));
    let (status, _) = get_json(&app, "/api/gauges/near?lat=95&lon=8").await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    Ok(())
}
