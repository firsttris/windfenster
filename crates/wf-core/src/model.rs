use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use ts_rs::TS;

/// Which selection rule a model serves. "Superforecast" and "Forecast" are not models of their
/// own: Superforecast takes the best regional model (≤ 3 km) whose domain covers the place,
/// Forecast the best global one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, TS)]
#[serde(rename_all = "lowercase")]
#[sqlx(rename_all = "lowercase")]
#[ts(export)]
pub enum ModelClass {
    Super,
    Global,
}

/// Computational domain of a regional model, in degrees.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BBox {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
}

impl BBox {
    pub fn contains(&self, lat: f64, lon: f64) -> bool {
        (self.south..=self.north).contains(&lat) && (self.west..=self.east).contains(&lon)
    }
}

/// A weather model as listed in the `model` table (static, maintained by migrations).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Model {
    /// Open-Meteo's model id, e.g. `icon_d2`.
    pub id: String,
    pub name: String,
    pub provider: String,
    pub resolution_km: f64,
    pub class: ModelClass,
    /// Computational domain; global models have none.
    pub domain: Option<BBox>,
    pub max_lead_h: u32,
    pub runs_per_day: u32,
}

impl Model {
    pub fn covers(&self, lat: f64, lon: f64) -> bool {
        self.domain.is_none_or(|d| d.contains(lat, lon))
    }
}

#[derive(sqlx::FromRow)]
struct ModelRow {
    id: String,
    name: String,
    provider: String,
    resolution_km: f64,
    class: ModelClass,
    bbox_west: Option<f64>,
    bbox_south: Option<f64>,
    bbox_east: Option<f64>,
    bbox_north: Option<f64>,
    max_lead_h: u32,
    runs_per_day: u32,
}

impl From<ModelRow> for Model {
    fn from(r: ModelRow) -> Self {
        let domain = match (r.bbox_west, r.bbox_south, r.bbox_east, r.bbox_north) {
            (Some(west), Some(south), Some(east), Some(north)) => Some(BBox {
                west,
                south,
                east,
                north,
            }),
            _ => None,
        };
        Self {
            id: r.id,
            name: r.name,
            provider: r.provider,
            resolution_km: r.resolution_km,
            class: r.class,
            domain,
            max_lead_h: r.max_lead_h,
            runs_per_day: r.runs_per_day,
        }
    }
}

/// All models, regional ones first, each group by resolution.
pub async fn list(db: &SqlitePool) -> sqlx::Result<Vec<Model>> {
    let rows: Vec<ModelRow> = sqlx::query_as(
        "SELECT id, name, provider, resolution_km, class, bbox_west, bbox_south, bbox_east, \
         bbox_north, max_lead_h, runs_per_day FROM model \
         ORDER BY class = 'global', resolution_km, id",
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(Model::from).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage() {
        let regional = Model {
            id: "test_d2".into(),
            name: "Test".into(),
            provider: "Test".into(),
            resolution_km: 2.2,
            class: ModelClass::Super,
            domain: Some(BBox {
                west: -4.0,
                south: 43.0,
                east: 20.0,
                north: 58.0,
            }),
            max_lead_h: 48,
            runs_per_day: 8,
        };
        assert!(regional.covers(48.78, 8.08)); // Baden Airpark
        assert!(!regional.covers(36.4, 28.09)); // Rhodes
        let global = Model {
            domain: None,
            class: ModelClass::Global,
            ..regional
        };
        assert!(global.covers(36.4, 28.09));
    }
}
