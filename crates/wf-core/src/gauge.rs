//! Water gauges (PEGELONLINE): level, flow and water temperature, and the current estimated
//! from the flow.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use ts_rs::TS;

use crate::geo::distance_km;

/// A level against PEGELONLINE's marks; anything else it reports (commented, out-dated) is
/// unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum GaugeState {
    Low,
    Normal,
    High,
    Unknown,
}

impl GaugeState {
    pub fn parse(s: Option<&str>) -> Self {
        match s {
            Some("low") => Self::Low,
            Some("normal") => Self::Normal,
            Some("high") => Self::High,
            _ => Self::Unknown,
        }
    }
}

/// A gauge as the import delivers it, with its latest values.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct Gauge {
    pub uuid: String,
    pub number: String,
    pub name: String,
    pub water: Option<String>,
    pub km: Option<f64>,
    pub lat: f64,
    pub lon: f64,
    pub level_cm: Option<f64>,
    pub level_at: Option<i64>,
    pub level_state: Option<String>,
    pub shipping_state: Option<String>,
    pub flow_m3s: Option<f64>,
    pub flow_at: Option<i64>,
    pub water_temp_c: Option<f64>,
    pub water_temp_at: Option<i64>,
}

const COLUMNS: &str = "uuid, number, name, water, km, lat, lon, level_cm, level_at, level_state, \
    shipping_state, flow_m3s, flow_at, water_temp_c, water_temp_at";

/// Inserts or updates gauges by their PEGELONLINE id; returns how many were written.
pub async fn upsert(db: &SqlitePool, gauges: &[Gauge], seen_at: i64) -> sqlx::Result<usize> {
    let mut tx = db.begin().await?;
    for g in gauges {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO gauge ({COLUMNS}, seen_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT (uuid) DO UPDATE SET number = excluded.number, name = excluded.name, \
             water = excluded.water, km = excluded.km, lat = excluded.lat, lon = excluded.lon, \
             level_cm = excluded.level_cm, level_at = excluded.level_at, \
             level_state = excluded.level_state, shipping_state = excluded.shipping_state, \
             flow_m3s = excluded.flow_m3s, flow_at = excluded.flow_at, \
             water_temp_c = excluded.water_temp_c, water_temp_at = excluded.water_temp_at, \
             seen_at = excluded.seen_at"
        )))
        .bind(&g.uuid)
        .bind(&g.number)
        .bind(&g.name)
        .bind(&g.water)
        .bind(g.km)
        .bind(g.lat)
        .bind(g.lon)
        .bind(g.level_cm)
        .bind(g.level_at)
        .bind(&g.level_state)
        .bind(&g.shipping_state)
        .bind(g.flow_m3s)
        .bind(g.flow_at)
        .bind(g.water_temp_c)
        .bind(g.water_temp_at)
        .bind(seen_at)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(gauges.len())
}

/// Drops gauges the list has not had since `before`.
pub async fn remove_unseen(db: &SqlitePool, before: i64) -> sqlx::Result<u64> {
    Ok(sqlx::query("DELETE FROM gauge WHERE seen_at < ?")
        .bind(before)
        .execute(db)
        .await?
        .rows_affected())
}

/// The gauge nearest to a place within `max_km`, with its distance in km.
pub async fn nearest(
    db: &SqlitePool,
    lat: f64,
    lon: f64,
    max_km: f64,
) -> sqlx::Result<Option<(Gauge, f64)>> {
    let dlat = max_km / 111.0;
    let dlon = max_km / (111.0 * lat.to_radians().cos().max(0.05));
    let candidates: Vec<Gauge> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM gauge WHERE lat BETWEEN ? AND ? AND lon BETWEEN ? AND ?"
    )))
    .bind(lat - dlat)
    .bind(lat + dlat)
    .bind(lon - dlon)
    .bind(lon + dlon)
    .fetch_all(db)
    .await?;
    Ok(candidates
        .into_iter()
        .map(|g| {
            let d = distance_km(lat, lon, g.lat, g.lon);
            (g, d)
        })
        .filter(|(_, d)| *d <= max_km)
        .min_by(|a, b| a.1.total_cmp(&b.1)))
}

