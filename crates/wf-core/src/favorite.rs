//! A user's favourites: stations or points with their own name and, optionally, the wind
//! directions that work there and the model their forecast opens with.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use ts_rs::TS;

/// Favourites an account may keep
pub const MAX_FAVORITES: i64 = 100;

/// Wind directions that work at a spot: from `from` clockwise to `to`, degrees the wind comes
/// from (from 225 to 315 is south-west to north-west; from 300 to 60 crosses north).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Sector {
    pub from: u16,
    pub to: u16,
}

impl Sector {
    /// Both ends within 0…359
    pub fn valid(self) -> bool {
        self.from < 360 && self.to < 360
    }

    /// Whether wind from `deg` falls in the sector (ends included).
    pub fn holds(self, deg: f64) -> bool {
        let d = deg.rem_euclid(360.0);
        let (from, to) = (f64::from(self.from), f64::from(self.to));
        if from <= to {
            (from..=to).contains(&d)
        } else {
            d >= from || d <= to
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Favorite {
    #[ts(type = "number")]
    pub id: i64,
    /// The station's slug, or null for a point
    pub slug: Option<String>,
    pub lat: f64,
    pub lon: f64,
    pub name: String,
    pub sector: Option<Sector>,
    /// The model the forecast opens with (a model's id or "mix"); null for automatic
    pub model: Option<String>,
}

/// What the app sends to add or change a favourite.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FavoriteInput {
    /// A station's slug; else the point `lat`, `lon`
    pub slug: Option<String>,
    pub lat: f64,
    pub lon: f64,
    pub name: String,
    pub sector: Option<Sector>,
    /// Null or left out: automatic
    #[serde(default)]
    pub model: Option<String>,
}

type Row = (
    i64,
    Option<String>,
    f64,
    f64,
    String,
    Option<i64>,
    Option<i64>,
    Option<String>,
);

fn of_row((id, slug, lat, lon, name, from, to, model): Row) -> Favorite {
    let sector = match (from, to) {
        (Some(from), Some(to)) => Some(Sector {
            from: from as u16,
            to: to as u16,
        }),
        _ => None,
    };
    Favorite {
        id,
        slug,
        lat,
        lon,
        name,
        sector,
        model,
    }
}

pub async fn list(db: &SqlitePool, account_id: i64) -> sqlx::Result<Vec<Favorite>> {
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, slug, lat, lon, name, wind_from, wind_to, model FROM favorite \
         WHERE account_id = ? ORDER BY position, id",
    )
    .bind(account_id)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(of_row).collect())
}

/// Adds a favourite at the end; None when the account has `MAX_FAVORITES` already.
pub async fn add(
    db: &SqlitePool,
    account_id: i64,
    f: &FavoriteInput,
    now: i64,
) -> sqlx::Result<Option<Favorite>> {
    let (count, last): (i64, Option<i64>) =
        sqlx::query_as("SELECT COUNT(*), MAX(position) FROM favorite WHERE account_id = ?")
            .bind(account_id)
            .fetch_one(db)
            .await?;
    if count >= MAX_FAVORITES {
        return Ok(None);
    }
    let row: Row = sqlx::query_as(
        "INSERT INTO favorite (account_id, slug, lat, lon, name, wind_from, wind_to, model, \
         position, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         RETURNING id, slug, lat, lon, name, wind_from, wind_to, model",
    )
    .bind(account_id)
    .bind(&f.slug)
    .bind(f.lat)
    .bind(f.lon)
    .bind(&f.name)
    .bind(f.sector.map(|s| s.from))
    .bind(f.sector.map(|s| s.to))
    .bind(&f.model)
    .bind(last.map_or(0, |p| p + 1))
    .bind(now)
    .fetch_one(db)
    .await?;
    Ok(Some(of_row(row)))
}

