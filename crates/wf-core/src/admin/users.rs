//! Accounts as admins see them: the list with what each holds, one account in detail, roles,
//! disabling, signing out everywhere, and an account's data as JSON (for its owner, GDPR).

use serde::Serialize;
use sqlx::SqlitePool;
use ts_rs::TS;

/// An account in the admin page's list.
#[derive(Debug, Clone, PartialEq, Serialize, TS, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AdminUser {
    #[ts(type = "number")]
    pub id: i64,
    pub name: String,
    pub email: Option<String>,
    pub email_confirmed: bool,
    #[ts(type = "number")]
    pub passkeys: i64,
    pub password: bool,
    pub admin: bool,
    /// When an admin disabled it; null when it may sign in
    #[ts(type = "number | null")]
    pub disabled_at: Option<i64>,
    #[ts(type = "number")]
    pub created_at: i64,
    /// When a session of it was last used (to a day); null when never since this was kept
    #[ts(type = "number | null")]
    pub last_seen_at: Option<i64>,
    #[ts(type = "number")]
    pub favorites: i64,
    #[ts(type = "number")]
    pub alarms: i64,
}

/// One account in detail.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AdminUserDetail {
    pub user: AdminUser,
    pub locale: String,
    pub time_zone: String,
    pub unit: String,
    #[ts(type = "number")]
    pub sessions: i64,
    #[ts(type = "number")]
    pub push_devices: i64,
    #[ts(type = "number")]
    pub gear: i64,
    /// Alarm messages handed to a channel in the last 30 days
    #[ts(type = "number")]
    pub deliveries: i64,
}

/// Counts for the page's head.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UserSummary {
    #[ts(type = "number")]
    pub total: i64,
    /// Made in the last 7 days
    #[ts(type = "number")]
    pub new_week: i64,
    /// Seen in the last 30 days
    #[ts(type = "number")]
    pub active_month: i64,
    #[ts(type = "number")]
    pub admins: i64,
    #[ts(type = "number")]
    pub disabled: i64,
}

const USER_COLUMNS: &str = "a.id, a.name, a.email, a.email_confirmed, \
     (SELECT COUNT(*) FROM passkey p WHERE p.account_id = a.id) AS passkeys, \
     a.password_hash IS NOT NULL AS password, a.role = 'admin' AS admin, a.disabled_at, \
     a.created_at, a.last_seen_at, \
     (SELECT COUNT(*) FROM favorite f WHERE f.account_id = a.id) AS favorites, \
     (SELECT COUNT(*) FROM alarm l JOIN favorite f ON f.id = l.favorite_id \
      WHERE f.account_id = a.id) AS alarms";

/// Every account, the most recently seen first.
pub async fn list(db: &SqlitePool) -> sqlx::Result<Vec<AdminUser>> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {USER_COLUMNS} FROM account a \
         ORDER BY COALESCE(a.last_seen_at, a.created_at) DESC, a.id DESC"
    )))
    .fetch_all(db)
    .await
}

pub async fn get(db: &SqlitePool, id: i64) -> sqlx::Result<Option<AdminUser>> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {USER_COLUMNS} FROM account a WHERE a.id = ?"
    )))
    .bind(id)
    .fetch_optional(db)
    .await
}

pub async fn detail(db: &SqlitePool, id: i64, now: i64) -> sqlx::Result<Option<AdminUserDetail>> {
    let Some(user) = get(db, id).await? else {
        return Ok(None);
    };
    let (locale, time_zone, unit): (String, String, String) =
        sqlx::query_as("SELECT locale, time_zone, unit FROM account WHERE id = ?")
            .bind(id)
            .fetch_one(db)
            .await?;
    let count = |sql: &'static str| async move {
        sqlx::query_scalar::<_, i64>(sql)
            .bind(id)
            .bind(now)
            .fetch_one(db)
            .await
    };
    Ok(Some(AdminUserDetail {
        user,
        locale,
        time_zone,
        unit,
        sessions: count("SELECT COUNT(*) FROM session WHERE account_id = ? AND expires_at > ?")
            .await?,
        push_devices: count(
            "SELECT COUNT(*) FROM push_subscription WHERE account_id = ? AND ? IS NOT NULL",
        )
        .await?,
        gear: count("SELECT COUNT(*) FROM gear WHERE account_id = ? AND ? IS NOT NULL").await?,
        deliveries: count(
            "SELECT COUNT(*) FROM alarm_delivery WHERE account_id = ? AND sent_at > ? - 30 * 86400",
        )
        .await?,
    }))
}

pub async fn summary(db: &SqlitePool, now: i64) -> sqlx::Result<UserSummary> {
    let (total, new_week, active_month, admins, disabled): (i64, i64, i64, i64, i64) =
        sqlx::query_as(
            "SELECT COUNT(*), \
             COALESCE(SUM(created_at > ?1 - 7 * 86400), 0), \
             COALESCE(SUM(last_seen_at > ?1 - 30 * 86400), 0), \
             COALESCE(SUM(role = 'admin'), 0), \
             COALESCE(SUM(disabled_at IS NOT NULL), 0) FROM account",
        )
        .bind(now)
        .fetch_one(db)
        .await?;
    Ok(UserSummary {
        total,
        new_week,
        active_month,
        admins,
        disabled,
    })
}

