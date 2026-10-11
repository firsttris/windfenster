//! Places, model runs and the stored forecast series, plus the forecast the API sends.

use sqlx::SqlitePool;

use crate::geo::GridPoint;
use crate::series::{self, Quantity};

mod api;
pub use api::{Forecast, ModelSeries, Notice};

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

    /// The index of the value at `t`, when a step falls on it.
    pub fn index(&self, t: i64) -> Option<usize> {
        let d = t - self.start;
        (d >= 0 && self.step_s > 0 && d % self.step_s == 0)
            .then(|| (d / self.step_s) as usize)
            .filter(|i| *i < self.len())
    }
}

/// A model run, e.g. ICON-D2 12 UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct Run {
    pub id: i64,
    pub run_at: i64,
    pub complete: bool,
    /// Taken from the schedule, not from Open-Meteo's run metadata.
    pub estimated: bool,
}

const RUN_COLUMNS: &str = "id, run_at, status = 'complete' AS complete, estimated";

/// The newest run Windfenster knows of for a model.
pub async fn latest_run(db: &SqlitePool, model_id: &str) -> sqlx::Result<Option<Run>> {
    sqlx::query_as(&format!(
        "SELECT {RUN_COLUMNS} FROM model_run WHERE model_id = ? ORDER BY run_at DESC LIMIT 1"
    ))
    .bind(model_id)
    .fetch_optional(db)
    .await
}

/// Records a run (once); returns it and whether it was new.
pub async fn add_run(
    db: &SqlitePool,
    model_id: &str,
    run_at: i64,
    estimated: bool,
) -> sqlx::Result<(Run, bool)> {
    let inserted = sqlx::query(
        "INSERT INTO model_run (model_id, run_at, estimated) VALUES (?, ?, ?) \
         ON CONFLICT (model_id, run_at) DO NOTHING",
    )
    .bind(model_id)
    .bind(run_at)
    .bind(estimated)
    .execute(db)
    .await?
    .rows_affected()
        > 0;
    let run = sqlx::query_as(&format!(
        "SELECT {RUN_COLUMNS} FROM model_run WHERE model_id = ? AND run_at = ?"
    ))
    .bind(model_id)
    .bind(run_at)
    .fetch_one(db)
    .await?;
    Ok((run, inserted))
}

/// Gives an estimated run the initialisation time Open-Meteo reports. If that run is already
/// recorded, the estimated one is dropped with its series. Returns the run that stays.
pub async fn confirm_run(
    db: &SqlitePool,
    model_id: &str,
    estimated: Run,
    run_at: i64,
) -> sqlx::Result<Run> {
    let mut tx = db.begin().await?;
    let existing: Option<Run> = sqlx::query_as(&format!(
        "SELECT {RUN_COLUMNS} FROM model_run WHERE model_id = ? AND run_at = ? AND id <> ?"
    ))
    .bind(model_id)
    .bind(run_at)
    .bind(estimated.id)
    .fetch_optional(&mut *tx)
    .await?;
    let run = match existing {
        Some(real) => {
            sqlx::query("DELETE FROM model_run WHERE id = ?")
                .bind(estimated.id)
                .execute(&mut *tx)
                .await?;
            real
        }
        None => {
            sqlx::query("UPDATE model_run SET run_at = ?, estimated = 0 WHERE id = ?")
                .bind(run_at)
                .bind(estimated.id)
                .execute(&mut *tx)
                .await?;
            Run {
                run_at,
                estimated: false,
                ..estimated
            }
        }
    };
    tx.commit().await?;
    Ok(run)
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

/// Records what Open-Meteo says about a place. A place fetched without a request (a tracked
/// station) is created pinned and never requested.
pub async fn set_place_info(
    db: &SqlitePool,
    p: GridPoint,
    timezone: Option<&str>,
    elevation_m: Option<f64>,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO place (lat_e2, lon_e2, timezone, elevation_m, requested_at, pinned) \
         VALUES (?, ?, ?, ?, 0, 1) ON CONFLICT (lat_e2, lon_e2) DO UPDATE SET \
         timezone = COALESCE(excluded.timezone, timezone), \
         elevation_m = COALESCE(excluded.elevation_m, elevation_m)",
    )
    .bind(p.lat_e2)
    .bind(p.lon_e2)
    .bind(timezone)
    .bind(elevation_m)
    .execute(db)
    .await?;
    Ok(())
}

