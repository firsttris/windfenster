//! An account's passkeys as its owner sees them on the account page, and removing one (a lost
//! phone, a password manager no longer used). The last way to sign in stays: a passkey goes only
//! while another one or a password is left.

use serde::Serialize;
use sqlx::SqlitePool;
use ts_rs::TS;

/// A passkey of the signed-in account. What it was made on is not known (webauthn-rs keeps no
/// name), so the dates tell them apart.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PasskeySummary {
    /// The credential id, hex
    pub id: String,
    #[ts(type = "number")]
    pub created_at: i64,
    /// When it last signed in; null when never since it was made
    #[ts(type = "number | null")]
    pub last_used_at: Option<i64>,
}

/// The account's passkeys, oldest first.
pub async fn list(db: &SqlitePool, account_id: i64) -> sqlx::Result<Vec<PasskeySummary>> {
    let rows: Vec<(String, i64, Option<i64>)> = sqlx::query_as(
        "SELECT id, created_at, last_used_at FROM passkey WHERE account_id = ? \
         ORDER BY created_at, id",
    )
    .bind(account_id)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, created_at, last_used_at)| PasskeySummary {
            id,
            created_at,
            last_used_at,
        })
        .collect())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Removal {
    Removed,
    /// No passkey of this account has the id
    NotFound,
    /// It is the account's only way to sign in (no other passkey, no password)
    LastSignIn,
}

/// Removes a passkey of the account unless it is the last way to sign in. One statement, so two
/// removals at once cannot take the last two passkeys of an account without a password.
pub async fn remove(db: &SqlitePool, account_id: i64, passkey_id: &str) -> sqlx::Result<Removal> {
    let removed = sqlx::query(
        "DELETE FROM passkey WHERE id = ?1 AND account_id = ?2 AND ( \
         (SELECT COUNT(*) FROM passkey WHERE account_id = ?2) > 1 \
         OR (SELECT password_hash FROM account WHERE id = ?2) IS NOT NULL)",
    )
    .bind(passkey_id)
    .bind(account_id)
    .execute(db)
    .await?
    .rows_affected()
        > 0;
    if removed {
        return Ok(Removal::Removed);
    }
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM passkey WHERE id = ? AND account_id = ?)")
            .bind(passkey_id)
            .bind(account_id)
            .fetch_one(db)
            .await?;
    Ok(if exists {
        Removal::LastSignIn
    } else {
        Removal::NotFound
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::{add_passkey, create, create_with_password, get, set_password};

    #[tokio::test]
    async fn removes_all_but_the_last_way_to_sign_in() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db = crate::db::connect(&dir.path().join("t.db")).await?;
        crate::db::migrate(&db).await?;
        let id = create(&db, "u1", "A", "k1", "{}", 100).await?;
        add_passkey(&db, id, "k2", "{}", 200).await?;
        crate::account::used_passkey(&db, "k2", None, 300).await?;
        assert_eq!(
            list(&db, id).await?,
            vec![
                PasskeySummary {
                    id: "k1".into(),
                    created_at: 100,
                    last_used_at: None
                },
                PasskeySummary {
                    id: "k2".into(),
                    created_at: 200,
                    last_used_at: Some(300)
                },
            ]
        );

        // Another account's passkey is not found from this one.
        let other = create(&db, "u2", "B", "b1", "{}", 0).await?;
        add_passkey(&db, other, "b2", "{}", 0).await?;
        assert_eq!(remove(&db, id, "b1").await?, Removal::NotFound);
        assert_eq!(remove(&db, id, "nope").await?, Removal::NotFound);

        // One of two goes; the last stays while there is no password.
        assert_eq!(remove(&db, id, "k1").await?, Removal::Removed);
        assert_eq!(remove(&db, id, "k2").await?, Removal::LastSignIn);
        assert_eq!(get(&db, id).await?.map(|a| a.passkeys), Some(1));

        // With a password, the last passkey may go too.
        set_password(&db, id, "a@x.de", "hash").await?;
        assert_eq!(remove(&db, id, "k2").await?, Removal::Removed);
        assert!(list(&db, id).await?.is_empty());

        // A password account removes a passkey it added.
        let p = create_with_password(&db, "u3", "C", "c@x.de", "hash", 0).await?;
        add_passkey(&db, p, "c1", "{}", 0).await?;
        assert_eq!(remove(&db, p, "c1").await?, Removal::Removed);
        Ok(())
    }
}