/// The account a name or an address names, for `windfenster admin`: the address exactly, else
/// the one account with that name. Several with the name: their ids, to pick one by `#id`.
pub async fn find(db: &SqlitePool, who: &str) -> sqlx::Result<Found> {
    let who = who.trim();
    if let Some(id) = who.strip_prefix('#').and_then(|n| n.parse::<i64>().ok()) {
        return Ok(match get(db, id).await? {
            Some(u) => Found::One(u),
            None => Found::None,
        });
    }
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM account WHERE email = lower(?1) \
         UNION SELECT id FROM account WHERE name = ?1 AND NOT EXISTS \
         (SELECT 1 FROM account WHERE email = lower(?1))",
    )
    .bind(who)
    .fetch_all(db)
    .await?;
    Ok(match ids[..] {
        [] => Found::None,
        [id] => get(db, id).await?.map_or(Found::None, Found::One),
        _ => Found::Several(ids),
    })
}

#[derive(Debug, PartialEq)]
pub enum Found {
    None,
    One(AdminUser),
    Several(Vec<i64>),
}

/// The accounts with the admin role.
pub async fn admins(db: &SqlitePool) -> sqlx::Result<Vec<AdminUser>> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {USER_COLUMNS} FROM account a WHERE a.role = 'admin' ORDER BY a.id"
    )))
    .fetch_all(db)
    .await
}

/// Gives or takes the admin role. Taking it from the last admin who may still sign in is
/// refused (false): nobody could open the admin page any more.
pub async fn set_admin(db: &SqlitePool, id: i64, admin: bool) -> sqlx::Result<bool> {
    let mut tx = db.begin().await?;
    if !admin {
        let others: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM account WHERE role = 'admin' AND disabled_at IS NULL AND id != ?",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        if others == 0 {
            return Ok(false);
        }
    }
    sqlx::query("UPDATE account SET role = ? WHERE id = ?")
        .bind(if admin { "admin" } else { "user" })
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(true)
}

