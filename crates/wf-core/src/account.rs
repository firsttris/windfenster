//! Accounts, their passkeys and sessions. Signing in is the API's business (WebAuthn); this
//! keeps what it needs: an account per WebAuthn user handle, each passkey as webauthn-rs
//! serialises it, and sessions by the SHA-256 of their token.

use serde::Serialize;
use sqlx::SqlitePool;
use ts_rs::TS;

/// How long a session lasts: half a year, renewed while it is used
pub const SESSION_S: i64 = 180 * 86_400;
/// A session's end is pushed out at most this often
const RENEW_S: i64 = 86_400;
/// Longest name an account may have, characters
pub const MAX_NAME: usize = 60;

/// The signed-in user, as the app shows it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Account {
    #[ts(type = "number")]
    pub id: i64,
    pub name: String,
    /// Passkeys registered (on how many devices or managers one can sign in)
    #[ts(type = "number")]
    pub passkeys: i64,
}

/// A name as stored: trimmed, inner whitespace collapsed; None when empty or too long.
pub fn clean_name(name: &str) -> Option<String> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    (!name.is_empty() && name.chars().count() <= MAX_NAME).then_some(name)
}

/// A new account with its first passkey; returns its id.
pub async fn create(
    db: &SqlitePool,
    handle: &str,
    name: &str,
    passkey_id: &str,
    credential: &str,
    now: i64,
) -> sqlx::Result<i64> {
    let mut tx = db.begin().await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO account (handle, name, created_at) VALUES (?, ?, ?) RETURNING id",
    )
    .bind(handle)
    .bind(name)
    .bind(now)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO passkey (id, account_id, credential, created_at) VALUES (?, ?, ?, ?)")
        .bind(passkey_id)
        .bind(id)
        .bind(credential)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(id)
}

/// Another passkey for an account (a second device or password manager).
pub async fn add_passkey(
    db: &SqlitePool,
    account_id: i64,
    passkey_id: &str,
    credential: &str,
    now: i64,
) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO passkey (id, account_id, credential, created_at) VALUES (?, ?, ?, ?)")
        .bind(passkey_id)
        .bind(account_id)
        .bind(credential)
        .bind(now)
        .execute(db)
        .await?;
    Ok(())
}

/// The account a user handle belongs to and its passkeys (serialised).
pub async fn by_handle(db: &SqlitePool, handle: &str) -> sqlx::Result<Option<(i64, Vec<String>)>> {
    let Some(id): Option<i64> = sqlx::query_scalar("SELECT id FROM account WHERE handle = ?")
        .bind(handle)
        .fetch_optional(db)
        .await?
    else {
        return Ok(None);
    };
    Ok(Some((id, passkeys(db, id).await?)))
}

/// An account's handle and passkeys (serialised), to register another without repeating one.
pub async fn handle_and_passkeys(
    db: &SqlitePool,
    account_id: i64,
) -> sqlx::Result<Option<(String, String, Vec<String>)>> {
    let Some((handle, name)): Option<(String, String)> =
        sqlx::query_as("SELECT handle, name FROM account WHERE id = ?")
            .bind(account_id)
            .fetch_optional(db)
            .await?
    else {
        return Ok(None);
    };
    Ok(Some((handle, name, passkeys(db, account_id).await?)))
}

async fn passkeys(db: &SqlitePool, account_id: i64) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar("SELECT credential FROM passkey WHERE account_id = ? ORDER BY created_at")
        .bind(account_id)
        .fetch_all(db)
        .await
}

/// A passkey after it signed someone in: its counter may have moved.
pub async fn used_passkey(
    db: &SqlitePool,
    passkey_id: &str,
    credential: Option<&str>,
    now: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE passkey SET last_used_at = ?, credential = COALESCE(?, credential) WHERE id = ?",
    )
    .bind(now)
    .bind(credential)
    .bind(passkey_id)
    .execute(db)
    .await?;
    Ok(())
}

/// An account as the app shows it.
pub async fn get(db: &SqlitePool, account_id: i64) -> sqlx::Result<Option<Account>> {
    let row: Option<(i64, String, i64)> = sqlx::query_as(
        "SELECT a.id, a.name, (SELECT COUNT(*) FROM passkey p WHERE p.account_id = a.id) \
         FROM account a WHERE a.id = ?",
    )
    .bind(account_id)
    .fetch_optional(db)
    .await?;
    Ok(row.map(|(id, name, passkeys)| Account { id, name, passkeys }))
}

/// A new session for the account, by the hash of its token.
pub async fn start_session(
    db: &SqlitePool,
    token_hash: &[u8],
    account_id: i64,
    now: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO session (token_hash, account_id, created_at, expires_at) VALUES (?, ?, ?, ?)",
    )
    .bind(token_hash)
    .bind(account_id)
    .bind(now)
    .bind(now + SESSION_S)
    .execute(db)
    .await?;
    Ok(())
}

