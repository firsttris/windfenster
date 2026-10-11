//! Places: grid points people asked for, what Open-Meteo says about them, and cleanup.

use sqlx::SqlitePool;

use crate::geo::GridPoint;

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