/// Disables an account: no sign-in, its sessions end, its alarms rest. The last admin who may
/// sign in cannot be disabled (false).
pub async fn disable(db: &SqlitePool, id: i64, now: i64) -> sqlx::Result<bool> {
    let mut tx = db.begin().await?;
    let last_admin: bool = sqlx::query_scalar(
        "SELECT role = 'admin' AND NOT EXISTS (SELECT 1 FROM account o WHERE o.role = 'admin' \
         AND o.disabled_at IS NULL AND o.id != a.id) FROM account a WHERE a.id = ?",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .unwrap_or(false);
    if last_admin {
        return Ok(false);
    }
    sqlx::query("UPDATE account SET disabled_at = COALESCE(disabled_at, ?) WHERE id = ?")
        .bind(now)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM session WHERE account_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(true)
}

pub async fn enable(db: &SqlitePool, id: i64) -> sqlx::Result<()> {
    sqlx::query("UPDATE account SET disabled_at = NULL WHERE id = ?")
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}

/// Ends every session of the account; returns how many.
pub async fn sign_out_everywhere(db: &SqlitePool, id: i64) -> sqlx::Result<u64> {
    Ok(sqlx::query("DELETE FROM session WHERE account_id = ?")
        .bind(id)
        .execute(db)
        .await?
        .rows_affected())
}

/// Deletes an account and all it holds, unless it is the last admin who may sign in (false).
pub async fn delete(db: &SqlitePool, id: i64) -> sqlx::Result<bool> {
    let last_admin: bool = sqlx::query_scalar(
        "SELECT role = 'admin' AND disabled_at IS NULL AND NOT EXISTS (SELECT 1 FROM account o \
         WHERE o.role = 'admin' AND o.disabled_at IS NULL AND o.id != a.id) \
         FROM account a WHERE a.id = ?",
    )
    .bind(id)
    .fetch_optional(db)
    .await?
    .unwrap_or(false);
    if last_admin {
        return Ok(false);
    }
    crate::account::delete(db, id).await?;
    Ok(true)
}

/// What the server keeps about an account, for its owner: the account (without the password
/// hash and the passkeys' keys), favourites with alarms, gear, devices for push (their address
/// only), sessions (times only).
pub async fn export(db: &SqlitePool, id: i64) -> sqlx::Result<Option<serde_json::Value>> {
    use serde_json::{Value, json};
    let Some(user) = get(db, id).await? else {
        return Ok(None);
    };
    let rows = |sql: &'static str| async move {
        let rows: Vec<(String,)> = sqlx::query_as(sql).bind(id).fetch_all(db).await?;
        Ok::<Vec<Value>, sqlx::Error>(
            rows.into_iter()
                .filter_map(|(j,)| serde_json::from_str(&j).ok())
                .collect(),
        )
    };
    Ok(Some(json!({
        "account": user,
        "favorites": rows(
            "SELECT json_object('name', f.name, 'slug', f.slug, 'lat', f.lat, 'lon', f.lon, \
             'windFrom', f.wind_from, 'windTo', f.wind_to, 'createdAt', f.created_at, \
             'alarm', CASE WHEN a.favorite_id IS NULL THEN NULL ELSE json_object( \
               'minKn', a.min_kn, 'forecast', a.forecast, 'live', a.live, 'push', a.push, \
               'email', a.email) END) \
             FROM favorite f LEFT JOIN alarm a ON a.favorite_id = f.id \
             WHERE f.account_id = ? ORDER BY f.position"
        ).await?,
        "gear": rows(
            "SELECT json_object('kind', kind, 'size', size, 'minKn', min_kn, 'maxKn', max_kn) \
             FROM gear WHERE account_id = ? ORDER BY position"
        ).await?,
        "pushDevices": rows(
            "SELECT json_object('endpoint', endpoint, 'createdAt', created_at) \
             FROM push_subscription WHERE account_id = ?"
        ).await?,
        "passkeys": rows(
            "SELECT json_object('createdAt', created_at, 'lastUsedAt', last_used_at) \
             FROM passkey WHERE account_id = ?"
        ).await?,
        "sessions": rows(
            "SELECT json_object('createdAt', created_at, 'expiresAt', expires_at) \
             FROM session WHERE account_id = ?"
        ).await?,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account;

    async fn db() -> anyhow::Result<(tempfile::TempDir, SqlitePool)> {
        let dir = tempfile::tempdir()?;
        let db = crate::db::connect(&dir.path().join("t.db")).await?;
        crate::db::migrate(&db).await?;
        Ok((dir, db))
    }

    #[tokio::test]
    async fn roles_and_the_last_admin() -> anyhow::Result<()> {
        let (_d, db) = db().await?;
        let a = account::create_with_password(&db, "h1", "Ann", "ann@x.de", "h", 100).await?;
        let b = account::create(&db, "h2", "Ben", "k2", "{}", 200).await?;
        assert!(matches!(find(&db, "ANN@x.de").await?, Found::One(u) if u.id == a));
        assert!(matches!(find(&db, "Ben").await?, Found::One(u) if u.id == b));
        assert!(matches!(find(&db, &format!("#{b}")).await?, Found::One(_)));
        assert_eq!(find(&db, "nobody").await?, Found::None);
        account::create(&db, "h3", "Ben", "k3", "{}", 300).await?;
        assert!(matches!(find(&db, "Ben").await?, Found::Several(ids) if ids.len() == 2));

        assert!(set_admin(&db, a, true).await?);
        assert!(account::get(&db, a).await?.unwrap().admin);
        // The only admin keeps the role, cannot be disabled or deleted
        assert!(!set_admin(&db, a, false).await?);
        assert!(!disable(&db, a, 400).await?);
        assert!(!delete(&db, a).await?);
        assert!(set_admin(&db, b, true).await?);
        assert!(set_admin(&db, a, false).await?);
        assert_eq!(admins(&db).await?.len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn a_disabled_account_cannot_use_its_sessions() -> anyhow::Result<()> {
        let (_d, db) = db().await?;
        let a = account::create(&db, "h1", "Ann", "k1", "{}", 100).await?;
        account::start_session(&db, b"s1", a, 100).await?;
        assert!(account::of_session(&db, b"s1", 200).await?.is_some());
        assert!(disable(&db, a, 300).await?);
        assert!(account::disabled(&db, a).await?);
        assert!(account::of_session(&db, b"s1", 400).await?.is_none());
        let u = get(&db, a).await?.unwrap();
        assert_eq!((u.disabled_at, u.last_seen_at), (Some(300), Some(100)));
        enable(&db, a).await?;
        assert!(!account::disabled(&db, a).await?);
        account::start_session(&db, b"s2", a, 500).await?;
        account::start_session(&db, b"s3", a, 500).await?;
        assert_eq!(sign_out_everywhere(&db, a).await?, 2);
        Ok(())
    }

    #[tokio::test]
    async fn lists_counts_and_exports() -> anyhow::Result<()> {
        let (_d, db) = db().await?;
        let now = 100 * 86_400;
        let a = account::create_with_password(&db, "h1", "Ann", "ann@x.de", "h", now).await?;
        crate::favorite::add(
            &db,
            a,
            &crate::favorite::FavoriteInput {
                slug: None,
                lat: 48.78,
                lon: 8.08,
                name: "Rhein".into(),
                sector: None,
                model: None,
            },
            now,
        )
        .await?;
        let users = list(&db).await?;
        assert_eq!(users.len(), 1);
        assert_eq!((users[0].favorites, users[0].password), (1, true));
        let s = summary(&db, now).await?;
        assert_eq!((s.total, s.new_week, s.admins), (1, 1, 0));
        let d = detail(&db, a, now).await?.unwrap();
        assert_eq!((d.sessions, d.locale.as_str()), (0, "de"));
        let e = export(&db, a).await?.unwrap();
        assert_eq!(e["favorites"][0]["name"], "Rhein");
        assert_eq!(e["account"]["email"], "ann@x.de");
        assert!(e.to_string().find("password_hash").is_none());
        Ok(())
    }
}
