//! Alarms per favourite: what counts (least wind, only with fitting gear), what to tell (ahead
//! from the forecast, live from the station) and by which channel (push, e-mail); the browsers
//! that take push messages; and what was told, so that only news is told again.

pub mod window;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use ts_rs::TS;

use crate::favorite::{Favorite, Sector};
use crate::units::WindUnit;

pub use window::{Hour, Notice, Rule, Told, Window};

/// A favourite's alarm settings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AlarmSettings {
    /// The least mean wind that counts, kn
    pub min_kn: f64,
    /// Only hours some piece of the account's gear fits
    pub gear: bool,
    /// Tell ahead from the forecast, and when that window changes or goes
    pub forecast: bool,
    /// Tell when the station measures it (stations only)
    pub live: bool,
    pub push: bool,
    pub email: bool,
}

impl AlarmSettings {
    pub fn valid(&self) -> bool {
        (0.0..=80.0).contains(&self.min_kn)
            && (self.forecast || self.live)
            && (self.push || self.email)
    }
}

/// An alarm as the app lists it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Alarm {
    #[ts(type = "number")]
    pub favorite_id: i64,
    pub settings: AlarmSettings,
}

/// How the browser shows things, so that messages read the same: language, time zone (IANA),
/// wind unit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ClientPrefs {
    pub locale: String,
    pub time_zone: String,
    pub unit: WindUnit,
}

impl ClientPrefs {
    pub fn valid(&self) -> bool {
        matches!(self.locale.as_str(), "de" | "en")
            && !self.time_zone.is_empty()
            && self.time_zone.len() <= 64
            && self
                .time_zone
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "/_+-".contains(c))
    }
}

/// `PUT /api/alarms/{favorite}`: the settings and the browser's ways.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AlarmInput {
    pub settings: AlarmSettings,
    pub client: ClientPrefs,
}

/// A browser's push subscription (the Push API's `PushSubscription.toJSON()`, flattened).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PushSubscription {
    pub endpoint: String,
    pub p256dh: String,
    pub auth: String,
}

impl PushSubscription {
    pub fn valid(&self) -> bool {
        known_push_service(&self.endpoint)
            && self.endpoint.len() <= 1024
            && (1..=200).contains(&self.p256dh.len())
            && (1..=100).contains(&self.auth.len())
    }
}

/// The push services browsers subscribe with, by host and its subdomains: Chrome, Edge, Opera,
/// Samsung Internet and Brave use Google's (FCM), Firefox Mozilla's (autopush), Safari Apple's,
/// old Edge Windows' (WNS). The server posts to the endpoint a browser gives; with any address
/// allowed, a signed-in user could make it post into the network it runs in.
const PUSH_SERVICES: &[&str] = &[
    "fcm.googleapis.com",
    "android.googleapis.com",
    "push.services.mozilla.com",
    "push.apple.com",
    "notify.windows.com",
];

/// Most browsers an account takes push messages in; a new one drops the oldest.
pub const MAX_PUSH_SUBSCRIPTIONS: i64 = 10;

/// Whether the endpoint is an https address on a known push service, without user, port or
/// anything else in the address's authority that could point it elsewhere.
pub fn known_push_service(endpoint: &str) -> bool {
    let Some(rest) = endpoint.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if host.is_empty()
        || !host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-.".contains(c))
    {
        return false;
    }
    let host = host.to_ascii_lowercase();
    PUSH_SERVICES.iter().any(|s| {
        host == *s
            || host
                .strip_suffix(s)
                .is_some_and(|sub| sub.len() > 1 && sub.ends_with('.'))
    })
}

type SettingsRow = (f64, bool, bool, bool, bool, bool);

fn settings((min_kn, gear, forecast, live, push, email): SettingsRow) -> AlarmSettings {
    AlarmSettings {
        min_kn,
        gear,
        forecast,
        live,
        push,
        email,
    }
}

