//! An account's passkeys as its owner sees them on the account page: named after the browser and
//! system they were added with, or by the user; one no longer wanted (a lost phone, a password
//! manager given up) is removed. The last way to sign in stays: a passkey goes only while another
//! one or a password is left.

use serde::Serialize;
use sqlx::SqlitePool;
use ts_rs::TS;

/// A passkey of the signed-in account. webauthn-rs keeps no name for it, so the browser and
/// system come from the User-Agent of the request that added it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PasskeySummary {
    /// The credential id, hex
    pub id: String,
    /// The user's own name for it; null shows the browser and system
    pub name: Option<String>,
    /// e.g. "Chrome", "Safari"; null when not known
    pub browser: Option<String>,
    /// e.g. "Android", "iOS", "macOS"; null when not known
    pub os: Option<String>,
    #[ts(type = "number")]
    pub created_at: i64,
    /// When it last signed in; null when never since it was made
    #[ts(type = "number | null")]
    pub last_used_at: Option<i64>,
}

/// The account's passkeys, oldest first.
pub async fn list(db: &SqlitePool, account_id: i64) -> sqlx::Result<Vec<PasskeySummary>> {
    let rows: Vec<Row> = sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM passkey WHERE account_id = ? ORDER BY created_at, id"
    ))
    .bind(account_id)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(summary).collect())
}

const COLUMNS: &str = "id, name, browser, os, created_at, last_used_at";
type Row = (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    i64,
    Option<i64>,
);

fn summary((id, name, browser, os, created_at, last_used_at): Row) -> PasskeySummary {
    PasskeySummary {
        id,
        name,
        browser,
        os,
        created_at,
        last_used_at,
    }
}

/// Notes the browser and system a passkey was added with.
pub async fn set_device(db: &SqlitePool, passkey_id: &str, device: Device) -> sqlx::Result<()> {
    sqlx::query("UPDATE passkey SET browser = ?, os = ? WHERE id = ?")
        .bind(device.browser)
        .bind(device.os)
        .bind(passkey_id)
        .execute(db)
        .await?;
    Ok(())
}

/// Gives a passkey of the account the user's own name (None: back to the browser and system);
/// None when the account has no passkey with the id.
pub async fn rename(
    db: &SqlitePool,
    account_id: i64,
    passkey_id: &str,
    name: Option<&str>,
) -> sqlx::Result<Option<PasskeySummary>> {
    let row: Option<Row> = sqlx::query_as(&format!(
        "UPDATE passkey SET name = ? WHERE id = ? AND account_id = ? RETURNING {COLUMNS}"
    ))
    .bind(name)
    .bind(passkey_id)
    .bind(account_id)
    .fetch_optional(db)
    .await?;
    Ok(row.map(summary))
}

/// The browser and system a request came from, as far as its User-Agent tells.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Device {
    pub browser: Option<&'static str>,
    pub os: Option<&'static str>,
}

/// Reads the browser and system off a User-Agent. Order matters: Edge, Opera and Samsung
/// Internet name Chrome too, Chrome names Safari, and iOS browsers all are Safari underneath
/// (they name themselves by their own token: CriOS, FxiOS, EdgiOS).
pub fn device_of(user_agent: &str) -> Device {
    let ua = user_agent;
    let has = |t: &str| ua.contains(t);
    let browser = if has("Edg/") || has("EdgA/") || has("EdgiOS/") {
        Some("Edge")
    } else if has("OPR/") || has("OPiOS/") {
        Some("Opera")
    } else if has("SamsungBrowser/") {
        Some("Samsung Internet")
    } else if has("Firefox/") || has("FxiOS/") {
        Some("Firefox")
    } else if has("Chrome/") || has("CriOS/") {
        Some("Chrome")
    } else if has("Safari/") && has("Version/") {
        Some("Safari")
    } else {
        None
    };
    let os = if has("iPhone") || has("iPod") {
        Some("iOS")
    } else if has("iPad") {
        Some("iPadOS")
    } else if has("Android") {
        Some("Android")
    } else if has("CrOS") {
        Some("ChromeOS")
    } else if has("Mac OS X") || has("Macintosh") {
        Some("macOS")
    } else if has("Windows") {
        Some("Windows")
    } else if has("Linux") {
        Some("Linux")
    } else {
        None
    };
    Device { browser, os }
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

    #[test]
    fn reads_browser_and_system_off_the_user_agent() {
        let cases = [
            (
                "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) \
                 Chrome/129.0.0.0 Mobile Safari/537.36",
                Some("Chrome"),
                Some("Android"),
            ),
            (
                "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 \
                 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1",
                Some("Safari"),
                Some("iOS"),
            ),
            (
                "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 \
                 (KHTML, like Gecko) CriOS/129.0.6668.69 Mobile/15E148 Safari/604.1",
                Some("Chrome"),
                Some("iOS"),
            ),
            (
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 \
                 (KHTML, like Gecko) Version/18.0 Safari/605.1.15",
                Some("Safari"),
                Some("macOS"),
            ),
            (
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) \
                 Chrome/129.0.0.0 Safari/537.36 Edg/129.0.0.0",
                Some("Edge"),
                Some("Windows"),
            ),
            (
                "Mozilla/5.0 (X11; Linux x86_64; rv:131.0) Gecko/20100101 Firefox/131.0",
                Some("Firefox"),
                Some("Linux"),
            ),
            (
                "Mozilla/5.0 (Linux; Android 14; SM-S921B) AppleWebKit/537.36 (KHTML, like Gecko) \
                 SamsungBrowser/26.0 Chrome/122.0.0.0 Mobile Safari/537.36",
                Some("Samsung Internet"),
                Some("Android"),
            ),
            ("curl/8.5.0", None, None),
            ("", None, None),
        ];
        for (ua, browser, os) in cases {
            assert_eq!(device_of(ua), Device { browser, os }, "{ua}");
        }
    }

    #[tokio::test]
    async fn names_a_passkey_by_its_device_or_by_the_user() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db = crate::db::connect(&dir.path().join("t.db")).await?;
        crate::db::migrate(&db).await?;
        let id = create(&db, "u1", "A", "k1", "{}", 100).await?;
        let other = create(&db, "u2", "B", "b1", "{}", 0).await?;
        set_device(
            &db,
            "k1",
            Device {
                browser: Some("Chrome"),
                os: Some("Android"),
            },
        )
        .await?;
        let k = rename(&db, id, "k1", Some("Pixel")).await?.unwrap();
        assert_eq!(
            (k.name.as_deref(), k.browser.as_deref(), k.os.as_deref()),
            (Some("Pixel"), Some("Chrome"), Some("Android"))
        );
        assert_eq!(list(&db, id).await?, vec![k]);
        // Back to the device; another account's passkey is not found from this one.
        assert_eq!(rename(&db, id, "k1", None).await?.unwrap().name, None);
        assert_eq!(rename(&db, id, "b1", Some("x")).await?, None);
        assert_eq!(list(&db, other).await?[0].name, None);
        Ok(())
    }

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
                    name: None,
                    browser: None,
                    os: None,
                    created_at: 100,
                    last_used_at: None
                },
                PasskeySummary {
                    id: "k2".into(),
                    name: None,
                    browser: None,
                    os: None,
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
