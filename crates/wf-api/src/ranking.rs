//! "Who hits here?": the station's scores of the last 30 days, the models they choose, and
//! the Windfenster mix weighted by them.

use std::sync::Arc;
use std::time::Duration;

use moka::future::Cache;
use serde::Serialize;
use ts_rs::TS;
use wf_core::forecast::{Forecast, ModelSeries, SeriesData};
use wf_core::mix;
use wf_core::model::{Model, ModelClass};
use wf_core::verification::{self, MIN_DAYS, MIX, ModelScore, Score, TAF, Weights};
use wf_ingest::openmeteo::FORECAST_DAYS;

use crate::AppState;

/// A free point takes a station's ranking up to this far away.
pub const RANKING_MAX_KM: f64 = 5.0;

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Ranking {
    /// Days of the last 30 with verified hours (the most of any entry)
    #[ts(type = "number")]
    pub days: i64,
    /// The first day ahead (0–24 h), lowest error first: models, "taf" and "mix" with
    /// verified hours
    pub scores: Vec<ModelScore>,
    /// The regional model with the lowest error 0–24 h, once 14 days are verified
    pub best_super: Option<String>,
    /// The global model with the lowest error 48–72 h (else 0–24 h), once 14 days are verified;
    /// only one that reaches the ten days, since the page fills them with it
    pub best_global: Option<String>,
    /// The mix beat the best single model 0–24 h over 14 days or more; it is then the default
    pub mix_first: bool,
}

pub struct Verified {
    pub ranking: Ranking,
    pub weights: Weights,
}

pub type RankingCache = Cache<i64, Arc<Verified>>;

/// The scores change once an hour, with the verification.
pub fn new_cache() -> RankingCache {
    Cache::builder()
        .max_capacity(10_000)
        .time_to_live(Duration::from_secs(3_600))
        .build()
}

/// Hours of the page's ten days: Forecast fills them, so a global model that stops sooner
/// (ICON-EU, 5 days) is never its pick.
const TABLE_H: u32 = FORECAST_DAYS * 24;

fn best(scores: &[Score], bucket: u8, class: ModelClass, models: &[Model]) -> Option<String> {
    let fits =
        |m: &Model| m.class == class && (class != ModelClass::Global || m.max_lead_h >= TABLE_H);
    scores
        .iter()
        .filter(|s| {
            s.bucket == bucket
                && s.days >= MIN_DAYS
                && models.iter().any(|m| m.id == s.model && fits(m))
        })
        .filter_map(|s| Some((s.errors.mae()?, &s.model)))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, m)| m.clone())
}

pub fn ranking(scores: &[Score], models: &[Model]) -> Ranking {
    let first: Vec<&Score> = scores.iter().filter(|s| s.bucket == 0).collect();
    let mut list: Vec<ModelScore> = first
        .iter()
        .filter(|s| s.model == TAF || s.model == MIX || models.iter().any(|m| m.id == s.model))
        .filter_map(|s| ModelScore::new(&s.model, s.days, &s.errors))
        .collect();
    list.sort_by(|a, b| a.mae.total_cmp(&b.mae));
    let single = list
        .iter()
        .filter(|s| s.model != TAF && s.model != MIX && s.days >= MIN_DAYS)
        .map(|s| s.mae)
        .reduce(f64::min);
    let mix_first = matches!(
        (list.iter().find(|s| s.model == MIX), single),
        (Some(mix), Some(best)) if mix.days >= MIN_DAYS && mix.mae < best
    );
    Ranking {
        days: first.iter().map(|s| s.days).max().unwrap_or(0),
        best_super: best(scores, 0, ModelClass::Super, models),
        best_global: best(scores, 2, ModelClass::Global, models)
            .or_else(|| best(scores, 0, ModelClass::Global, models)),
        mix_first,
        scores: list,
    }
}

impl AppState {
    /// The station's ranking and mix weights, held for an hour.
    pub(crate) async fn verified(
        &self,
        station_id: i64,
        models: &[Model],
    ) -> sqlx::Result<Arc<Verified>> {
        if let Some(v) = self.rankings.get(&station_id).await {
            return Ok(v);
        }
        let (from, to) = verification::ranking_days(wf_core::time::now());
        let scores = verification::scores(&self.db, station_id, from, to).await?;
        let v = Arc::new(Verified {
            ranking: ranking(&scores, models),
            weights: Weights::from_scores(&scores),
        });
        self.rankings.insert(station_id, v.clone()).await;
        Ok(v)
    }
}

fn data(s: &ModelSeries) -> SeriesData {
    SeriesData {
        start: s.start,
        step_s: s.step,
        wind: s.wind.clone(),
        gust: s.gust.clone(),
        dir: s.dir.clone(),
        temp: s.temp.clone(),
        rain: s.rain.clone(),
        cloud: s.cloud.clone(),
    }
}