pub async fn list(db: &SqlitePool, account_id: i64) -> sqlx::Result<Vec<Alarm>> {
    let rows: Vec<(i64, f64, bool, bool, bool, bool, bool)> = sqlx::query_as(
        "SELECT a.favorite_id, a.min_kn, a.gear, a.forecast, a.live, a.push, a.email \
         FROM alarm a JOIN favorite f ON f.id = a.favorite_id WHERE f.account_id = ? \
         ORDER BY f.position",
    )
    .bind(account_id)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(favorite_id, a, b, c, d, e, f)| Alarm {
            favorite_id,
            settings: settings((a, b, c, d, e, f)),
        })
        .collect())
}

/// Sets a favourite's alarm; false when the favourite is not the account's.
pub async fn set(
    db: &SqlitePool,
    account_id: i64,
    favorite_id: i64,
    s: &AlarmSettings,
    now: i64,
) -> sqlx::Result<bool> {
    let done = sqlx::query(
        "INSERT INTO alarm (favorite_id, min_kn, gear, forecast, live, push, email, created_at) \
         SELECT id, ?, ?, ?, ?, ?, ?, ? FROM favorite WHERE id = ? AND account_id = ? \
         ON CONFLICT (favorite_id) DO UPDATE SET min_kn = excluded.min_kn, gear = excluded.gear, \
         forecast = excluded.forecast, live = excluded.live, push = excluded.push, \
         email = excluded.email",
    )
    .bind(s.min_kn)
    .bind(s.gear)
    .bind(s.forecast)
    .bind(s.live)
    .bind(s.push)
    .bind(s.email)
    .bind(now)
    .bind(favorite_id)
    .bind(account_id)
    .execute(db)
    .await?;
    Ok(done.rows_affected() > 0)
}

pub async fn remove(db: &SqlitePool, account_id: i64, favorite_id: i64) -> sqlx::Result<bool> {
    let done = sqlx::query(
        "DELETE FROM alarm WHERE favorite_id = ? \
         AND favorite_id IN (SELECT id FROM favorite WHERE account_id = ?)",
    )
    .bind(favorite_id)
    .bind(account_id)
    .execute(db)
    .await?;
    Ok(done.rows_affected() > 0)
}

pub async fn set_client(db: &SqlitePool, account_id: i64, c: &ClientPrefs) -> sqlx::Result<()> {
    sqlx::query("UPDATE account SET locale = ?, time_zone = ?, unit = ? WHERE id = ?")
        .bind(&c.locale)
        .bind(&c.time_zone)
        .bind(unit_str(c.unit))
        .bind(account_id)
        .execute(db)
        .await?;
    Ok(())
}

/// The account's language, time zone and unit as last set.
pub async fn client(db: &SqlitePool, account_id: i64) -> sqlx::Result<Option<ClientPrefs>> {
    let row: Option<(String, String, String)> =
        sqlx::query_as("SELECT locale, time_zone, unit FROM account WHERE id = ?")
            .bind(account_id)
            .fetch_optional(db)
            .await?;
    Ok(row.map(|(locale, time_zone, unit)| ClientPrefs {
        locale,
        time_zone,
        unit: parse_unit(&unit),
    }))
}

fn unit_str(u: WindUnit) -> &'static str {
    match u {
        WindUnit::Kn => "kn",
        WindUnit::Ms => "ms",
        WindUnit::Kmh => "kmh",
    }
}

fn parse_unit(s: &str) -> WindUnit {
    match s {
        "ms" => WindUnit::Ms,
        "kmh" => WindUnit::Kmh,
        _ => WindUnit::Kn,
    }
}

