//! Invitations, for when sign-up needs one: a link with a random token, of which only the
//! SHA-256 is kept, valid for a time and used once.

use serde::Serialize;
use sqlx::SqlitePool;
use ts_rs::TS;

/// How long an invitation works
pub const VALID_S: i64 = 14 * 86_400;

#[derive(Debug, Clone, PartialEq, Serialize, TS, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Invite {
    /// The token's hash in hex: names the invitation to withdraw it
    pub id: String,
    pub created_by: Option<String>,
    #[ts(type = "number")]
    pub created_at: i64,
    #[ts(type = "number")]
    pub expires_at: i64,
    #[ts(type = "number | null")]
    pub used_at: Option<i64>,
    pub used_by: Option<String>,
}

pub async fn create(db: &SqlitePool, token_hash: &[u8], by: i64, now: i64) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO invite (token_hash, created_by, created_at, expires_at) VALUES (?, ?, ?, ?)",
    )
    .bind(token_hash)
    .bind(by)
    .bind(now)
    .bind(now + VALID_S)
    .execute(db)
    .await?;
    Ok(())
}

/// Whether an invitation can be used now: known, not used, not run out.
pub async fn usable(db: &SqlitePool, token_hash: &[u8], now: i64) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM invite WHERE token_hash = ? AND used_at IS NULL \
         AND expires_at > ?)",
    )
    .bind(token_hash)
    .bind(now)
    .fetch_one(db)
    .await
}

/// Marks an invitation as used by a new account; false when it was not usable any more.
pub async fn take(
    db: &SqlitePool,
    token_hash: &[u8],
    account_id: i64,
    now: i64,
) -> sqlx::Result<bool> {
    Ok(sqlx::query(
        "UPDATE invite SET used_at = ?, used_by = ? WHERE token_hash = ? AND used_at IS NULL \
         AND expires_at > ?",
    )
    .bind(now)
    .bind(account_id)
    .bind(token_hash)
    .bind(now)
    .execute(db)
    .await?
    .rows_affected()
        == 1)
}

/// The invitations of the last 30 days and those still open, newest first.
pub async fn list(db: &SqlitePool, now: i64) -> sqlx::Result<Vec<Invite>> {
    sqlx::query_as(
        "SELECT lower(hex(i.token_hash)) AS id, c.name AS created_by, i.created_at, i.expires_at, \
         i.used_at, u.name AS used_by FROM invite i \
         LEFT JOIN account c ON c.id = i.created_by LEFT JOIN account u ON u.id = i.used_by \
         WHERE i.created_at > ? - 30 * 86400 OR (i.used_at IS NULL AND i.expires_at > ?) \
         ORDER BY i.created_at DESC",
    )
    .bind(now)
    .bind(now)
    .fetch_all(db)
    .await
}

/// Withdraws an open invitation by its id (the hash in hex).
pub async fn withdraw(db: &SqlitePool, id: &str) -> sqlx::Result<bool> {
    Ok(sqlx::query(
        "DELETE FROM invite WHERE lower(hex(token_hash)) = lower(?) AND used_at IS NULL",
    )
    .bind(id)
    .execute(db)
    .await?
    .rows_affected()
        == 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn an_invitation_works_once() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db = crate::db::connect(&dir.path().join("t.db")).await?;
        crate::db::migrate(&db).await?;
        let a = crate::account::create(&db, "h", "Ann", "k", "{}", 0).await?;
        create(&db, b"tok", a, 100).await?;
        assert!(usable(&db, b"tok", 200).await?);
        assert!(!usable(&db, b"tok", 100 + VALID_S).await?);
        let b = crate::account::create(&db, "h2", "Ben", "k2", "{}", 0).await?;
        assert!(take(&db, b"tok", b, 300).await?);
        assert!(!take(&db, b"tok", b, 301).await?);
        let l = list(&db, 400).await?;
        assert_eq!(l[0].used_by.as_deref(), Some("Ben"));
        assert_eq!(l[0].id, hex_of(b"tok"));
        create(&db, b"two", a, 500).await?;
        assert!(withdraw(&db, &hex_of(b"two")).await?);
        assert!(!withdraw(&db, &hex_of(b"tok")).await?);
        Ok(())
    }

    fn hex_of(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }
}
