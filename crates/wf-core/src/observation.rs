//! Measured wind at stations.

use std::collections::HashMap;

use serde::Serialize;
use sqlx::SqlitePool;
use ts_rs::TS;

use crate::station::StationSource;

/// One measurement. Wind and gusts in knots.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Observation {
    #[ts(type = "number")]
    pub at: i64,
    pub wind: Option<f32>,
    pub gust: Option<f32>,
    /// Where the wind comes from, degrees; null when variable.
    pub dir: Option<f32>,
    pub temp: Option<f32>,
    /// The METAR as sent.
    pub raw: Option<String>,
}

#[derive(sqlx::FromRow)]
struct Row {
    observed_at: i64,
    wind_kn: Option<f64>,
    gust_kn: Option<f64>,
    dir_deg: Option<f64>,
    temp_c: Option<f64>,
    raw: Option<String>,
}

impl From<Row> for Observation {
    fn from(r: Row) -> Self {
        Self {
            at: r.observed_at,
            wind: r.wind_kn.map(|v| v as f32),
            gust: r.gust_kn.map(|v| v as f32),
            dir: r.dir_deg.map(|v| v as f32),
            temp: r.temp_c.map(|v| v as f32),
            raw: r.raw,
        }
    }
}

/// Stores measurements; a repeated one (same station and time) replaces the earlier, so a
/// corrected METAR wins. Returns how many were written.
pub async fn store(db: &SqlitePool, rows: &[(i64, Observation)]) -> sqlx::Result<usize> {
    let mut tx = db.begin().await?;
    for (station_id, o) in rows {
        sqlx::query(
            "INSERT OR REPLACE INTO observation (station_id, observed_at, wind_kn, gust_kn, dir_deg, \
             temp_c, raw) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(station_id)
        .bind(o.at)
        .bind(o.wind)
        .bind(o.gust)
        .bind(o.dir)
        .bind(o.temp)
        .bind(&o.raw)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(rows.len())
}

/// Measurements of a station since `since`, oldest first.
pub async fn since(db: &SqlitePool, station_id: i64, since: i64) -> sqlx::Result<Vec<Observation>> {
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT observed_at, wind_kn, gust_kn, dir_deg, temp_c, raw FROM observation \
         WHERE station_id = ? AND observed_at >= ? ORDER BY observed_at",
    )
    .bind(station_id)
    .bind(since)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(Observation::from).collect())
}

/// Measurements of a station in [from, to), oldest first.
pub async fn between(
    db: &SqlitePool,
    station_id: i64,
    from: i64,
    to: i64,
) -> sqlx::Result<Vec<Observation>> {
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT observed_at, wind_kn, gust_kn, dir_deg, temp_c, raw FROM observation \
         WHERE station_id = ? AND observed_at >= ? AND observed_at < ? ORDER BY observed_at",
    )
    .bind(station_id)
    .bind(from)
    .bind(to)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(Observation::from).collect())
}

/// The time of a station's oldest stored measurement.
pub async fn oldest(db: &SqlitePool, station_id: i64) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar("SELECT MIN(observed_at) FROM observation WHERE station_id = ?")
        .bind(station_id)
        .fetch_one(db)
        .await
}

/// A measurement counts for a forecast hour this close to it (METARs come at :20 and :50).
pub const MATCH_S: i64 = 20 * 60;

/// The measurement closest to `time` within `within` seconds; `obs` oldest first.
pub fn nearest(obs: &[Observation], time: i64, within: i64) -> Option<&Observation> {
    obs.iter()
        .filter(|o| (o.at - time).abs() <= within)
        .min_by_key(|o| (o.at - time).abs())
}

/// Station ids of a source by the source's code (ICAO code, DWD number).
pub async fn station_ids(
    db: &SqlitePool,
    source: StationSource,
) -> sqlx::Result<HashMap<String, i64>> {
    let rows: Vec<(String, i64)> = sqlx::query_as("SELECT code, id FROM station WHERE source = ?")
        .bind(source)
        .fetch_all(db)
        .await?;
    Ok(rows.into_iter().collect())
}

/// A station with its newest measurement, for the map's pins.
#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StationWind {
    pub slug: String,
    pub source: StationSource,
    pub code: String,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    /// When the newest measurement since the asked time was taken; null without one
    #[ts(type = "number | null")]
    pub at: Option<i64>,
    /// kn
    pub wind: Option<f32>,
    pub gust: Option<f32>,
    /// Where the wind comes from, degrees; null when variable
    pub dir: Option<f32>,
}

