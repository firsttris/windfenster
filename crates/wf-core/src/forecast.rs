//! Places, model runs and the stored forecast series, plus the forecast the API sends.

use serde::Serialize;
use sqlx::SqlitePool;
use ts_rs::TS;

use crate::geo::GridPoint;
use crate::model::Model;
use crate::series::{self, Quantity};

/// One model's series for a place, decoded. Index i is the value at `start + i * step`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SeriesData {
    pub start: i64,
    pub step_s: i64,
    pub wind: Vec<Option<f32>>,
    pub gust: Vec<Option<f32>>,
    pub dir: Vec<Option<f32>>,
    pub temp: Vec<Option<f32>>,
    pub rain: Vec<Option<f32>>,
    pub cloud: Vec<Option<f32>>,
}

impl SeriesData {
    /// A model that delivered no values at the place (outside its domain): stored as an empty
    /// series, so the place is not asked again for this model.
    pub fn is_empty(&self) -> bool {
        self.wind.is_empty()
    }

    pub fn len(&self) -> usize {
        self.wind.len()
    }
}

/// A model run, e.g. ICON-D2 12 UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct Run {
    pub id: i64,
    pub run_at: i64,
    pub complete: bool,
}

/// The newest run Windfenster knows of for a model.
pub async fn latest_run(db: &SqlitePool, model_id: &str) -> sqlx::Result<Option<Run>> {
    sqlx::query_as(
        "SELECT id, run_at, status = 'complete' AS complete FROM model_run WHERE model_id = ? \
         ORDER BY run_at DESC LIMIT 1",
    )
    .bind(model_id)
    .fetch_optional(db)
    .await
}

/// Records a run (once); returns it and whether it was new.
pub async fn add_run(db: &SqlitePool, model_id: &str, run_at: i64) -> sqlx::Result<(Run, bool)> {
    let inserted = sqlx::query(
        "INSERT INTO model_run (model_id, run_at) VALUES (?, ?) ON CONFLICT (model_id, run_at) DO NOTHING",
    )
    .bind(model_id)
    .bind(run_at)
    .execute(db)
    .await?
    .rows_affected()
        > 0;
    let run = sqlx::query_as(
        "SELECT id, run_at, status = 'complete' AS complete FROM model_run \
         WHERE model_id = ? AND run_at = ?",
    )
    .bind(model_id)
    .bind(run_at)
    .fetch_one(db)
    .await?;
    Ok((run, inserted))
}

pub async fn complete_run(db: &SqlitePool, run_id: i64, now: i64) -> sqlx::Result<()> {
    sqlx::query("UPDATE model_run SET status = 'complete', ingested_at = ? WHERE id = ?")
        .bind(now)
        .bind(run_id)
        .execute(db)
        .await?;
    Ok(())
}

/// Notes that someone asked for a place; `pinned` marks the grid point of a station.
pub async fn touch_place(
    db: &SqlitePool,
    p: GridPoint,
    pinned: bool,
    now: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO place (lat_e2, lon_e2, requested_at, pinned) VALUES (?, ?, ?, ?) \
         ON CONFLICT (lat_e2, lon_e2) DO UPDATE SET requested_at = excluded.requested_at, \
         pinned = MAX(pinned, excluded.pinned)",
    )
    .bind(p.lat_e2)
    .bind(p.lon_e2)
    .bind(now)
    .bind(pinned)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn set_place_info(
    db: &SqlitePool,
    p: GridPoint,
    timezone: Option<&str>,
    elevation_m: Option<f64>,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE place SET timezone = COALESCE(?, timezone), elevation_m = COALESCE(?, elevation_m) \
         WHERE lat_e2 = ? AND lon_e2 = ?",
    )
    .bind(timezone)
    .bind(elevation_m)
    .bind(p.lat_e2)
    .bind(p.lon_e2)
    .execute(db)
    .await?;
    Ok(())
}

/// Places requested since `since`, most recently requested first.
pub async fn hot_places(db: &SqlitePool, since: i64) -> sqlx::Result<Vec<GridPoint>> {
    let rows: Vec<(i32, i32)> = sqlx::query_as(
        "SELECT lat_e2, lon_e2 FROM place WHERE requested_at >= ? ORDER BY requested_at DESC",
    )
    .bind(since)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(lat_e2, lon_e2)| GridPoint { lat_e2, lon_e2 })
        .collect())
}

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

