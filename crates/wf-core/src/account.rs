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
    /// For signing in with a password; null when the account has none
    pub email: Option<String>,
    /// Whether its owner confirmed the address by the link sent to it; alarm e-mails go only to
    /// a confirmed one
    pub email_confirmed: bool,
    /// Passkeys registered (on how many devices or managers one can sign in)
    #[ts(type = "number")]
    pub passkeys: i64,
    /// May open the admin page (made one on the server with `windfenster admin`)
    pub admin: bool,
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
    let row: Option<(i64, String, Option<String>, bool, i64, String)> = sqlx::query_as(
        "SELECT a.id, a.name, a.email, a.email_confirmed, \
         (SELECT COUNT(*) FROM passkey p WHERE p.account_id = a.id), a.role \
         FROM account a WHERE a.id = ?",
    )
    .bind(account_id)
    .fetch_optional(db)
    .await?;
    Ok(row.map(
        |(id, name, email, email_confirmed, passkeys, role)| Account {
            id,
            name,
            email,
            email_confirmed,
            passkeys,
            admin: role == "admin",
        },
    ))
}

/// An e-mail address as stored: trimmed and lower-case; None when it does not look like one.
pub fn clean_email(email: &str) -> Option<String> {
    let e = email.trim().to_lowercase();
    let (local, domain) = e.split_once('@')?;
    let ok = !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && e.len() <= 254
        && !e.chars().any(|c| c.is_whitespace() || c == ',');
    ok.then_some(e)
}

/// A new account that signs in with e-mail and password (Argon2 hash); returns its id.
pub async fn create_with_password(
    db: &SqlitePool,
    handle: &str,
    name: &str,
    email: &str,
    password_hash: &str,
    now: i64,
) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "INSERT INTO account (handle, name, email, password_hash, created_at) \
         VALUES (?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(handle)
    .bind(name)
    .bind(email)
    .bind(password_hash)
    .bind(now)
    .fetch_one(db)
    .await
}

/// Whether another account than `except` uses the address.
pub async fn email_taken(db: &SqlitePool, email: &str, except: Option<i64>) -> sqlx::Result<bool> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM account WHERE email = ? AND id IS NOT ?)")
        .bind(email)
        .bind(except)
        .fetch_one(db)
        .await
}

/// The account with this address and its password hash, if it has one.
pub async fn by_email(db: &SqlitePool, email: &str) -> sqlx::Result<Option<(i64, Option<String>)>> {
    sqlx::query_as("SELECT id, password_hash FROM account WHERE email = ?")
        .bind(email)
        .fetch_optional(db)
        .await
}

pub async fn password_hash(db: &SqlitePool, account_id: i64) -> sqlx::Result<Option<String>> {
    Ok(
        sqlx::query_scalar("SELECT password_hash FROM account WHERE id = ?")
            .bind(account_id)
            .fetch_optional(db)
            .await?
            .flatten(),
    )
}

/// Sets the address and password an account signs in with; a new address is unconfirmed.
pub async fn set_password(
    db: &SqlitePool,
    account_id: i64,
    email: &str,
    password_hash: &str,
) -> sqlx::Result<()> {
    // SQLite reads the old row on the right-hand side: `email = ?1` compares with the old address.
    sqlx::query(
        "UPDATE account SET email_confirmed = (email_confirmed AND email IS ?1), \
         email = ?1, password_hash = ?2 WHERE id = ?3",
    )
    .bind(email)
    .bind(password_hash)
    .bind(account_id)
    .execute(db)
    .await?;
    Ok(())
}

/// Keeps a link to confirm `email` for the account, by the hash of its token, until `expires_at`;
/// links that ran out go along the way.
pub async fn add_email_confirmation(
    db: &SqlitePool,
    token_hash: &[u8],
    account_id: i64,
    email: &str,
    now: i64,
    expires_at: i64,
) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM email_confirmation WHERE expires_at <= ?")
        .bind(now)
        .execute(db)
        .await?;
    sqlx::query(
        "INSERT INTO email_confirmation (token_hash, account_id, email, expires_at) \
         VALUES (?, ?, ?, ?)",
    )
    .bind(token_hash)
    .bind(account_id)
    .bind(email)
    .bind(expires_at)
    .execute(db)
    .await?;
    Ok(())
}

