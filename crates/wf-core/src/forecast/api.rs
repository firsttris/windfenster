//! The forecast as the API sends it.

use serde::Serialize;
use ts_rs::TS;

use super::StoredSeries;
use crate::model::Model;

/// One model's forecast as the API sends it. Value i belongs to `start + i * step` (unix s).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ModelSeries {
    pub model: String,
    /// Initialisation time of the run.
    #[ts(type = "number")]
    pub run_at: i64,
    /// When Windfenster fetched it.
    #[ts(type = "number")]
    pub fetched_at: i64,
    #[ts(type = "number")]
    pub start: i64,
    /// Seconds between values.
    #[ts(type = "number")]
    pub step: i64,
    /// Mean wind at 10 m, kn
    pub wind: Vec<Option<f32>>,
    /// Gusts, kn
    pub gust: Vec<Option<f32>>,
    /// Direction the wind comes from, degrees
    pub dir: Vec<Option<f32>>,
    /// Temperature at 2 m, °C
    pub temp: Vec<Option<f32>>,
    /// Precipitation in the hour before, mm
    pub rain: Vec<Option<f32>>,
    /// Cloud cover, %
    pub cloud: Vec<Option<f32>>,
}

impl From<StoredSeries> for ModelSeries {
    fn from(s: StoredSeries) -> Self {
        Self {
            model: s.model_id,
            run_at: s.run_at,
            fetched_at: s.fetched_at,
            start: s.data.start,
            step: s.data.step_s,
            wind: s.data.wind,
            gust: s.data.gust,
            dir: s.data.dir,
            temp: s.data.temp,
            rain: s.data.rain,
            cloud: s.data.cloud,
        }
    }
}

/// Everything the forecast table of a place needs.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Forecast {
    /// The grid point the series belong to.
    pub lat: f64,
    pub lon: f64,
    /// IANA time zone of the place, e.g. Europe/Berlin.
    pub timezone: Option<String>,
    pub elevation_m: Option<f64>,
    /// Models whose domain covers the place, in their default order.
    pub models: Vec<Model>,
    /// The newest series per model; models without values here are left out.
    pub series: Vec<ModelSeries>,
    /// Set when the newest data could not be fetched; the series are then older or missing.
    pub notice: Option<Notice>,
}

/// Why a forecast is not current. The SPA shows a text per code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Notice {
    /// The weather source did not answer or failed.
    SourceUnavailable,
    /// The day's request budget for the weather source is used up.
    BudgetExhausted,
}