/// Places to keep current: the grid points of tracked stations, then the places requested
/// since `since`, most recently requested first.
pub async fn hot_places(db: &SqlitePool, since: i64) -> sqlx::Result<Vec<GridPoint>> {
    let rows: Vec<(i32, i32)> = sqlx::query_as(
        "SELECT lat_e2, lon_e2 FROM ( \
           SELECT CAST(round(lat * 100) AS INTEGER) AS lat_e2, \
                  CAST(round(lon * 100) AS INTEGER) AS lon_e2, 1 AS tracked, 0 AS at \
           FROM station WHERE tracked \
           UNION ALL \
           SELECT lat_e2, lon_e2, 0, requested_at FROM place WHERE requested_at >= ? \
         ) GROUP BY lat_e2, lon_e2 ORDER BY MAX(tracked) DESC, MAX(at) DESC",
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
    fn decode(self) -> anyhow::Result<StoredSeries> {
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

pub async fn latest_series(db: &SqlitePool, p: GridPoint) -> anyhow::Result<Vec<StoredSeries>> {
    let rows: Vec<SeriesRow> = sqlx::query_as(&format!(
        "SELECT {SERIES_COLUMNS} FROM forecast_series s JOIN model_run r ON r.id = s.run_id \
         WHERE s.lat_e2 = ?1 AND s.lon_e2 = ?2 AND r.run_at = ( \
           SELECT MAX(r2.run_at) FROM forecast_series s2 JOIN model_run r2 ON r2.id = s2.run_id \
           WHERE s2.lat_e2 = ?1 AND s2.lon_e2 = ?2 AND s2.model_id = s.model_id)"
    ))
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
) -> anyhow::Result<Vec<StoredSeries>> {
    let rows: Vec<SeriesRow> = sqlx::query_as(&format!(
        "SELECT {SERIES_COLUMNS} FROM forecast_series s JOIN model_run r ON r.id = s.run_id \
         WHERE s.lat_e2 = ? AND s.lon_e2 = ? AND r.run_at >= ? AND r.run_at < ? \
         AND length(s.wind) > 0 ORDER BY r.run_at"
    ))
    .bind(p.lat_e2)
    .bind(p.lon_e2)
    .bind(from)
    .bind(to)
    .fetch_all(db)
    .await?;
    rows.into_iter().map(SeriesRow::decode).collect()
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

        let (r1, new) = add_run(&db, "icon_d2", 1_000, false).await?;
        assert!(new && !r1.complete);
        let (again, new) = add_run(&db, "icon_d2", 1_000, false).await?;
        assert!(!new);
        assert_eq!(again.id, r1.id);
        let (r2, _) = add_run(&db, "icon_d2", 2_000, false).await?;
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
        let (g, _) = add_run(&db, "gfs_global", 1_500, false).await?;
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

    #[tokio::test]
    async fn estimated_runs_take_the_real_time() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db = crate::db::connect(&dir.path().join("t.db")).await?;
        crate::db::migrate(&db).await?;
        // Guessed 06 UTC, Open-Meteo says 03 UTC: the run moves to 03 UTC.
        let (guess, _) = add_run(&db, "icon_d2", 21_600, true).await?;
        assert!(guess.estimated);
        let real = confirm_run(&db, "icon_d2", guess, 10_800).await?;
        assert_eq!(
            (real.id, real.run_at, real.estimated),
            (guess.id, 10_800, false)
        );
        assert_eq!(latest_run(&db, "icon_d2").await?, Some(real));

        // Guessed again, but the real run is already recorded: the guess goes.
        let (guess, _) = add_run(&db, "icon_d2", 21_600, true).await?;
        let kept = confirm_run(&db, "icon_d2", guess, 10_800).await?;
        assert_eq!(kept.id, real.id);
        assert_eq!(latest_run(&db, "icon_d2").await?, Some(real));
        Ok(())
    }
}