pub async fn latest_series(db: &SqlitePool, p: GridPoint) -> anyhow::Result<Vec<StoredSeries>> {
    let rows: Vec<SeriesRow> = sqlx::query_as(
        "SELECT s.model_id, s.run_id, r.run_at, s.fetched_at, s.start_at, s.step_s, s.wind, s.gust, \
         s.dir, s.temp, s.rain, s.cloud \
         FROM forecast_series s JOIN model_run r ON r.id = s.run_id \
         WHERE s.lat_e2 = ?1 AND s.lon_e2 = ?2 AND r.run_at = ( \
           SELECT MAX(r2.run_at) FROM forecast_series s2 JOIN model_run r2 ON r2.id = s2.run_id \
           WHERE s2.lat_e2 = ?1 AND s2.lon_e2 = ?2 AND s2.model_id = s.model_id)",
    )
    .bind(p.lat_e2)
    .bind(p.lon_e2)
    .fetch_all(db)
    .await?;
    rows.into_iter()
        .map(|r| {
            Ok(StoredSeries {
                model_id: r.model_id,
                run_id: r.run_id,
                run_at: r.run_at,
                fetched_at: r.fetched_at,
                data: SeriesData {
                    start: r.start_at,
                    step_s: r.step_s,
                    wind: series::decode(Quantity::Wind, &r.wind)?,
                    gust: series::decode(Quantity::Gust, &r.gust)?,
                    dir: series::decode(Quantity::Direction, &r.dir)?,
                    temp: series::decode(Quantity::Temperature, &r.temp)?,
                    rain: series::decode(Quantity::Rain, &r.rain)?,
                    cloud: series::decode(Quantity::Cloud, &r.cloud)?,
                },
            })
        })
        .collect()
}

/// Drops runs older than `runs_before` (with their series) and free points nobody asked for
/// since `points_before`. Returns (runs, places) removed.
pub async fn purge(
    db: &SqlitePool,
    runs_before: i64,
    points_before: i64,
) -> sqlx::Result<(u64, u64)> {
    let runs = sqlx::query("DELETE FROM model_run WHERE run_at < ?")
        .bind(runs_before)
        .execute(db)
        .await?
        .rows_affected();
    let mut tx = db.begin().await?;
    sqlx::query(
        "DELETE FROM forecast_series WHERE (lat_e2, lon_e2) IN \
         (SELECT lat_e2, lon_e2 FROM place WHERE NOT pinned AND requested_at < ?)",
    )
    .bind(points_before)
    .execute(&mut *tx)
    .await?;
    let places = sqlx::query("DELETE FROM place WHERE NOT pinned AND requested_at < ?")
        .bind(points_before)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    tx.commit().await?;
    Ok((runs, places))
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

/// What a place knows about itself.
#[derive(Debug, Clone, PartialEq, Default, sqlx::FromRow)]
pub struct PlaceInfo {
    pub timezone: Option<String>,
    pub elevation_m: Option<f64>,
}

pub async fn place_info(db: &SqlitePool, p: GridPoint) -> sqlx::Result<PlaceInfo> {
    Ok(
        sqlx::query_as("SELECT timezone, elevation_m FROM place WHERE lat_e2 = ? AND lon_e2 = ?")
            .bind(p.lat_e2)
            .bind(p.lon_e2)
            .fetch_optional(db)
            .await?
            .unwrap_or_default(),
    )
}

/// One model's forecast as the API sends it. Value i belongs to `start + i * step` (unix s).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ModelSeries {
    pub model: String,
    /// Initialisation time of the run.
    #[ts(type = "number")]
    pub run_at: i64,
    /// When Windfenster fetched it.
    #[ts(type = "number")]
    pub fetched_at: i64,
    #[ts(type = "number")]
    pub start: i64,
    /// Seconds between values.
    #[ts(type = "number")]
    pub step: i64,
    /// Mean wind at 10 m, kn
    pub wind: Vec<Option<f32>>,
    /// Gusts, kn
    pub gust: Vec<Option<f32>>,
    /// Direction the wind comes from, degrees
    pub dir: Vec<Option<f32>>,
    /// Temperature at 2 m, °C
    pub temp: Vec<Option<f32>>,
    /// Precipitation in the hour before, mm
    pub rain: Vec<Option<f32>>,
    /// Cloud cover, %
    pub cloud: Vec<Option<f32>>,
}