/// Adds a browser to the account's push targets (a browser that belonged to another account
/// moves over).
pub async fn subscribe(
    db: &SqlitePool,
    account_id: i64,
    s: &PushSubscription,
    now: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO push_subscription (endpoint, account_id, p256dh, auth, created_at) \
         VALUES (?, ?, ?, ?, ?) ON CONFLICT (endpoint) DO UPDATE SET \
         account_id = excluded.account_id, p256dh = excluded.p256dh, auth = excluded.auth, \
         created_at = excluded.created_at",
    )
    .bind(&s.endpoint)
    .bind(account_id)
    .bind(&s.p256dh)
    .bind(&s.auth)
    .bind(now)
    .execute(db)
    .await?;
    // A browser that was reset or reinstalled leaves its old subscription behind: the ones not
    // turned on longest go.
    sqlx::query(
        "DELETE FROM push_subscription WHERE account_id = ?1 AND endpoint NOT IN \
         (SELECT endpoint FROM push_subscription WHERE account_id = ?1 \
          ORDER BY endpoint = ?2 DESC, created_at DESC LIMIT ?3)",
    )
    .bind(account_id)
    .bind(&s.endpoint)
    .bind(MAX_PUSH_SUBSCRIPTIONS)
    .execute(db)
    .await?;
    Ok(())
}

/// Drops a push target: the user turned it off, or the push service says it is gone.
pub async fn unsubscribe(db: &SqlitePool, endpoint: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM push_subscription WHERE endpoint = ?")
        .bind(endpoint)
        .execute(db)
        .await?;
    Ok(())
}

/// Drops one of the account's push targets.
pub async fn unsubscribe_own(db: &SqlitePool, account_id: i64, endpoint: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM push_subscription WHERE endpoint = ? AND account_id = ?")
        .bind(endpoint)
        .bind(account_id)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn subscriptions(
    db: &SqlitePool,
    account_id: i64,
) -> sqlx::Result<Vec<PushSubscription>> {
    let rows: Vec<(String, String, String)> =
        sqlx::query_as("SELECT endpoint, p256dh, auth FROM push_subscription WHERE account_id = ?")
            .bind(account_id)
            .fetch_all(db)
            .await?;
    Ok(rows
        .into_iter()
        .map(|(endpoint, p256dh, auth)| PushSubscription {
            endpoint,
            p256dh,
            auth,
        })
        .collect())
}

/// An alarm to check, with all it needs.
#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub favorite: Favorite,
    pub settings: AlarmSettings,
    pub account_id: i64,
    pub email: Option<String>,
    pub locale: String,
    pub time_zone: String,
    pub unit: WindUnit,
}

#[derive(sqlx::FromRow)]
struct JobRow {
    id: i64,
    slug: Option<String>,
    lat: f64,
    lon: f64,
    name: String,
    wind_from: Option<i64>,
    wind_to: Option<i64>,
    min_kn: f64,
    gear: bool,
    forecast: bool,
    live: bool,
    push: bool,
    email: bool,
    account_id: i64,
    account_email: Option<String>,
    locale: String,
    time_zone: String,
    unit: String,
}

/// Every alarm.
pub async fn jobs(db: &SqlitePool) -> sqlx::Result<Vec<Job>> {
    let rows: Vec<JobRow> = sqlx::query_as(
        "SELECT f.id, f.slug, f.lat, f.lon, f.name, f.wind_from, f.wind_to, \
         a.min_kn, a.gear, a.forecast, a.live, a.push, a.email, \
         c.id AS account_id, \
         CASE WHEN c.email_confirmed THEN c.email END AS account_email, c.locale, c.time_zone, c.unit \
         FROM alarm a JOIN favorite f ON f.id = a.favorite_id JOIN account c ON c.id = f.account_id",
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Job {
            favorite: Favorite {
                id: r.id,
                slug: r.slug,
                lat: r.lat,
                lon: r.lon,
                name: r.name,
                sector: match (r.wind_from, r.wind_to) {
                    (Some(from), Some(to)) => Some(Sector {
                        from: from as u16,
                        to: to as u16,
                    }),
                    _ => None,
                },
            },
            settings: settings((r.min_kn, r.gear, r.forecast, r.live, r.push, r.email)),
            account_id: r.account_id,
            email: r.account_email,
            locale: r.locale,
            time_zone: r.time_zone,
            unit: parse_unit(&r.unit),
        })
        .collect())
}