/// Confirms the address a link was sent to, once; false when the link is unknown, ran out, or
/// the account has another address by now.
pub async fn confirm_email(db: &SqlitePool, token_hash: &[u8], now: i64) -> sqlx::Result<bool> {
    let mut tx = db.begin().await?;
    let row: Option<(i64, String)> = sqlx::query_as(
        "DELETE FROM email_confirmation WHERE token_hash = ? AND expires_at > ? \
         RETURNING account_id, email",
    )
    .bind(token_hash)
    .bind(now)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((account_id, email)) = row else {
        return Ok(false);
    };
    let done = sqlx::query("UPDATE account SET email_confirmed = 1 WHERE id = ? AND email = ?")
        .bind(account_id)
        .bind(&email)
        .execute(&mut *tx)
        .await?
        .rows_affected()
        > 0;
    tx.commit().await?;
    Ok(done)
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
    seen(db, account_id, now).await
}

/// id, name, email, confirmed, session end, passkeys, role
type SessionRow = (i64, String, Option<String>, bool, i64, i64, String);

/// The account of a session that has not run out, unless the account is disabled; a session in
/// use lasts on, and the account counts as seen.
pub async fn of_session(
    db: &SqlitePool,
    token_hash: &[u8],
    now: i64,
) -> sqlx::Result<Option<Account>> {
    let row: Option<SessionRow> = sqlx::query_as(
        "SELECT a.id, a.name, a.email, a.email_confirmed, s.expires_at, \
         (SELECT COUNT(*) FROM passkey p WHERE p.account_id = a.id), a.role \
         FROM session s JOIN account a ON a.id = s.account_id \
         WHERE s.token_hash = ? AND s.expires_at > ? AND a.disabled_at IS NULL",
    )
    .bind(token_hash)
    .bind(now)
    .fetch_optional(db)
    .await?;
    let Some((id, name, email, email_confirmed, expires_at, passkeys, role)) = row else {
        return Ok(None);
    };
    if expires_at - now < SESSION_S - RENEW_S {
        sqlx::query("UPDATE session SET expires_at = ? WHERE token_hash = ?")
            .bind(now + SESSION_S)
            .bind(token_hash)
            .execute(db)
            .await?;
        seen(db, id, now).await?;
    }
    Ok(Some(Account {
        id,
        name,
        email,
        email_confirmed,
        passkeys,
        admin: role == "admin",
    }))
}

/// Marks the account as seen now.
pub async fn seen(db: &SqlitePool, account_id: i64, now: i64) -> sqlx::Result<()> {
    sqlx::query("UPDATE account SET last_seen_at = ? WHERE id = ?")
        .bind(now)
        .bind(account_id)
        .execute(db)
        .await?;
    Ok(())
}

/// Whether an admin disabled the account: it may not sign in.
pub async fn disabled(db: &SqlitePool, account_id: i64) -> sqlx::Result<bool> {
    Ok(
        sqlx::query_scalar::<_, Option<i64>>("SELECT disabled_at FROM account WHERE id = ?")
            .bind(account_id)
            .fetch_optional(db)
            .await?
            .flatten()
            .is_some(),
    )
}

pub async fn end_session(db: &SqlitePool, token_hash: &[u8]) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM session WHERE token_hash = ?")
        .bind(token_hash)
        .execute(db)
        .await?;
    Ok(())
}

