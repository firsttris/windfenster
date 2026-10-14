//! Every alarm message handed to a channel, delivered or not, for the admin page; kept a month.

use serde::Serialize;
use sqlx::SqlitePool;
use ts_rs::TS;

/// One message on one channel.
#[derive(Debug, Clone, PartialEq, Serialize, TS, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Delivery {
    #[ts(type = "number")]
    pub sent_at: i64,
    /// The account's name; null when it is deleted
    pub account: Option<String>,
    /// The spot's name; null for a test message or a deleted spot
    pub spot: Option<String>,
    /// "push" or "email"
    pub channel: String,
    /// "window", "live" or "test"
    pub kind: String,
    pub title: String,
    pub ok: bool,
    /// Why it failed, technical
    pub detail: Option<String>,
}

/// What a channel made of one message.
pub struct Record<'a> {
    pub account_id: i64,
    pub favorite_id: Option<i64>,
    pub channel: &'a str,
    pub kind: &'a str,
    pub title: &'a str,
    pub ok: bool,
    pub detail: Option<&'a str>,
}

pub async fn record(db: &SqlitePool, now: i64, r: &Record<'_>) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO alarm_delivery (sent_at, account_id, favorite_id, channel, kind, title, ok, \
         detail) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(now)
    .bind(r.account_id)
    .bind(r.favorite_id)
    .bind(r.channel)
    .bind(r.kind)
    .bind(r.title)
    .bind(r.ok)
    .bind(r.detail)
    .execute(db)
    .await?;
    Ok(())
}

/// The newest deliveries first.
pub async fn recent(db: &SqlitePool, limit: i64) -> sqlx::Result<Vec<Delivery>> {
    sqlx::query_as(
        "SELECT d.sent_at, a.name AS account, f.name AS spot, d.channel, d.kind, d.title, d.ok, \
         d.detail FROM alarm_delivery d LEFT JOIN account a ON a.id = d.account_id \
         LEFT JOIN favorite f ON f.id = d.favorite_id ORDER BY d.sent_at DESC, d.id DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(db)
    .await
}

/// Messages since `since`: (delivered by push, by e-mail, failed).
pub async fn counts(db: &SqlitePool, since: i64) -> sqlx::Result<(i64, i64, i64)> {
    sqlx::query_as(
        "SELECT COALESCE(SUM(ok AND channel = 'push'), 0), COALESCE(SUM(ok AND channel = 'email'), 0), \
         COALESCE(SUM(NOT ok), 0) FROM alarm_delivery WHERE sent_at >= ?",
    )
    .bind(since)
    .fetch_one(db)
    .await
}

/// Drops deliveries before `before`.
pub async fn forget(db: &SqlitePool, before: i64) -> sqlx::Result<u64> {
    Ok(sqlx::query("DELETE FROM alarm_delivery WHERE sent_at < ?")
        .bind(before)
        .execute(db)
        .await?
        .rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn keeps_and_counts_deliveries() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db = crate::db::connect(&dir.path().join("t.db")).await?;
        crate::db::migrate(&db).await?;
        let a = crate::account::create(&db, "h", "Ann", "k", "{}", 0).await?;
        let r = |channel, ok| Record {
            account_id: a,
            favorite_id: None,
            channel,
            kind: "test",
            title: "Test",
            ok,
            detail: (!ok).then_some("push 410"),
        };
        record(&db, 100, &r("push", true)).await?;
        record(&db, 200, &r("email", true)).await?;
        record(&db, 300, &r("push", false)).await?;
        assert_eq!(counts(&db, 0).await?, (1, 1, 1));
        let l = recent(&db, 10).await?;
        assert_eq!((l[0].ok, l[0].detail.as_deref()), (false, Some("push 410")));
        assert_eq!(l[0].account.as_deref(), Some("Ann"));
        assert_eq!(forget(&db, 250).await?, 2);
        Ok(())
    }
}
