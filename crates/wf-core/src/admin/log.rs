//! What admins changed, newest first on the admin page. Actions are codes (`role.grant`,
//! `user.disable`, `setting.set` ...): the app words them.

use serde::Serialize;
use sqlx::SqlitePool;
use ts_rs::TS;

/// Entries kept: a year
const KEEP_S: i64 = 365 * 86_400;

#[derive(Debug, Clone, PartialEq, Serialize, TS, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AdminLogEntry {
    #[ts(type = "number")]
    pub at: i64,
    /// Who did it; null for the command line or a deleted account
    pub who: Option<String>,
    pub action: String,
    /// What it was done to: an account's name, a setting's key ...
    pub target: Option<String>,
}

pub async fn add(
    db: &SqlitePool,
    now: i64,
    account_id: Option<i64>,
    action: &str,
    target: Option<&str>,
) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO admin_log (at, account_id, action, target) VALUES (?, ?, ?, ?)")
        .bind(now)
        .bind(account_id)
        .bind(action)
        .bind(target)
        .execute(db)
        .await?;
    sqlx::query("DELETE FROM admin_log WHERE at < ?")
        .bind(now - KEEP_S)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn recent(db: &SqlitePool, limit: i64) -> sqlx::Result<Vec<AdminLogEntry>> {
    sqlx::query_as(
        "SELECT l.at, a.name AS who, l.action, l.target FROM admin_log l \
         LEFT JOIN account a ON a.id = l.account_id ORDER BY l.at DESC, l.id DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(db)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn keeps_a_year_newest_first() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db = crate::db::connect(&dir.path().join("t.db")).await?;
        crate::db::migrate(&db).await?;
        let a = crate::account::create(&db, "h", "Ann", "k", "{}", 0).await?;
        add(&db, 10, None, "role.grant", Some("Ann")).await?;
        add(&db, 20, Some(a), "user.disable", Some("Ben")).await?;
        let r = recent(&db, 10).await?;
        assert_eq!(r[0].who.as_deref(), Some("Ann"));
        assert_eq!(r[1].action, "role.grant");
        add(&db, KEEP_S + 15, Some(a), "x", None).await?;
        assert_eq!(recent(&db, 10).await?.len(), 2);
        Ok(())
    }
}