/// Changes a favourite's name, directions and model (where it is stays); None when it is not
/// the account's.
pub async fn update(
    db: &SqlitePool,
    account_id: i64,
    id: i64,
    name: &str,
    sector: Option<Sector>,
    model: Option<&str>,
) -> sqlx::Result<Option<Favorite>> {
    let row: Option<Row> = sqlx::query_as(
        "UPDATE favorite SET name = ?, wind_from = ?, wind_to = ?, model = ? \
         WHERE id = ? AND account_id = ? \
         RETURNING id, slug, lat, lon, name, wind_from, wind_to, model",
    )
    .bind(name)
    .bind(sector.map(|s| s.from))
    .bind(sector.map(|s| s.to))
    .bind(model)
    .bind(id)
    .bind(account_id)
    .fetch_optional(db)
    .await?;
    Ok(row.map(of_row))
}

/// Removes a favourite; false when it is not the account's.
pub async fn remove(db: &SqlitePool, account_id: i64, id: i64) -> sqlx::Result<bool> {
    Ok(
        sqlx::query("DELETE FROM favorite WHERE id = ? AND account_id = ?")
            .bind(id)
            .bind(account_id)
            .execute(db)
            .await?
            .rows_affected()
            == 1,
    )
}

/// Puts the account's favourites in the order of `ids`; ids that are not its are ignored.
pub async fn reorder(db: &SqlitePool, account_id: i64, ids: &[i64]) -> sqlx::Result<()> {
    let mut tx = db.begin().await?;
    for (position, id) in ids.iter().enumerate() {
        sqlx::query("UPDATE favorite SET position = ? WHERE id = ? AND account_id = ?")
            .bind(position as i64)
            .bind(id)
            .bind(account_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sectors_hold_directions() {
        let west = Sector { from: 225, to: 315 };
        assert!(west.holds(270.0) && west.holds(225.0) && !west.holds(200.0));
        // Across north
        let north = Sector { from: 300, to: 60 };
        assert!(north.holds(0.0) && north.holds(330.0) && north.holds(-10.0));
        assert!(!north.holds(180.0));
        assert!(!Sector { from: 360, to: 10 }.valid());
    }

    #[tokio::test]
    async fn keeps_an_accounts_favourites() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db = crate::db::connect(&dir.path().join("t.db")).await?;
        crate::db::migrate(&db).await?;
        let me = crate::account::create(&db, "u1", "A", "c1", "{}", 0).await?;
        let other = crate::account::create(&db, "u2", "B", "c2", "{}", 0).await?;
        let edsb = FavoriteInput {
            slug: Some("edsb".into()),
            lat: 48.78,
            lon: 8.08,
            name: "Baggersee".into(),
            sector: Some(Sector { from: 200, to: 290 }),
            model: Some("arome".into()),
        };
        let point = FavoriteInput {
            slug: None,
            lat: 36.41,
            lon: 28.09,
            name: "Prasonisi".into(),
            sector: None,
            model: None,
        };
        let a = add(&db, me, &edsb, 1).await?.unwrap();
        let b = add(&db, me, &point, 2).await?.unwrap();
        assert_eq!(a.sector, Some(Sector { from: 200, to: 290 }));
        assert_eq!(
            (a.model.as_deref(), b.model.as_deref()),
            (Some("arome"), None)
        );
        assert_eq!(list(&db, me).await?, vec![a.clone(), b.clone()]);
        assert!(list(&db, other).await?.is_empty());

        reorder(&db, me, &[b.id, a.id]).await?;
        assert_eq!(list(&db, me).await?[0].name, "Prasonisi");
        // Someone else's favourite cannot be changed or removed.
        assert!(update(&db, other, a.id, "x", None, None).await?.is_none());
        assert!(!remove(&db, other, a.id).await?);
        let changed = update(&db, me, a.id, "Rhein", None, Some("mix"))
            .await?
            .unwrap();
        assert_eq!((changed.name.as_str(), changed.sector), ("Rhein", None));
        assert_eq!(changed.model.as_deref(), Some("mix"));
        assert!(remove(&db, me, b.id).await?);
        assert_eq!(list(&db, me).await?.len(), 1);
        // Gone with the account
        crate::account::delete(&db, me).await?;
        assert!(list(&db, me).await?.is_empty());
        Ok(())
    }
}
