//! A user's gear: kites, wings, windsurf sails and parawings, each with the mean wind (kn) it
//! works in. The app shows per hour which of them fits; the kind only names it.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use ts_rs::TS;

/// Pieces an account may keep
pub const MAX_GEAR: usize = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum GearKind {
    Kite,
    Wing,
    Windsurf,
    Parawing,
}

impl GearKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Kite => "kite",
            Self::Wing => "wing",
            Self::Windsurf => "windsurf",
            Self::Parawing => "parawing",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "kite" => Self::Kite,
            "wing" => Self::Wing,
            "windsurf" => Self::Windsurf,
            "parawing" => Self::Parawing,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GearItem {
    pub kind: GearKind,
    /// m²
    pub size: f64,
    /// Mean wind it works in, kn
    pub min_kn: f64,
    pub max_kn: f64,
}

impl GearItem {
    /// A size up to 50 m² and a wind range within 0–80 kn, low below high
    pub fn valid(&self) -> bool {
        self.size > 0.0
            && self.size <= 50.0
            && self.min_kn >= 0.0
            && self.max_kn <= 80.0
            && self.min_kn < self.max_kn
    }
}

pub async fn list(db: &SqlitePool, account_id: i64) -> sqlx::Result<Vec<GearItem>> {
    let rows: Vec<(String, f64, f64, f64)> = sqlx::query_as(
        "SELECT kind, size, min_kn, max_kn FROM gear WHERE account_id = ? ORDER BY position",
    )
    .bind(account_id)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(kind, size, min_kn, max_kn)| {
            Some(GearItem {
                kind: GearKind::parse(&kind)?,
                size,
                min_kn,
                max_kn,
            })
        })
        .collect())
}

/// Puts the account's gear as given, in this order.
pub async fn replace(db: &SqlitePool, account_id: i64, items: &[GearItem]) -> sqlx::Result<()> {
    let mut tx = db.begin().await?;
    sqlx::query("DELETE FROM gear WHERE account_id = ?")
        .bind(account_id)
        .execute(&mut *tx)
        .await?;
    for (position, g) in items.iter().enumerate() {
        sqlx::query(
            "INSERT INTO gear (account_id, kind, size, min_kn, max_kn, position) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(account_id)
        .bind(g.kind.as_str())
        .bind(g.size)
        .bind(g.min_kn)
        .bind(g.max_kn)
        .bind(position as i64)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn keeps_an_accounts_gear_in_order() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db = crate::db::connect(&dir.path().join("t.db")).await?;
        crate::db::migrate(&db).await?;
        let me = crate::account::create(&db, "u1", "A", "c1", "{}", 0).await?;
        let kite = GearItem {
            kind: GearKind::Kite,
            size: 12.0,
            min_kn: 13.0,
            max_kn: 20.0,
        };
        let wing = GearItem {
            kind: GearKind::Wing,
            size: 4.0,
            min_kn: 14.0,
            max_kn: 30.0,
        };
        replace(&db, me, &[wing, kite]).await?;
        assert_eq!(list(&db, me).await?, vec![wing, kite]);
        replace(&db, me, &[kite]).await?;
        assert_eq!(list(&db, me).await?, vec![kite]);
        assert!(kite.valid());
        assert!(
            !GearItem {
                max_kn: 10.0,
                ..kite
            }
            .valid()
        );
        assert!(!GearItem { size: 0.0, ..kite }.valid());
        Ok(())
    }
}