/// What kind of message
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Window,
    Live,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Window => "window",
            Kind::Live => "live",
        }
    }
}

/// What was last told about a favourite's day; None when nothing was.
pub async fn told(
    db: &SqlitePool,
    favorite_id: i64,
    day: i64,
    kind: Kind,
) -> sqlx::Result<Option<Told>> {
    let row: Option<(Option<i64>, Option<i64>, Option<f64>)> = sqlx::query_as(
        "SELECT start_at, end_at, peak_kn FROM alarm_sent WHERE favorite_id = ? AND day = ? AND kind = ?",
    )
    .bind(favorite_id)
    .bind(day)
    .bind(kind.as_str())
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| match r {
        (Some(start), Some(end), Some(peak_kn)) => Some(Window {
            start,
            end,
            peak_kn,
            peak_at: start,
            dir: 0.0,
        }),
        _ => None,
    }))
}

/// Notes what was told about a day: a window, or (None) that it is gone or the live alarm.
pub async fn tell(
    db: &SqlitePool,
    favorite_id: i64,
    day: i64,
    kind: Kind,
    window: Option<&Window>,
    now: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO alarm_sent (favorite_id, day, kind, start_at, end_at, peak_kn, sent_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT (favorite_id, day, kind) DO UPDATE SET \
         start_at = excluded.start_at, end_at = excluded.end_at, peak_kn = excluded.peak_kn, \
         sent_at = excluded.sent_at",
    )
    .bind(favorite_id)
    .bind(day)
    .bind(kind.as_str())
    .bind(window.map(|w| w.start))
    .bind(window.map(|w| w.end))
    .bind(window.map(|w| w.peak_kn))
    .bind(now)
    .execute(db)
    .await?;
    Ok(())
}

