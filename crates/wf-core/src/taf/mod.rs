//! Airport forecasts (TAF): the change groups as aviationweather.gov decodes them, and the
//! hourly series they add up to.

mod resolve;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use ts_rs::TS;

/// How a group relates to what comes before it (ICAO Annex 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum TafChange {
    /// The conditions at the start of the validity
    Base,
    /// FM: from this time on, everything as given
    From,
    /// BECMG: changing between `from` and `by`, then lasting
    Becoming,
    /// TEMPO: at times, each time for less than an hour
    Tempo,
    /// PROB30/40 without TEMPO: possible for the whole period
    Prob,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TafCloud {
    /// FEW, SCT, BKN, OVC, NSC …
    pub cover: String,
    pub base_ft: Option<f32>,
    /// CB or TCU
    pub kind: Option<String>,
}

/// One group of a TAF. Wind and gusts in knots; a group without wind leaves it unchanged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TafGroup {
    pub change: TafChange,
    /// PROB30 / PROB40
    pub probability: Option<u8>,
    #[ts(type = "number")]
    pub from: i64,
    #[ts(type = "number")]
    pub to: i64,
    /// BECMG: the change is complete by then.
    #[ts(type = "number | null")]
    pub by: Option<i64>,
    /// Where the wind comes from; null when variable or not given
    pub dir: Option<f32>,
    pub variable: bool,
    pub wind: Option<f32>,
    pub gust: Option<f32>,
    /// Metres; 9999 for 10 km or more
    pub visibility_m: Option<f32>,
    /// Weather as coded, e.g. "-SHRA", "BCFG", "NSW"
    pub weather: Option<String>,
    pub clouds: Vec<TafCloud>,
}

/// The forecast for one hour. `low`/`high` span what TEMPO, PROB and a BECMG in progress allow
/// beside the prevailing wind; they are set only when that span is wider than `wind`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TafHour {
    #[ts(type = "number")]
    pub at: i64,
    pub wind: Option<f32>,
    pub gust: Option<f32>,
    pub dir: Option<f32>,
    pub low: Option<f32>,
    pub high: Option<f32>,
    /// Highest gust of the hour, TEMPO and PROB included
    pub gust_high: Option<f32>,
    /// Set when the span comes only from PROB groups
    pub probability: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Taf {
    #[ts(type = "number")]
    pub issued_at: i64,
    #[ts(type = "number")]
    pub valid_from: i64,
    #[ts(type = "number")]
    pub valid_to: i64,
    pub raw: String,
    pub groups: Vec<TafGroup>,
    pub hours: Vec<TafHour>,
    #[ts(type = "number")]
    pub fetched_at: i64,
}

impl Taf {
    pub fn new(
        issued_at: i64,
        valid_from: i64,
        valid_to: i64,
        raw: String,
        groups: Vec<TafGroup>,
        fetched_at: i64,
    ) -> Self {
        let hours = resolve(&groups, valid_from, valid_to);
        Self {
            issued_at,
            valid_from,
            valid_to,
            raw,
            groups,
            hours,
            fetched_at,
        }
    }
}

pub use resolve::resolve;

#[derive(sqlx::FromRow)]
struct Row {
    issued_at: i64,
    valid_from: i64,
    valid_to: i64,
    raw: String,
    groups: String,
    fetched_at: i64,
}

impl Row {
    fn into_taf(self) -> Option<Taf> {
        let groups: Vec<TafGroup> = serde_json::from_str(&self.groups).ok()?;
        Some(Taf::new(
            self.issued_at,
            self.valid_from,
            self.valid_to,
            self.raw,
            groups,
            self.fetched_at,
        ))
    }
}

