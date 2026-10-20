//! What the data holds, counted for the admin page: model runs, places kept current, station
//! lists, measurements, gauges, statistics, verification, rows per table.

use serde::Serialize;
use sqlx::SqlitePool;
use ts_rs::TS;

/// A model's newest run.
#[derive(Debug, Clone, PartialEq, Serialize, TS, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ModelRunState {
    pub model: String,
    #[ts(type = "number | null")]
    pub run_at: Option<i64>,
    /// When its fetching finished; null while places are still being fetched
    #[ts(type = "number | null")]
    pub ingested_at: Option<i64>,
    /// Its time was taken from the model's schedule: Open-Meteo's run metadata did not answer
    pub estimated: bool,
    /// Places with this run's series stored
    #[ts(type = "number")]
    pub places: i64,
}

pub async fn model_runs(db: &SqlitePool) -> sqlx::Result<Vec<ModelRunState>> {
    sqlx::query_as(
        "SELECT m.id AS model, r.run_at, r.ingested_at, COALESCE(r.estimated, 0) AS estimated, \
         (SELECT COUNT(*) FROM forecast_series s WHERE s.run_id = r.id) AS places \
         FROM model m LEFT JOIN model_run r ON r.id = \
           (SELECT id FROM model_run WHERE model_id = m.id ORDER BY run_at DESC LIMIT 1) \
         ORDER BY m.rank",
    )
    .fetch_all(db)
    .await
}

/// A place kept current: a station's grid point or a point someone opened.
#[derive(Debug, Clone, PartialEq, Serialize, TS, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HotPlace {
    pub lat: f64,
    pub lon: f64,
    /// The station there, if any
    pub name: Option<String>,
    pub tracked: bool,
    #[ts(type = "number | null")]
    pub requested_at: Option<i64>,
}

/// The places refreshed after every run, tracked stations first, then the most recently opened;
/// at most `limit`, and how many there are.
pub async fn hot_places(
    db: &SqlitePool,
    since: i64,
    limit: i64,
) -> sqlx::Result<(i64, Vec<HotPlace>)> {
    let total = crate::forecast::hot_places(db, since).await?.len() as i64;
    let rows = sqlx::query_as(
        "SELECT lat_e2 / 100.0 AS lat, lon_e2 / 100.0 AS lon, \
           (SELECT s.name FROM station s WHERE CAST(round(s.lat * 100) AS INTEGER) = h.lat_e2 \
              AND CAST(round(s.lon * 100) AS INTEGER) = h.lon_e2 ORDER BY s.tracked DESC LIMIT 1) AS name, \
           MAX(tracked) = 1 AS tracked, NULLIF(MAX(at), 0) AS requested_at FROM ( \
           SELECT CAST(round(lat * 100) AS INTEGER) AS lat_e2, \
                  CAST(round(lon * 100) AS INTEGER) AS lon_e2, 1 AS tracked, 0 AS at \
           FROM station WHERE tracked \
           UNION ALL \
           SELECT lat_e2, lon_e2, 0, requested_at FROM place WHERE requested_at >= ? \
         ) h GROUP BY lat_e2, lon_e2 ORDER BY MAX(tracked) DESC, MAX(at) DESC LIMIT ?",
    )
    .bind(since)
    .bind(limit)
    .fetch_all(db)
    .await?;
    Ok((total, rows))
}

/// A station list: stations from a source and when the last import listed them.
#[derive(Debug, Clone, PartialEq, Serialize, TS, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StationList {
    /// "metar", "dwd" or "owm"
    pub source: String,
    #[ts(type = "number")]
    pub stations: i64,
    #[ts(type = "number")]
    pub seen_at: i64,
}

pub async fn station_lists(db: &SqlitePool) -> sqlx::Result<Vec<StationList>> {
    sqlx::query_as(
        "SELECT source, COUNT(*) AS stations, COALESCE(MAX(seen_at), 0) AS seen_at \
         FROM station GROUP BY source ORDER BY source DESC",
    )
    .fetch_all(db)
    .await
}