impl From<StoredSeries> for ModelSeries {
    fn from(s: StoredSeries) -> Self {
        Self {
            model: s.model_id,
            run_at: s.run_at,
            fetched_at: s.fetched_at,
            start: s.data.start,
            step: s.data.step_s,
            wind: s.data.wind,
            gust: s.data.gust,
            dir: s.data.dir,
            temp: s.data.temp,
            rain: s.data.rain,
            cloud: s.data.cloud,
        }
    }
}

/// Everything the forecast table of a place needs.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Forecast {
    /// The grid point the series belong to.
    pub lat: f64,
    pub lon: f64,
    /// IANA time zone of the place, e.g. Europe/Berlin.
    pub timezone: Option<String>,
    pub elevation_m: Option<f64>,
    /// Models whose domain covers the place, in their default order.
    pub models: Vec<Model>,
    /// The newest series per model; models without values here are left out.
    pub series: Vec<ModelSeries>,
    /// Set when the newest data could not be fetched; the series are then older or missing.
    pub notice: Option<Notice>,
}

/// Why a forecast is not current. The SPA shows a text per code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Notice {
    /// The weather source did not answer or failed.
    SourceUnavailable,
    /// The day's request budget for the weather source is used up.
    BudgetExhausted,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn runs_series_places_and_purge() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db = crate::db::connect(&dir.path().join("t.db")).await?;
        crate::db::migrate(&db).await?;
        let p = GridPoint::new(48.78, 8.08).unwrap();
        let free = GridPoint::new(47.0, 9.0).unwrap();

        let (r1, new) = add_run(&db, "icon_d2", 1_000).await?;
        assert!(new && !r1.complete);
        let (again, new) = add_run(&db, "icon_d2", 1_000).await?;
        assert!(!new);
        assert_eq!(again.id, r1.id);
        let (r2, _) = add_run(&db, "icon_d2", 2_000).await?;
        complete_run(&db, r2.id, 2_100).await?;
        let latest = latest_run(&db, "icon_d2").await?.unwrap();
        assert_eq!((latest.id, latest.complete), (r2.id, true));

        let data = |kn: f32| SeriesData {
            start: 3_600,
            step_s: 3_600,
            wind: vec![Some(kn), None],
            gust: vec![Some(kn + 5.0), None],
            dir: vec![Some(225.0), None],
            temp: vec![Some(17.5), None],
            rain: vec![Some(0.0), None],
            cloud: vec![Some(40.0), None],
        };
        touch_place(&db, p, true, 5_000).await?;
        touch_place(&db, free, false, 5_000).await?;
        set_place_info(&db, p, Some("Europe/Berlin"), Some(121.0)).await?;
        store_series(&db, p, "icon_d2", r1.id, &data(10.0), 1_100).await?;
        store_series(&db, p, "icon_d2", r2.id, &data(14.0), 2_100).await?;
        store_series(&db, free, "icon_d2", r2.id, &data(8.0), 2_100).await?;
        // A model with nothing at the place is stored empty.
        let (g, _) = add_run(&db, "gfs_global", 1_500).await?;
        store_series(&db, p, "gfs_global", g.id, &SeriesData::default(), 1_600).await?;

        let mut latest = latest_series(&db, p).await?;
        latest.sort_by(|a, b| a.model_id.cmp(&b.model_id));
        assert_eq!(latest.len(), 2);
        assert!(latest[0].data.is_empty());
        assert_eq!(latest[1].run_id, r2.id);
        assert_eq!(latest[1].data, data(14.0));
        assert_eq!(
            place_info(&db, p).await?.timezone.as_deref(),
            Some("Europe/Berlin")
        );
        assert_eq!(hot_places(&db, 4_000).await?.len(), 2);
        let mut state = series_state(&db, p).await?;
        state.sort();
        assert_eq!(
            state,
            [
                ("gfs_global".to_owned(), 1_500, true),
                ("icon_d2".to_owned(), 2_000, false)
            ]
        );

        // Run 1 is older than the cutoff; the free point was last asked for before its cutoff.
        let (runs, places) = purge(&db, 1_200, 6_000).await?;
        assert_eq!((runs, places), (1, 1));
        assert!(latest_series(&db, free).await?.is_empty());
        assert_eq!(latest_series(&db, p).await?.len(), 2);
        Ok(())
    }
}