/// Every station with its newest measurement taken at `since` or later.
pub async fn newest_per_station(db: &SqlitePool, since: i64) -> sqlx::Result<Vec<StationWind>> {
    // One index lookup per station: the primary key is (station_id, observed_at).
    sqlx::query_as(
        "SELECT s.slug, s.source, s.code, s.name, s.lat, s.lon, o.observed_at AS at, \
         o.wind_kn AS wind, o.gust_kn AS gust, o.dir_deg AS dir \
         FROM station s LEFT JOIN observation o ON o.station_id = s.id AND o.observed_at = \
         (SELECT MAX(observed_at) FROM observation WHERE station_id = s.id AND observed_at >= ?) \
         ORDER BY s.id",
    )
    .bind(since)
    .fetch_all(db)
    .await
}

/// How long measurements stay, as the oldest times kept (Unix seconds).
#[derive(Debug, Clone, Copy)]
pub struct Keep {
    /// Every station's measurements from here on
    pub all: i64,
    /// Stations kept whole (`station::KEPT_IDS`), opened since this time
    pub opened_since: i64,
    /// Kept stations' measurements from here on
    pub kept: i64,
    /// The report as sent, from here on
    pub raw: i64,
}

/// Thins out the measurements: kept stations (tracked, opened lately, someone's favourite) keep
/// theirs to `keep.kept`, all others only to `keep.all`; the reports' text goes after
/// `keep.raw`. Worldwide that is thousands of stations every half hour, too much to keep a year
/// of each. Returns the measurements dropped.
pub async fn purge(db: &SqlitePool, keep: Keep) -> sqlx::Result<u64> {
    let dropped = sqlx::query(&format!(
        "DELETE FROM observation WHERE observed_at < ? \
         OR (observed_at < ? AND station_id NOT IN ({}))",
        crate::station::KEPT_IDS
    ))
    .bind(keep.kept)
    .bind(keep.all)
    .bind(keep.opened_since)
    .execute(db)
    .await?
    .rows_affected();
    sqlx::query("UPDATE observation SET raw = NULL WHERE observed_at < ? AND raw IS NOT NULL")
        .bind(keep.raw)
        .execute(db)
        .await?;
    Ok(dropped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::station::{NewStation, Station};

    #[tokio::test]
    async fn stores_and_reads() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db = crate::db::connect(&dir.path().join("t.db")).await?;
        crate::db::migrate(&db).await?;
        crate::station::upsert(
            &db,
            &[NewStation {
                station: Station {
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
                    has_taf: true,
                },
                keywords: None,
            }],
            1,
        )
        .await?;
        let id = station_ids(&db, StationSource::Metar).await?["EDSB"];
        let o = |at: i64, wind: f32| Observation {
            at,
            wind: Some(wind),
            gust: None,
            dir: Some(210.0),
            temp: Some(13.0),
            raw: Some(format!("METAR EDSB {wind}")),
        };
        store(&db, &[(id, o(100, 9.0)), (id, o(200, 10.0))]).await?;
        // A correction of the same time replaces it.
        store(&db, &[(id, o(200, 11.0))]).await?;
        let got = since(&db, id, 150).await?;
        assert_eq!(got, [o(200, 11.0)]);
        assert_eq!(nearest(&got, 2_000, MATCH_S), None);
        assert_eq!(between(&db, id, 100, 200).await?, [o(100, 9.0)]);
        assert_eq!(oldest(&db, id).await?, Some(100));
        assert_eq!(nearest(&got, 210, MATCH_S), Some(&o(200, 11.0)));
        let pins = newest_per_station(&db, 150).await?;
        assert_eq!(
            (pins[0].slug.as_str(), pins[0].at, pins[0].wind, pins[0].dir),
            ("edsb", Some(200), Some(11.0), Some(210.0))
        );
        assert_eq!(newest_per_station(&db, 201).await?[0].at, None);
        let keep = |all, kept| Keep {
            all,
            opened_since: 0,
            kept,
            raw: 0,
        };
        // Nobody tracks, opened or saved the station: it keeps only the short span.
        assert_eq!(purge(&db, keep(150, 0)).await?, 1);
        assert_eq!(since(&db, id, 0).await?.len(), 1);
        // Tracked, it keeps its measurements to the long span; the report text goes earlier.
        store(&db, &[(id, o(100, 9.0))]).await?;
        sqlx::query("UPDATE station SET tracked = 1")
            .execute(&db)
            .await?;
        let keep_raw = Keep {
            raw: 150,
            ..keep(250, 50)
        };
        assert_eq!(purge(&db, keep_raw).await?, 0);
        let got = since(&db, id, 0).await?;
        assert_eq!((got[0].raw.as_deref(), got[1].raw.is_some()), (None, true));
        assert_eq!(purge(&db, keep(250, 150)).await?, 1);
        Ok(())
    }
}
