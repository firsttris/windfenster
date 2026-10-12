//! Alarms and push targets over HTTP: only on one's own favourites, settings checked, the
//! browser's language and time zone kept for the messages.

mod common;

use anyhow::Result;
use axum::http::StatusCode;
use common::session::{call, signed_in};
use serde_json::{Value, json};

#[tokio::test]
async fn alarms_on_ones_own_favourites() -> Result<()> {
    let (dir, app, _) = common::app().await?;
    // Without alarms set up the server says it has no push key and no e-mail.
    let (_, info) = call(&app, "GET", "/api/push", None, false).await?;
    assert_eq!(info, json!({"key": null, "email": false}));
    let (status, _) = call(&app, "GET", "/api/alarms", None, true).await?;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let me = signed_in(&dir).await?;
    let spot = json!({"slug": "edsb", "lat": 0, "lon": 0, "name": "Airpark", "sector": null});
    let (_, fav) = call(&app, "POST", "/api/favorites", Some(spot), true).await?;
    let path = format!("/api/alarms/{}", fav["id"]);

    let settings = json!({"minKn": 14, "gear": false, "forecast": true, "live": true, "push": true, "email": false});
    let client = json!({"locale": "en", "timeZone": "Europe/Athens", "unit": "ms"});
    let input = json!({"settings": settings, "client": client});
    let (status, got) = call(&app, "PUT", &path, Some(input), true).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        got,
        json!({"favoriteId": fav["id"], "settings": {"minKn": 14.0, "gear": false, "forecast": true, "live": true, "push": true, "email": false}})
    );
    let (_, list) = call(&app, "GET", "/api/alarms", None, true).await?;
    assert_eq!(list.as_array().map(Vec::len), Some(1));
    let db = wf_core::db::connect(&dir.path().join("test.db")).await?;
    let prefs = wf_core::alarm::client(&db, me).await?.unwrap();
    assert_eq!(
        (prefs.locale.as_str(), prefs.time_zone.as_str()),
        ("en", "Europe/Athens")
    );

    // No channel, an unknown time zone, someone else's favourite
    let none = json!({"settings": {"minKn": 14, "gear": false, "forecast": true, "live": false, "push": false, "email": false}, "client": client});
    let (status, body) = call(&app, "PUT", &path, Some(none), true).await?;
    assert_eq!(
        (status, body),
        (StatusCode::BAD_REQUEST, json!({"code": "invalid_alarm"}))
    );
    let bad_tz = json!({"settings": settings, "client": {"locale": "de", "timeZone": "Mars/Olympus", "unit": "kn"}});
    let (status, _) = call(&app, "PUT", &path, Some(bad_tz), true).await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let other = json!({"settings": settings, "client": client});
    let (status, _) = call(&app, "PUT", "/api/alarms/999", Some(other), true).await?;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // A browser takes the push messages, and stops.
    let sub = json!({"endpoint": "https://push.example/abc", "p256dh": "key", "auth": "secret"});
    let (status, _) = call(&app, "POST", "/api/push/subscriptions", Some(sub), true).await?;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(wf_core::alarm::subscriptions(&db, me).await?.len(), 1);
    let plain = json!({"endpoint": "http://push.example/abc", "p256dh": "key", "auth": "secret"});
    let (status, body) = call(&app, "POST", "/api/push/subscriptions", Some(plain), true).await?;
    assert_eq!(
        (status, body),
        (
            StatusCode::BAD_REQUEST,
            json!({"code": "invalid_subscription"})
        )
    );
    let gone = json!({"endpoint": "https://push.example/abc"});
    call(&app, "DELETE", "/api/push/subscriptions", Some(gone), true).await?;
    assert!(wf_core::alarm::subscriptions(&db, me).await?.is_empty());
    // Without alarms set up a test reaches nothing.
    let (_, reached) = call(&app, "POST", "/api/alarms/test", None, true).await?;
    assert_eq!(reached, json!({"browsers": 0, "email": false}));

    let (status, body) = call(&app, "DELETE", &path, None, true).await?;
    assert_eq!((status, body), (StatusCode::NO_CONTENT, Value::Null));
    let (status, _) = call(&app, "DELETE", &path, None, true).await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    Ok(())
}
