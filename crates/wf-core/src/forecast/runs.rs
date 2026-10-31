//! Model runs: when each one started, whether it is fetched, whether its time is estimated.

use sqlx::SqlitePool;

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
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {RUN_COLUMNS} FROM model_run WHERE model_id = ? ORDER BY run_at DESC LIMIT 1"
    )))
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
    let run = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {RUN_COLUMNS} FROM model_run WHERE model_id = ? AND run_at = ?"
    )))
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
    let existing: Option<Run> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {RUN_COLUMNS} FROM model_run WHERE model_id = ? AND run_at = ? AND id <> ?"
    )))
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