/// A tracked station: whether the slug names one.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TrackedStation {
    pub slug: String,
    /// None: no station with this slug
    pub name: Option<String>,
    pub code: Option<String>,
}

pub async fn tracked(db: &SqlitePool, slugs: &[String]) -> sqlx::Result<Vec<TrackedStation>> {
    let mut out = Vec::new();
    for slug in slugs {
        let row: Option<(String, String)> =
            sqlx::query_as("SELECT name, code FROM station WHERE slug = ?")
                .bind(slug.trim().to_ascii_lowercase())
                .fetch_optional(db)
                .await?;
        out.push(TrackedStation {
            slug: slug.clone(),
            name: row.as_ref().map(|r| r.0.clone()),
            code: row.map(|r| r.1),
        });
    }
    Ok(out)
}

/// Measurements and what is made from them.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MeasurementCounts {
    /// Measurements of the last hour, by stations' source
    #[ts(type = "number")]
    pub metar_hour: i64,
    #[ts(type = "number")]
    pub dwd_hour: i64,
    /// TAFs valid now
    #[ts(type = "number")]
    pub tafs: i64,
    #[ts(type = "number")]
    pub gauges: i64,
    /// Gauges with a level of the last three hours
    #[ts(type = "number")]
    pub gauges_fresh: i64,
    /// Stations with ten-year statistics made
    #[ts(type = "number")]
    pub wind_stats: i64,
    /// The newest UTC day verified (yyyymmdd) and how many stations it had
    pub verified_day: Option<u32>,
    #[ts(type = "number")]
    pub verified_stations: i64,
}

pub async fn measurement_counts(db: &SqlitePool, now: i64) -> sqlx::Result<MeasurementCounts> {
    let hour = |source: &'static str| async move {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM observation o JOIN station s ON s.id = o.station_id \
             WHERE s.source = ? AND o.observed_at > ?",
        )
        .bind(source)
        .bind(now - 3_600)
        .fetch_one(db)
        .await
    };
    let scalar = |sql: &'static str| async move {
        sqlx::query_scalar::<_, i64>(sql)
            .bind(now)
            .fetch_one(db)
            .await
    };
    let verified: Option<(u32, i64)> = sqlx::query_as(
        "SELECT day, COUNT(DISTINCT station_id) FROM verification_daily \
         GROUP BY day ORDER BY day DESC LIMIT 1",
    )
    .fetch_optional(db)
    .await?;
    Ok(MeasurementCounts {
        metar_hour: hour("metar").await?,
        dwd_hour: hour("dwd").await?,
        tafs: scalar("SELECT COUNT(*) FROM taf WHERE valid_to > ?").await?,
        gauges: scalar("SELECT COUNT(*) FROM gauge WHERE ? IS NOT NULL").await?,
        gauges_fresh: scalar("SELECT COUNT(*) FROM gauge WHERE level_at > ? - 3 * 3600").await?,
        wind_stats: scalar(
            "SELECT COUNT(*) FROM wind_stats WHERE stats IS NOT NULL AND ? IS NOT NULL",
        )
        .await?,
        verified_day: verified.map(|v| v.0),
        verified_stations: verified.map_or(0, |v| v.1),
    })
}

/// Rows per table, for the storage section.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TableRows {
    pub table: String,
    #[ts(type = "number")]
    pub rows: i64,
}

/// The tables the admin page lists, largest first in practice.
const TABLES: [&str; 14] = [
    "observation",
    "forecast_series",
    "verification_daily",
    "taf",
    "place",
    "station",
    "gauge",
    "wind_stats",
    "model_run",
    "account",
    "session",
    "favorite",
    "alarm_delivery",
    "admin_log",
];

pub async fn table_rows(db: &SqlitePool) -> sqlx::Result<Vec<TableRows>> {
    let mut out = Vec::new();
    for t in TABLES {
        // The names are ours, not input
        let rows: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {t}"))
            .fetch_one(db)
            .await?;
        out.push(TableRows {
            table: t.to_owned(),
            rows,
        });
    }
    Ok(out)
}

