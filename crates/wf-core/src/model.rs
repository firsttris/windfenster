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
    /// Default order while a station has no verification data (lower first).
    pub rank: u32,
    /// Open-Meteo's dataset name for the run metadata (`/data/{meta_id}/static/meta.json`).
    #[serde(skip)]
    #[ts(skip)]
    pub meta_id: Option<String>,
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
    rank: u32,
    meta_id: Option<String>,
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
            rank: r.rank,
            meta_id: r.meta_id,
        }
    }
}

/// All models in their default order (regional ones first).
pub async fn list(db: &SqlitePool) -> sqlx::Result<Vec<Model>> {
    let rows: Vec<ModelRow> = sqlx::query_as(
        "SELECT id, name, provider, resolution_km, class, bbox_west, bbox_south, bbox_east, \
         bbox_north, max_lead_h, runs_per_day, rank, meta_id FROM model \
         ORDER BY class = 'global', rank, id",
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(Model::from).collect())
}

/// The models to use: all of them, or those named in `only` (e.g. from `WF_MODELS`), in the
/// default order. Unknown ids in `only` are an error, so a typo does not go unnoticed.
pub async fn active(db: &SqlitePool, only: Option<&[String]>) -> anyhow::Result<Vec<Model>> {
    let all = list(db).await?;
    let Some(only) = only else { return Ok(all) };
    if let Some(unknown) = only.iter().find(|id| !all.iter().any(|m| &m.id == *id)) {
        anyhow::bail!(
            "unknown model '{unknown}'; known: {}",
            all.iter()
                .map(|m| m.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    Ok(all.into_iter().filter(|m| only.contains(&m.id)).collect())
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
            rank: 10,
            meta_id: None,
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