/// Drops the record of days long past.
pub async fn forget(db: &SqlitePool, before_day: i64) -> sqlx::Result<u64> {
    Ok(sqlx::query("DELETE FROM alarm_sent WHERE day < ?")
        .bind(before_day)
        .execute(db)
        .await?
        .rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::favorite::FavoriteInput;

    #[tokio::test]
    async fn keeps_alarms_targets_and_what_was_told() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db = crate::db::connect(&dir.path().join("t.db")).await?;
        crate::db::migrate(&db).await?;
        let me = crate::account::create(&db, "u1", "Ana", "c1", "{}", 0).await?;
        let other = crate::account::create(&db, "u2", "Ben", "c2", "{}", 0).await?;
        let input = FavoriteInput {
            slug: None,
            lat: 48.78,
            lon: 8.08,
            name: "See".into(),
            sector: Some(Sector { from: 200, to: 300 }),
        };
        let fav = crate::favorite::add(&db, me, &input, 1)
            .await?
            .expect("added");
        let s = AlarmSettings {
            min_kn: 14.0,
            gear: true,
            forecast: true,
            live: false,
            push: true,
            email: false,
        };
        assert!(s.valid());
        assert!(!AlarmSettings { push: false, ..s }.valid());
        // Only on one's own favourites
        assert!(!set(&db, other, fav.id, &s, 2).await?);
        assert!(set(&db, me, fav.id, &s, 2).await?);
        assert!(set(&db, me, fav.id, &AlarmSettings { min_kn: 16.0, ..s }, 3).await?);
        assert_eq!(list(&db, me).await?[0].settings.min_kn, 16.0);
        let prefs = ClientPrefs {
            locale: "en".into(),
            time_zone: "Europe/Athens".into(),
            unit: WindUnit::Ms,
        };
        assert!(prefs.valid());
        assert!(
            !ClientPrefs {
                time_zone: "x'; --".into(),
                ..prefs.clone()
            }
            .valid()
        );
        set_client(&db, me, &prefs).await?;
        let job = &jobs(&db).await?[0];
        assert_eq!(
            (job.favorite.sector, job.locale.as_str(), job.unit),
            (Some(Sector { from: 200, to: 300 }), "en", WindUnit::Ms)
        );

        let sub = PushSubscription {
            endpoint: "https://push.example/a".into(),
            p256dh: "k".into(),
            auth: "a".into(),
        };
        subscribe(&db, other, &sub, 1).await?;
        subscribe(&db, me, &sub, 2).await?;
        assert_eq!(
            (
                subscriptions(&db, me).await?.len(),
                subscriptions(&db, other).await?.len()
            ),
            (1, 0)
        );
        unsubscribe(&db, &sub.endpoint).await?;
        assert!(subscriptions(&db, me).await?.is_empty());

        let w = Window {
            start: 100,
            end: 200,
            peak_kn: 18.0,
            peak_at: 100,
            dir: 0.0,
        };
        assert_eq!(told(&db, fav.id, 20261012, Kind::Window).await?, None);
        tell(&db, fav.id, 20261012, Kind::Window, Some(&w), 5).await?;
        assert_eq!(
            told(&db, fav.id, 20261012, Kind::Window).await?,
            Some(Some(w))
        );
        tell(&db, fav.id, 20261012, Kind::Window, None, 6).await?;
        assert_eq!(told(&db, fav.id, 20261012, Kind::Window).await?, Some(None));
        assert_eq!(forget(&db, 20261013).await?, 1);
        // Gone with the favourite
        assert!(remove(&db, me, fav.id).await?);
        assert!(list(&db, me).await?.is_empty());
        Ok(())
    }

    #[test]
    fn push_goes_only_to_the_known_services() {
        for ok in [
            "https://fcm.googleapis.com/fcm/send/abc",
            "https://updates.push.services.mozilla.com/wpush/v2/abc",
            "https://web.push.apple.com/QGh",
            "https://wns2-par02p.notify.windows.com/w/?token=abc",
            "https://FCM.googleapis.com/wp/abc",
        ] {
            assert!(known_push_service(ok), "{ok}");
        }
        for bad in [
            "http://fcm.googleapis.com/fcm/send/abc",
            "https://192.168.1.1/x",
            "https://fcm.googleapis.com.evil.example/x",
            "https://evilfcm.googleapis.com/x",
            "https://user@fcm.googleapis.com/x",
            "https://fcm.googleapis.com:8443/x",
            "https://evil.example/fcm.googleapis.com",
            "https://evil.example?fcm.googleapis.com",
            "https://.push.apple.com/x",
            "https://",
        ] {
            assert!(!known_push_service(bad), "{bad}");
        }
    }

    #[tokio::test]
    async fn an_account_keeps_its_newest_browsers() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let db = crate::db::connect(&dir.path().join("t.db")).await?;
        crate::db::migrate(&db).await?;
        let me = crate::account::create(&db, "u1", "Ana", "c1", "{}", 0).await?;
        let sub = |i: i64| PushSubscription {
            endpoint: format!("https://fcm.googleapis.com/fcm/send/{i}"),
            p256dh: "k".into(),
            auth: "a".into(),
        };
        for i in 0..15 {
            subscribe(&db, me, &sub(i), i).await?;
        }
        let kept: Vec<String> = subscriptions(&db, me)
            .await?
            .into_iter()
            .map(|s| s.endpoint)
            .collect();
        assert_eq!(kept.len(), MAX_PUSH_SUBSCRIPTIONS as usize);
        assert!(!kept.contains(&sub(4).endpoint) && kept.contains(&sub(5).endpoint));
        // A browser that subscribes again counts as new: the next oldest goes instead.
        subscribe(&db, me, &sub(5), 99).await?;
        subscribe(&db, me, &sub(15), 100).await?;
        let kept: Vec<String> = subscriptions(&db, me)
            .await?
            .into_iter()
            .map(|s| s.endpoint)
            .collect();
        assert!(kept.contains(&sub(15).endpoint) && !kept.contains(&sub(6).endpoint));
        Ok(())
    }
}
