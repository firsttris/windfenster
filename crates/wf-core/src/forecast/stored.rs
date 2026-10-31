//! Forecast series as stored: one row per place, model and run, each quantity a blob.

use sqlx::SqlitePool;

use super::{LoadError, SeriesData};
use crate::geo::GridPoint;
use crate::series::{self, Quantity};

/// Stores one model's series for a place and run (replacing a previous fetch of the same run).
pub async fn store_series(
    db: &SqlitePool,
    p: GridPoint,
    model_id: &str,
    run_id: i64,
    data: &SeriesData,
    fetched_at: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT OR REPLACE INTO forecast_series (lat_e2, lon_e2, model_id, run_id, start_at, step_s, \
         wind, gust, dir, temp, rain, cloud, fetched_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(p.lat_e2)
    .bind(p.lon_e2)
    .bind(model_id)
    .bind(run_id)
    .bind(data.start)
    .bind(data.step_s.max(1))
    .bind(series::encode(Quantity::Wind, &data.wind))
    .bind(series::encode(Quantity::Gust, &data.gust))
    .bind(series::encode(Quantity::Direction, &data.dir))
    .bind(series::encode(Quantity::Temperature, &data.temp))
    .bind(series::encode(Quantity::Rain, &data.rain))
    .bind(series::encode(Quantity::Cloud, &data.cloud))
    .bind(fetched_at)
    .execute(db)
    .await?;
    Ok(())
}

/// The newest stored series of each model at a place.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredSeries {
    pub model_id: String,
    pub run_id: i64,
    pub run_at: i64,
    pub fetched_at: i64,
    pub data: SeriesData,
}

const SERIES_COLUMNS: &str = "s.model_id, s.run_id, r.run_at, s.fetched_at, s.start_at, s.step_s, \
    s.wind, s.gust, s.dir, s.temp, s.rain, s.cloud";

#[derive(sqlx::FromRow)]
struct SeriesRow {
    model_id: String,
    run_id: i64,
    run_at: i64,
    fetched_at: i64,
    start_at: i64,
    step_s: i64,
    wind: Vec<u8>,
    gust: Vec<u8>,
    dir: Vec<u8>,
    temp: Vec<u8>,
    rain: Vec<u8>,
    cloud: Vec<u8>,
}

impl SeriesRow {
    fn decode(self) -> Result<StoredSeries, LoadError> {
        Ok(StoredSeries {
            model_id: self.model_id,
            run_id: self.run_id,
            run_at: self.run_at,
            fetched_at: self.fetched_at,
            data: SeriesData {
                start: self.start_at,
                step_s: self.step_s,
                wind: series::decode(Quantity::Wind, &self.wind)?,
                gust: series::decode(Quantity::Gust, &self.gust)?,
                dir: series::decode(Quantity::Direction, &self.dir)?,
                temp: series::decode(Quantity::Temperature, &self.temp)?,
                rain: series::decode(Quantity::Rain, &self.rain)?,
                cloud: series::decode(Quantity::Cloud, &self.cloud)?,
            },
        })
    }
}

pub async fn latest_series(db: &SqlitePool, p: GridPoint) -> Result<Vec<StoredSeries>, LoadError> {
    let rows: Vec<SeriesRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {SERIES_COLUMNS} FROM forecast_series s JOIN model_run r ON r.id = s.run_id \
         WHERE s.lat_e2 = ?1 AND s.lon_e2 = ?2 AND r.run_at = ( \
           SELECT MAX(r2.run_at) FROM forecast_series s2 JOIN model_run r2 ON r2.id = s2.run_id \
           WHERE s2.lat_e2 = ?1 AND s2.lon_e2 = ?2 AND s2.model_id = s.model_id)"
    )))
    .bind(p.lat_e2)
    .bind(p.lon_e2)
    .fetch_all(db)
    .await?;
    rows.into_iter().map(SeriesRow::decode).collect()
}

/// Every stored series of a place whose run started in [from, to), oldest run first; empty
/// series (models without values here) are left out.
pub async fn series_between(
    db: &SqlitePool,
    p: GridPoint,
    from: i64,
    to: i64,
) -> Result<Vec<StoredSeries>, LoadError> {
    let rows: Vec<SeriesRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {SERIES_COLUMNS} FROM forecast_series s JOIN model_run r ON r.id = s.run_id \
         WHERE s.lat_e2 = ? AND s.lon_e2 = ? AND r.run_at >= ? AND r.run_at < ? \
         AND length(s.wind) > 0 ORDER BY r.run_at"
    )))
    .bind(p.lat_e2)
    .bind(p.lon_e2)
    .bind(from)
    .bind(to)
    .fetch_all(db)
    .await?;
    rows.into_iter().map(SeriesRow::decode).collect()
}

/// Per model, the run of the newest stored series at a place and whether it is empty.
pub async fn series_state(db: &SqlitePool, p: GridPoint) -> sqlx::Result<Vec<(String, i64, bool)>> {
    let rows: Vec<(String, i64, bool)> = sqlx::query_as(
        "SELECT s.model_id, r.run_at, length(s.wind) = 0 FROM forecast_series s \
         JOIN model_run r ON r.id = s.run_id WHERE s.lat_e2 = ? AND s.lon_e2 = ? \
         ORDER BY r.run_at DESC",
    )
    .bind(p.lat_e2)
    .bind(p.lon_e2)
    .fetch_all(db)
    .await?;
    let mut seen = std::collections::HashSet::new();
    Ok(rows
        .into_iter()
        .filter(|(m, _, _)| seen.insert(m.clone()))
        .collect())
}