/// The account of a session that has not run out; a session in use lasts on.
pub async fn of_session(
    db: &SqlitePool,
    token_hash: &[u8],
    now: i64,
) -> sqlx::Result<Option<Account>> {
    let row: Option<(i64, String, i64, i64)> = sqlx::query_as(
        "SELECT a.id, a.name, s.expires_at, \
         (SELECT COUNT(*) FROM passkey p WHERE p.account_id = a.id) \
         FROM session s JOIN account a ON a.id = s.account_id \
         WHERE s.token_hash = ? AND s.expires_at > ?",
    )
    .bind(token_hash)
    .bind(now)
    .fetch_optional(db)
    .await?;
    let Some((id, name, expires_at, passkeys)) = row else {
        return Ok(None);
    };
    if expires_at - now < SESSION_S - RENEW_S {
        sqlx::query("UPDATE session SET expires_at = ? WHERE token_hash = ?")
            .bind(now + SESSION_S)
            .bind(token_hash)
            .execute(db)
            .await?;
    }
    Ok(Some(Account { id, name, passkeys }))
}

pub async fn end_session(db: &SqlitePool, token_hash: &[u8]) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM session WHERE token_hash = ?")
        .bind(token_hash)
        .execute(db)
        .await?;
    Ok(())
}

/// Sessions that ran out; returns how many went.
pub async fn prune_sessions(db: &SqlitePool, now: i64) -> sqlx::Result<u64> {
    Ok(sqlx::query("DELETE FROM session WHERE expires_at <= ?")
        .bind(now)
        .execute(db)
        .await?
        .rows_affected())
}

pub async fn rename(db: &SqlitePool, account_id: i64, name: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE account SET name = ? WHERE id = ?")
        .bind(name)
        .bind(account_id)
        .execute(db)
        .await?;
    Ok(())
}

/// The account with its passkeys, sessions and favourites.
pub async fn delete(db: &SqlitePool, account_id: i64) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM account WHERE id = ?")
        .bind(account_id)
        .execute(db)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn db() -> anyhow::Result<(tempfile::TempDir, SqlitePool)> {
        let dir = tempfile::tempdir()?;
        let db = crate::db::connect(&dir.path().join("t.db")).await?;
        crate::db::migrate(&db).await?;
        Ok((dir, db))
    }

    #[test]
    fn cleans_names() {
        assert_eq!(clean_name("  Tristan   T. "), Some("Tristan T.".into()));
        assert_eq!(clean_name("   "), None);
        assert_eq!(clean_name(&"x".repeat(61)), None);
        assert_eq!(clean_name(&"ä".repeat(60)), Some("ä".repeat(60)));
    }

    #[tokio::test]
    async fn keeps_accounts_passkeys_and_sessions() -> anyhow::Result<()> {
        let (_dir, db) = db().await?;
        let id = create(&db, "uuid-1", "Tristan", "cred-1", "{}", 100).await?;
        add_passkey(&db, id, "cred-2", "{\"n\":2}", 200).await?;
        let (found, keys) = by_handle(&db, "uuid-1").await?.unwrap();
        assert_eq!((found, keys.len()), (id, 2));
        assert!(by_handle(&db, "uuid-2").await?.is_none());
        used_passkey(&db, "cred-2", Some("{\"n\":3}"), 300).await?;
        let (_, name, keys) = handle_and_passkeys(&db, id).await?.unwrap();
        assert_eq!((name.as_str(), keys[1].as_str()), ("Tristan", "{\"n\":3}"));

        // A session: found while it lasts, renewed when used a day on, gone when ended
        start_session(&db, b"hash", id, 1_000).await?;
        let me = of_session(&db, b"hash", 1_000).await?.unwrap();
        assert_eq!(
            me,
            Account {
                id,
                name: "Tristan".into(),
                passkeys: 2
            }
        );
        assert!(of_session(&db, b"other", 1_000).await?.is_none());
        let expires = async || -> sqlx::Result<i64> {
            sqlx::query_scalar("SELECT expires_at FROM session WHERE token_hash = ?")
                .bind(b"hash".as_slice())
                .fetch_one(&db)
                .await
        };
        // Used again within a day: unchanged; two days on: half a year from then
        of_session(&db, b"hash", 1_000 + 3_600).await?.unwrap();
        assert_eq!(expires().await?, 1_000 + SESSION_S);
        let later = 1_000 + 2 * 86_400;
        of_session(&db, b"hash", later).await?.unwrap();
        assert_eq!(expires().await?, later + SESSION_S);
        assert!(of_session(&db, b"hash", later + SESSION_S).await?.is_none());
        assert_eq!(prune_sessions(&db, later + SESSION_S).await?, 1);

        start_session(&db, b"two", id, 1_000).await?;
        end_session(&db, b"two").await?;
        assert!(of_session(&db, b"two", 1_000).await?.is_none());

        assert_eq!(get(&db, id).await?.map(|a| a.passkeys), Some(2));
        rename(&db, id, "T.").await?;
        start_session(&db, b"three", id, 1_000).await?;
        assert_eq!(of_session(&db, b"three", 1_000).await?.unwrap().name, "T.");
        delete(&db, id).await?;
        assert!(of_session(&db, b"three", 1_000).await?.is_none());
        assert!(by_handle(&db, "uuid-1").await?.is_none());
        Ok(())
    }
}