/// Stores TAFs; the same station and issue time again replaces the row (fetched again).
pub async fn store(db: &SqlitePool, tafs: &[(i64, Taf)]) -> sqlx::Result<()> {
    let mut tx = db.begin().await?;
    for (station_id, t) in tafs {
        let groups = serde_json::to_string(&t.groups).unwrap_or_else(|_| "[]".into());
        sqlx::query(
            "INSERT OR REPLACE INTO taf (station_id, issued_at, valid_from, valid_to, raw, groups, \
             fetched_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(station_id)
        .bind(t.issued_at)
        .bind(t.valid_from)
        .bind(t.valid_to)
        .bind(&t.raw)
        .bind(groups)
        .bind(t.fetched_at)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

/// The newest TAF of a station that is still valid at `now`.
pub async fn current(db: &SqlitePool, station_id: i64, now: i64) -> sqlx::Result<Option<Taf>> {
    let row: Option<Row> = sqlx::query_as(
        "SELECT issued_at, valid_from, valid_to, raw, groups, fetched_at FROM taf \
         WHERE station_id = ? AND valid_to > ? ORDER BY issued_at DESC LIMIT 1",
    )
    .bind(station_id)
    .bind(now)
    .fetch_optional(db)
    .await?;
    Ok(row.and_then(Row::into_taf))
}

/// Every TAF of a station valid at some time in [from, to), oldest first.
pub async fn between(
    db: &SqlitePool,
    station_id: i64,
    from: i64,
    to: i64,
) -> sqlx::Result<Vec<Taf>> {
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT issued_at, valid_from, valid_to, raw, groups, fetched_at FROM taf \
         WHERE station_id = ? AND valid_to > ? AND valid_from < ? ORDER BY issued_at",
    )
    .bind(station_id)
    .bind(from)
    .bind(to)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().filter_map(Row::into_taf).collect())
}

/// When the station's TAF was last fetched, if ever.
pub async fn last_fetch(db: &SqlitePool, station_id: i64) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar("SELECT MAX(fetched_at) FROM taf WHERE station_id = ?")
        .bind(station_id)
        .fetch_one(db)
        .await
}

/// Drops TAFs whose validity ended before `before`.
pub async fn purge(db: &SqlitePool, before: i64) -> sqlx::Result<u64> {
    Ok(sqlx::query("DELETE FROM taf WHERE valid_to < ?")
        .bind(before)
        .execute(db)
        .await?
        .rows_affected())
}

/// Groups and times shared by the tests.
#[cfg(test)]
pub(crate) mod fixtures {
    use super::{TafChange, TafGroup};

    pub const H: i64 = 3_600;
    /// 2026-10-11 06:00 UTC
    pub const T0: i64 = 1_791_698_400;

    pub fn group(change: TafChange, from: i64, to: i64, wind: Option<(f32, f32)>) -> TafGroup {
        TafGroup {
            change,
            probability: None,
            from,
            to,
            by: None,
            dir: wind.map(|w| w.0),
            variable: false,
            wind: wind.map(|w| w.1),
            gust: None,
            visibility_m: None,
            weather: None,
            clouds: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    #[tokio::test]
    async fn stores_the_current_one() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db = crate::db::connect(&dir.path().join("t.db")).await?;
        crate::db::migrate(&db).await?;
        sqlx::query(
            "INSERT INTO station (slug, source, code, name, lat, lon, search, seen_at) \
             VALUES ('edsb', 'metar', 'EDSB', 'Baden', 48.78, 8.08, 'baden', 1)",
        )
        .execute(&db)
        .await?;
        let groups = vec![group(TafChange::Base, T0, T0 + 24 * H, Some((220.0, 7.0)))];
        let taf = |issued: i64, fetched: i64| {
            Taf::new(
                issued,
                T0,
                T0 + 24 * H,
                format!("TAF EDSB {issued}"),
                groups.clone(),
                fetched,
            )
        };
        store(
            &db,
            &[(1, taf(T0 - H, T0)), (1, taf(T0 + 2 * H, T0 + 2 * H))],
        )
        .await?;
        let got = current(&db, 1, T0 + 3 * H).await?.unwrap();
        assert_eq!(got.issued_at, T0 + 2 * H);
        assert_eq!(got.hours.len(), 24);
        assert_eq!(last_fetch(&db, 1).await?, Some(T0 + 2 * H));
        assert_eq!(between(&db, 1, T0 + 23 * H, T0 + 30 * H).await?.len(), 2);
        assert!(between(&db, 1, T0 + 24 * H, T0 + 30 * H).await?.is_empty());
        assert!(current(&db, 1, T0 + 24 * H).await?.is_none());
        assert_eq!(purge(&db, T0 + 25 * H).await?, 2);
        Ok(())
    }
}