/// A gauge with its level, for the map's pins.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GaugePin {
    pub uuid: String,
    pub name: String,
    pub water: Option<String>,
    pub lat: f64,
    pub lon: f64,
    /// cm against the gauge's zero; null without a reading since the asked time
    pub level_cm: Option<f64>,
    pub state: GaugeState,
}

/// Every gauge, with its level when it was read at `since` or later.
pub async fn pins(db: &SqlitePool, since: i64) -> sqlx::Result<Vec<GaugePin>> {
    type Row = (
        String,
        String,
        Option<String>,
        f64,
        f64,
        Option<f64>,
        Option<i64>,
        Option<String>,
    );
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT uuid, name, water, lat, lon, level_cm, level_at, level_state FROM gauge ORDER BY uuid",
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(uuid, name, water, lat, lon, level, at, state)| {
            let fresh = at.is_some_and(|t| t >= since);
            GaugePin {
                uuid,
                name,
                water,
                lat,
                lon,
                level_cm: level.filter(|_| fresh),
                state: if fresh {
                    GaugeState::parse(state.as_deref())
                } else {
                    GaugeState::Unknown
                },
            }
        })
        .collect())
}

/// Surface current in m/s, estimated from the flow through a blanket cross-section: for the
/// Rhine 300 m wide and 6 m deep (as in the Flugwetterdaten project), the surface flowing
/// about 1.3 times as fast as the mean. Other waters have no cross-section yet, so no estimate.
pub fn estimated_current_ms(water: Option<&str>, flow_m3s: f64) -> Option<f64> {
    const RHINE_M2: f64 = 300.0 * 6.0;
    const SURFACE: f64 = 1.3;
    (water == Some("Rhein") && flow_m3s > 0.0).then(|| flow_m3s / RHINE_M2 * SURFACE)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn iffezheim() -> Gauge {
        Gauge {
            uuid: "b02be240-1364-4c97-8bb6-675d7d842332".into(),
            number: "23500600".into(),
            name: "Iffezheim".into(),
            water: Some("Rhein".into()),
            km: Some(336.2),
            lat: 48.852149,
            lon: 8.116902,
            level_cm: Some(62.0),
            level_at: Some(1_791_710_100),
            level_state: Some("low".into()),
            shipping_state: Some("unknown".into()),
            flow_m3s: Some(358.0),
            flow_at: Some(1_791_709_200),
            water_temp_c: None,
            water_temp_at: None,
        }
    }

    #[test]
    fn current_estimate() {
        // 358 m³/s at Iffezheim: 0.26 m/s at the surface, about half a knot.
        let v = estimated_current_ms(Some("Rhein"), 358.0).unwrap();
        assert!((v - 0.2586).abs() < 0.001, "{v}");
        assert_eq!(estimated_current_ms(Some("Mosel"), 358.0), None);
        assert_eq!(GaugeState::parse(Some("out-dated")), GaugeState::Unknown);
    }

    #[tokio::test]
    async fn finds_the_nearest() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db = crate::db::connect(&dir.path().join("t.db")).await?;
        crate::db::migrate(&db).await?;
        let mut far = iffezheim();
        far.uuid = "x".into();
        far.name = "Maxau".into();
        far.lat = 49.03;
        upsert(&db, &[iffezheim(), far], 10).await?;
        // From Baden Airpark (EDSB)
        let (g, d) = nearest(&db, 48.7794, 8.0805, 15.0).await?.unwrap();
        assert_eq!(g, iffezheim());
        assert!((8.0..9.0).contains(&d), "{d}");
        assert!(nearest(&db, 36.4, 28.1, 15.0).await?.is_none());
        // On the map: Iffezheim's level while fresh, none once older than asked
        let all = pins(&db, 1_791_710_000).await?;
        let iff = all.iter().find(|p| p.name == "Iffezheim").unwrap();
        assert_eq!((iff.level_cm, iff.state), (Some(62.0), GaugeState::Low));
        let old = pins(&db, 1_791_710_200).await?;
        let iff = old.iter().find(|p| p.name == "Iffezheim").unwrap();
        assert_eq!((iff.level_cm, iff.state), (None, GaugeState::Unknown));
        assert_eq!(all.len(), 2);
        upsert(&db, &[iffezheim()], 20).await?;
        assert_eq!(remove_unseen(&db, 20).await?, 1);
        Ok(())
    }
}
