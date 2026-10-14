//! Counters per UTC day kept in the database (`usage_daily`), so a restart does not reset them:
//! Open-Meteo calls against the daily budget, e-mails sent.

use sqlx::SqlitePool;

/// Open-Meteo calls for keeping places current after a new model run
pub const OPEN_METEO_REFRESH: &str = "open_meteo_refresh";
/// Open-Meteo calls for places someone opened that the server did not have yet
pub const OPEN_METEO_OPENED: &str = "open_meteo_opened";
/// E-mails sent by the server: confirmations, alarms, tests
pub const MAIL: &str = "mail";

/// The UTC day of a Unix time: days since 1970-01-01.
pub fn day(t: i64) -> i64 {
    t.div_euclid(86_400)
}

/// Adds `n` to a counter of a day.
pub async fn add(db: &SqlitePool, day: i64, counter: &str, n: i64) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO usage_daily (day, counter, n) VALUES (?, ?, ?)
         ON CONFLICT (day, counter) DO UPDATE SET n = n + excluded.n",
    )
    .bind(day)
    .bind(counter)
    .bind(n)
    .execute(db)
    .await?;
    Ok(())
}

/// A counter's value on a day, 0 when nothing was counted.
pub async fn get(db: &SqlitePool, day: i64, counter: &str) -> sqlx::Result<i64> {
    let n: Option<i64> =
        sqlx::query_scalar("SELECT n FROM usage_daily WHERE day = ? AND counter = ?")
            .bind(day)
            .bind(counter)
            .fetch_optional(db)
            .await?;
    Ok(n.unwrap_or(0))
}

/// A counter per day from `from` on, oldest first; days without a count are left out.
pub async fn history(db: &SqlitePool, from: i64, counter: &str) -> sqlx::Result<Vec<(i64, i64)>> {
    sqlx::query_as("SELECT day, n FROM usage_daily WHERE day >= ? AND counter = ? ORDER BY day")
        .bind(from)
        .bind(counter)
        .fetch_all(db)
        .await
}

/// Drops counts older than `before` (a day number).
pub async fn purge(db: &SqlitePool, before: i64) -> sqlx::Result<u64> {
    Ok(sqlx::query("DELETE FROM usage_daily WHERE day < ?")
        .bind(before)
        .execute(db)
        .await?
        .rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[tokio::test]
    async fn counts_per_day() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db = db::connect(&dir.path().join("t.db")).await?;
        db::migrate(&db).await?;
        add(&db, 20_000, MAIL, 2).await?;
        add(&db, 20_000, MAIL, 3).await?;
        add(&db, 20_001, MAIL, 1).await?;
        add(&db, 20_001, OPEN_METEO_REFRESH, 7).await?;
        assert_eq!(get(&db, 20_000, MAIL).await?, 5);
        assert_eq!(get(&db, 20_002, MAIL).await?, 0);
        assert_eq!(
            history(&db, 20_000, MAIL).await?,
            vec![(20_000, 5), (20_001, 1)]
        );
        assert_eq!(purge(&db, 20_001).await?, 1);
        assert_eq!(history(&db, 0, MAIL).await?, vec![(20_001, 1)]);
        assert_eq!(day(86_400 * 3 + 5), 3);
        assert_eq!(day(-1), -1);
        Ok(())
    }
}
