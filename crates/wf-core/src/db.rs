use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};

pub type Db = SqlitePool;

/// The migrations in `/migrations`, compiled into the binary.
pub static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");

/// Opens (and creates) the database in WAL mode. Does not migrate; see [`migrate`].
pub async fn connect(path: &Path) -> Result<SqlitePool> {
    let opts = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(10));
    SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(opts)
        .await
        .with_context(|| format!("opening {}", path.display()))
}

/// Applies pending migrations; returns the schema version afterwards.
pub async fn migrate(db: &SqlitePool) -> Result<i64> {
    MIGRATOR.run(db).await.context("running migrations")?;
    schema_version(db).await
}

/// Version of the newest applied migration, 0 for an empty database.
pub async fn schema_version(db: &SqlitePool) -> Result<i64> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations')",
    )
    .fetch_one(db)
    .await?;
    if !exists {
        return Ok(0);
    }
    let v: Option<i64> =
        sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations WHERE success")
            .fetch_one(db)
            .await?;
    Ok(v.unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::series::{self, Quantity};

    #[tokio::test]
    async fn migrates_and_stores_a_series() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let db = connect(&dir.path().join("test.db")).await?;
        assert_eq!(schema_version(&db).await?, 0);
        let v = migrate(&db).await?;
        assert!(v >= 1);
        // Running again is a no-op.
        assert_eq!(migrate(&db).await?, v);

        sqlx::query(
            "INSERT INTO model (id, name, provider, resolution_km, class, max_lead_h, runs_per_day) \
             VALUES ('test_global', 'Test', 'Test', 9, 'global', 360, 4)",
        )
        .execute(&db)
        .await?;
        let run_id: i64 = sqlx::query_scalar(
            "INSERT INTO model_run (model_id, run_at) VALUES ('test_global', 1791676800) RETURNING id",
        )
        .fetch_one(&db)
        .await?;
        let wind = series::encode(Quantity::Wind, &[Some(12.0), Some(14.5)]);
        let other = series::encode(Quantity::Cloud, &[None, None]);
        sqlx::query(
            "INSERT INTO forecast_series (lat_e2, lon_e2, model_id, run_id, start_at, step_s, \
             wind, gust, dir, temp, rain, cloud, fetched_at) \
             VALUES (4878, 808, 'test_global', ?, 1791676800, 3600, ?, ?, ?, ?, ?, ?, 1791680000)",
        )
        .bind(run_id)
        .bind(&wind)
        .bind(&other)
        .bind(&other)
        .bind(&other)
        .bind(&other)
        .bind(&other)
        .execute(&db)
        .await?;
        let blob: Vec<u8> = sqlx::query_scalar(
            "SELECT wind FROM forecast_series WHERE lat_e2 = 4878 AND lon_e2 = 808",
        )
        .fetch_one(&db)
        .await?;
        assert_eq!(
            series::decode(Quantity::Wind, &blob)?,
            [Some(12.0), Some(14.5)]
        );

        // A series must belong to a run of its own model.
        sqlx::query(
            "INSERT INTO model (id, name, provider, resolution_km, class, max_lead_h, runs_per_day) \
             VALUES ('other', 'Other', 'Test', 13, 'global', 384, 4)",
        )
        .execute(&db)
        .await?;
        let mismatch = sqlx::query(
            "INSERT INTO forecast_series (lat_e2, lon_e2, model_id, run_id, start_at, step_s, \
             wind, gust, dir, temp, rain, cloud, fetched_at) \
             VALUES (4878, 808, 'other', ?, 0, 3600, x'', x'', x'', x'', x'', x'', 0)",
        )
        .bind(run_id)
        .execute(&db)
        .await;
        assert!(mismatch.is_err());

        // Deleting a run removes its series.
        sqlx::query("DELETE FROM model_run WHERE id = ?")
            .bind(run_id)
            .execute(&db)
            .await?;
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM forecast_series")
            .fetch_one(&db)
            .await?;
        assert_eq!(n, 0);
        Ok(())
    }
}