/// Alarms and the ways they go out.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AlarmCounts {
    #[ts(type = "number")]
    pub alarms: i64,
    /// Spots with an alarm
    #[ts(type = "number")]
    pub spots: i64,
    #[ts(type = "number")]
    pub accounts: i64,
    /// Addresses set, and those confirmed
    #[ts(type = "number")]
    pub addresses: i64,
    #[ts(type = "number")]
    pub confirmed: i64,
    /// Browsers taking push messages, by push service ("google", "mozilla", "apple",
    /// "microsoft", "other")
    #[ts(type = "Array<[string, number]>")]
    pub devices: Vec<(String, i64)>,
}

pub async fn alarm_counts(db: &SqlitePool) -> sqlx::Result<AlarmCounts> {
    let (alarms, spots, accounts): (i64, i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COUNT(DISTINCT a.favorite_id), COUNT(DISTINCT f.account_id) \
         FROM alarm a JOIN favorite f ON f.id = a.favorite_id",
    )
    .fetch_one(db)
    .await?;
    let (addresses, confirmed): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(email), COALESCE(SUM(email IS NOT NULL AND email_confirmed), 0) FROM account",
    )
    .fetch_one(db)
    .await?;
    let devices = sqlx::query_as(
        "SELECT CASE \
           WHEN endpoint LIKE '%googleapis.com/%' THEN 'google' \
           WHEN endpoint LIKE '%mozilla.com/%' THEN 'mozilla' \
           WHEN endpoint LIKE '%push.apple.com/%' THEN 'apple' \
           WHEN endpoint LIKE '%notify.windows.com/%' THEN 'microsoft' \
           ELSE 'other' END AS service, COUNT(*) FROM push_subscription \
         GROUP BY service ORDER BY COUNT(*) DESC",
    )
    .fetch_all(db)
    .await?;
    Ok(AlarmCounts {
        alarms,
        spots,
        accounts,
        addresses,
        confirmed,
        devices,
    })
}

/// What a cleanup would remove now: sessions and links that ran out, accounts not seen for
/// a year.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Stale {
    #[ts(type = "number")]
    pub sessions: i64,
    #[ts(type = "number")]
    pub links: i64,
    #[ts(type = "number")]
    pub inactive_accounts: i64,
}

pub async fn stale(db: &SqlitePool, now: i64) -> sqlx::Result<Stale> {
    let one = |sql: &'static str| async move {
        sqlx::query_scalar::<_, i64>(sql)
            .bind(now)
            .fetch_one(db)
            .await
    };
    Ok(Stale {
        sessions: one("SELECT COUNT(*) FROM session WHERE expires_at <= ?").await?,
        links: one("SELECT COUNT(*) FROM email_confirmation WHERE expires_at <= ?").await?,
        inactive_accounts: one(
            "SELECT COUNT(*) FROM account WHERE COALESCE(last_seen_at, created_at) < ? - 365 * 86400",
        )
        .await?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn counts_on_an_empty_database() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db = crate::db::connect(&dir.path().join("t.db")).await?;
        crate::db::migrate(&db).await?;
        let runs = model_runs(&db).await?;
        assert_eq!(runs.len(), 7);
        assert_eq!(runs[0].model, "icon_d2");
        assert_eq!(runs[0].run_at, None);
        let (n, places) = hot_places(&db, 0, 10).await?;
        assert_eq!((n, places.len()), (0, 0));
        assert!(station_lists(&db).await?.is_empty());
        let t = tracked(&db, &["edsb".into()]).await?;
        assert_eq!(t[0].name, None);
        let m = measurement_counts(&db, 1_000).await?;
        assert_eq!((m.metar_hour, m.verified_day), (0, None));
        assert_eq!(table_rows(&db).await?.len(), TABLES.len());
        let a = alarm_counts(&db).await?;
        assert_eq!((a.alarms, a.devices.len()), (0, 0));
        crate::account::create(&db, "h", "Ann", "k", "{}", 0).await?;
        assert_eq!(stale(&db, 400 * 86_400).await?.inactive_accounts, 1);
        Ok(())
    }
}