/// The forecast with the mix in front: a virtual model and its series, weighted by lead time
/// from `now` (hours past and the first day with the 0–24 h weights).
pub fn with_mix(fc: &Forecast, weights: &Weights, now: i64) -> Forecast {
    let mut out = fc.clone();
    if fc.series.len() < 2 {
        return out;
    }
    let data: Vec<(String, SeriesData)> = fc
        .series
        .iter()
        .map(|s| (s.model.clone(), data(s)))
        .collect();
    let inputs: Vec<(&str, &SeriesData)> = data.iter().map(|(m, d)| (m.as_str(), d)).collect();
    let mixed = mix::series(&inputs, |m, t| {
        let b = ((t - now).max(0) / 86_400).min(i64::from(verification::BUCKETS) - 1) as u8;
        weights.weight(m, b)
    });
    if mixed.is_empty() {
        return out;
    }
    let used: Vec<&Model> = fc
        .models
        .iter()
        .filter(|m| fc.series.iter().any(|s| s.model == m.id))
        .collect();
    out.models.insert(
        0,
        Model {
            id: MIX.into(),
            name: "Mix".into(),
            provider: "Windfenster".into(),
            resolution_km: used
                .iter()
                .map(|m| m.resolution_km)
                .fold(f64::INFINITY, f64::min),
            class: ModelClass::Mix,
            domain: None,
            max_lead_h: used.iter().map(|m| m.max_lead_h).max().unwrap_or(0),
            runs_per_day: used.iter().map(|m| m.runs_per_day).sum(),
            rank: 0,
            meta_id: None,
        },
    );
    out.series.insert(
        0,
        ModelSeries {
            model: MIX.into(),
            run_at: fc.series.iter().map(|s| s.run_at).max().unwrap_or(0),
            fetched_at: fc.series.iter().map(|s| s.fetched_at).max().unwrap_or(0),
            start: mixed.start,
            step: mixed.step_s,
            wind: mixed.wind,
            gust: mixed.gust,
            dir: mixed.dir,
            temp: mixed.temp,
            rain: mixed.rain,
            cloud: mixed.cloud,
        },
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_core::verification::Errors;

    fn model(id: &str, class: ModelClass) -> Model {
        Model {
            id: id.into(),
            name: id.into(),
            provider: String::new(),
            resolution_km: 2.0,
            class,
            domain: None,
            max_lead_h: if class == ModelClass::Global { 360 } else { 48 },
            runs_per_day: 8,
            rank: 1,
            meta_id: None,
        }
    }

    fn score(model: &str, bucket: u8, days: i64, mae: f64) -> Score {
        Score {
            model: model.into(),
            bucket,
            days,
            errors: Errors {
                n: 100,
                sum_abs: mae * 100.0,
                ..Errors::default()
            },
        }
    }

    #[test]
    fn picks_after_14_days() {
        let models = [
            model("icon_d2", ModelClass::Super),
            model("arome", ModelClass::Super),
            model("ecmwf", ModelClass::Global),
            model("gfs", ModelClass::Global),
        ];
        let scores = [
            score("icon_d2", 0, 20, 2.1),
            score("arome", 0, 20, 1.8),
            score("ecmwf", 0, 20, 2.4),
            score("gfs", 0, 20, 3.0),
            score("ecmwf", 2, 20, 3.1),
            score("gfs", 2, 20, 2.9),
            score(TAF, 0, 20, 1.9),
            score(MIX, 0, 20, 1.7),
        ];
        let r = ranking(&scores, &models);
        let order: Vec<&str> = r.scores.iter().map(|s| s.model.as_str()).collect();
        assert_eq!(order, ["mix", "arome", "taf", "icon_d2", "ecmwf", "gfs"]);
        assert_eq!(r.days, 20);
        assert_eq!(r.best_super.as_deref(), Some("arome"));
        // 48–72 h decides among the global models
        assert_eq!(r.best_global.as_deref(), Some("gfs"));
        assert!(r.mix_first);

        let young = [score("icon_d2", 0, 5, 2.1), score(MIX, 0, 5, 1.0)];
        let r = ranking(&young, &models);
        assert_eq!((r.days, r.best_super, r.mix_first), (5, None, false));
        assert_eq!(r.scores.len(), 2);
    }

    #[test]
    fn the_global_pick_reaches_the_ten_days() {
        let short = Model {
            max_lead_h: 120,
            ..model("icon_eu", ModelClass::Global)
        };
        let models = [short, model("ecmwf", ModelClass::Global)];
        let scores = [
            score("icon_eu", 2, 20, 2.0),
            score("ecmwf", 2, 20, 3.0),
            score("icon_eu", 0, 20, 1.5),
        ];
        let r = ranking(&scores, &models);
        assert_eq!(r.best_global.as_deref(), Some("ecmwf"));
        // still ranked with the others
        assert_eq!(r.scores[0].model, "icon_eu");
    }

    #[test]
    fn mix_goes_first() {
        let s = |model: &str, wind: f32| ModelSeries {
            model: model.into(),
            run_at: 100,
            fetched_at: 200,
            start: 0,
            step: 3_600,
            wind: vec![Some(wind); 3],
            gust: vec![None; 3],
            dir: vec![Some(270.0); 3],
            temp: vec![None; 3],
            rain: vec![None; 3],
            cloud: vec![None; 3],
        };
        let fc = Forecast {
            lat: 48.78,
            lon: 8.08,
            timezone: None,
            elevation_m: None,
            models: vec![
                model("a", ModelClass::Super),
                model("b", ModelClass::Global),
            ],
            series: vec![s("a", 10.0), s("b", 14.0)],
            notice: None,
        };
        let out = with_mix(&fc, &Weights::default(), 0);
        assert_eq!(out.models[0].id, "mix");
        assert_eq!(out.models[0].class, ModelClass::Mix);
        assert_eq!(out.series[0].wind, vec![Some(12.0); 3]);
        assert_eq!(out.series[0].dir[0], Some(270.0));
        assert_eq!(out.series.len(), 3);
    }
}