/// Links to confirm an address that ran out; returns how many went.
pub async fn prune_email_confirmations(db: &SqlitePool, now: i64) -> sqlx::Result<u64> {
    Ok(
        sqlx::query("DELETE FROM email_confirmation WHERE expires_at <= ?")
            .bind(now)
            .execute(db)
            .await?
            .rows_affected(),
    )
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

    #[test]
    fn cleans_addresses() {
        assert_eq!(
            clean_email(" Tristan@Example.org "),
            Some("tristan@example.org".into())
        );
        for bad in ["", "a@b", "@x.de", "a@.de", "a@x.", "a b@x.de", "nope"] {
            assert_eq!(clean_email(bad), None, "{bad}");
        }
    }

    #[tokio::test]
    async fn signs_in_with_a_password_too() -> anyhow::Result<()> {
        let (_dir, db) = db().await?;
        let id = create_with_password(&db, "u1", "A", "a@x.de", "hash", 0).await?;
        assert_eq!(
            by_email(&db, "a@x.de").await?,
            Some((id, Some("hash".into())))
        );
        assert!(email_taken(&db, "a@x.de", None).await?);
        assert!(!email_taken(&db, "a@x.de", Some(id)).await?);
        // Two accounts cannot share an address.
        assert!(
            create_with_password(&db, "u2", "B", "a@x.de", "h", 0)
                .await
                .is_err()
        );
        // A passkey account adds one later.
        let other = create(&db, "u3", "C", "c1", "{}", 0).await?;
        assert_eq!(password_hash(&db, other).await?, None);
        set_password(&db, other, "c@x.de", "h2").await?;
        assert_eq!(
            get(&db, other).await?.unwrap().email.as_deref(),
            Some("c@x.de")
        );
        assert_eq!(password_hash(&db, other).await?.as_deref(), Some("h2"));
        Ok(())
    }

    #[tokio::test]
    async fn confirms_an_address_once_by_its_link() -> anyhow::Result<()> {
        let (_dir, db) = db().await?;
        let id = create_with_password(&db, "u1", "A", "a@x.de", "hash", 0).await?;
        let confirmed =
            async || -> anyhow::Result<bool> { Ok(get(&db, id).await?.unwrap().email_confirmed) };
        assert!(!confirmed().await?);
        // A link that ran out, or an unknown one, confirms nothing.
        add_email_confirmation(&db, b"old", id, "a@x.de", 0, 100).await?;
        assert!(!confirm_email(&db, b"old", 100).await?);
        assert!(!confirm_email(&db, b"nope", 50).await?);
        // A link confirms once.
        add_email_confirmation(&db, b"t1", id, "a@x.de", 200, 1_000).await?;
        assert!(confirm_email(&db, b"t1", 500).await?);
        assert!(confirmed().await?);
        assert!(!confirm_email(&db, b"t1", 600).await?);
        // The same address again stays confirmed, a new one is not.
        set_password(&db, id, "a@x.de", "h2").await?;
        assert!(confirmed().await?);
        set_password(&db, id, "b@x.de", "h3").await?;
        assert!(!confirmed().await?);
        // A link sent to the old address does not confirm the new one.
        add_email_confirmation(&db, b"t2", id, "a@x.de", 700, 2_000).await?;
        add_email_confirmation(&db, b"t3", id, "b@x.de", 700, 2_000).await?;
        assert!(!confirm_email(&db, b"t2", 800).await?);
        assert!(!confirmed().await?);
        assert!(confirm_email(&db, b"t3", 800).await?);
        assert!(confirmed().await?);
        // Links that ran out go in the hourly purge, the others stay.
        add_email_confirmation(&db, b"t4", id, "b@x.de", 800, 5_000).await?;
        let links = async || -> sqlx::Result<Vec<Vec<u8>>> {
            sqlx::query_scalar("SELECT token_hash FROM email_confirmation ORDER BY token_hash")
                .fetch_all(&db)
                .await
        };
        prune_email_confirmations(&db, 2_000).await?;
        assert_eq!(links().await?, vec![b"t4".to_vec()]);
        assert_eq!(prune_email_confirmations(&db, 5_000).await?, 1);
        assert!(links().await?.is_empty());
        Ok(())
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
                email: None,
                email_confirmed: false,
                passkeys: 2,
                admin: false
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
