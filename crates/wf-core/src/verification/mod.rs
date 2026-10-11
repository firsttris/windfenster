//! How well each model, the TAF and the mix hit at a station: errors per hour with a
//! measurement, summed per UTC day and lead time, and the scores over 30 days that pick the
//! model and weight the mix.

mod run;

use std::collections::HashMap;

use serde::Serialize;
use sqlx::SqlitePool;
use ts_rs::TS;

use crate::time;

pub use run::verify_day;

/// Hours: lead times 0–24, 24–48 and 48–72 h.
pub const BUCKETS: u8 = 3;
const BUCKET_S: i64 = 86_400;

/// "Good wind" for hits and false alarms.
pub const WINDOW_KN: f32 = 14.0;
/// Directions are compared from this measured wind on.
pub const DIR_MIN_KN: f32 = 5.0;
/// Days the ranking looks back.
pub const RANKING_DAYS: i64 = 30;
/// Days of data before the ranking chooses a model or weights the mix.
pub const MIN_DAYS: i64 = 14;

pub const TAF: &str = "taf";
pub const MIX: &str = "mix";

pub fn bucket(lead_s: i64) -> Option<u8> {
    (0..i64::from(BUCKETS) * BUCKET_S)
        .contains(&lead_s)
        .then_some((lead_s / BUCKET_S) as u8)
}

/// Sums of the errors of a set of forecast hours.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Errors {
    pub n: i64,
    pub sum_abs: f64,
    pub sum_err: f64,
    pub n_dir: i64,
    pub sum_dir: f64,
    pub hits: i64,
    pub misses: i64,
    pub false_alarms: i64,
}

/// The angle between two directions, 0–180°.
pub fn angle(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

impl Errors {
    pub fn add(&mut self, fc_wind: f32, fc_dir: Option<f32>, obs_wind: f32, obs_dir: Option<f32>) {
        let err = f64::from(fc_wind - obs_wind);
        self.n += 1;
        self.sum_abs += err.abs();
        self.sum_err += err;
        if obs_wind >= DIR_MIN_KN
            && let (Some(f), Some(o)) = (fc_dir, obs_dir)
        {
            self.n_dir += 1;
            self.sum_dir += f64::from(angle(f, o));
        }
        match (fc_wind >= WINDOW_KN, obs_wind >= WINDOW_KN) {
            (true, true) => self.hits += 1,
            (false, true) => self.misses += 1,
            (true, false) => self.false_alarms += 1,
            (false, false) => {}
        }
    }

    pub fn merge(&mut self, o: &Errors) {
        self.n += o.n;
        self.sum_abs += o.sum_abs;
        self.sum_err += o.sum_err;
        self.n_dir += o.n_dir;
        self.sum_dir += o.sum_dir;
        self.hits += o.hits;
        self.misses += o.misses;
        self.false_alarms += o.false_alarms;
    }

    pub fn mae(&self) -> Option<f64> {
        (self.n > 0).then(|| self.sum_abs / self.n as f64)
    }
}

/// Replaces a station's rows of one day.
pub async fn store_day(
    db: &SqlitePool,
    station_id: i64,
    day: u32,
    rows: &HashMap<(String, u8), Errors>,
) -> sqlx::Result<()> {
    let mut tx = db.begin().await?;
    sqlx::query("DELETE FROM verification_daily WHERE station_id = ? AND day = ?")
        .bind(station_id)
        .bind(day)
        .execute(&mut *tx)
        .await?;
    for ((model, bucket), e) in rows {
        sqlx::query(
            "INSERT INTO verification_daily (station_id, model_id, day, bucket, n, sum_abs, \
             sum_err, n_dir, sum_dir, hits, misses, false_alarms) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(station_id)
        .bind(model)
        .bind(day)
        .bind(bucket)
        .bind(e.n)
        .bind(e.sum_abs)
        .bind(e.sum_err)
        .bind(e.n_dir)
        .bind(e.sum_dir)
        .bind(e.hits)
        .bind(e.misses)
        .bind(e.false_alarms)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

/// A model's errors over a span of days in one lead-time bucket.
#[derive(Debug, Clone, PartialEq)]
pub struct Score {
    pub model: String,
    pub bucket: u8,
    /// Days with at least one compared hour
    pub days: i64,
    pub errors: Errors,
}

#[derive(sqlx::FromRow)]
struct ScoreRow {
    model_id: String,
    bucket: u8,
    days: i64,
    n: i64,
    sum_abs: f64,
    sum_err: f64,
    n_dir: i64,
    sum_dir: f64,
    hits: i64,
    misses: i64,
    false_alarms: i64,
}

/// Scores of a station from `from_day` to `to_day` (yyyymmdd, both included).
pub async fn scores(
    db: &SqlitePool,
    station_id: i64,
    from_day: u32,
    to_day: u32,
) -> sqlx::Result<Vec<Score>> {
    let rows: Vec<ScoreRow> = sqlx::query_as(
        "SELECT model_id, bucket, COUNT(*) AS days, SUM(n) AS n, SUM(sum_abs) AS sum_abs, \
         SUM(sum_err) AS sum_err, SUM(n_dir) AS n_dir, SUM(sum_dir) AS sum_dir, \
         SUM(hits) AS hits, SUM(misses) AS misses, SUM(false_alarms) AS false_alarms \
         FROM verification_daily WHERE station_id = ? AND day BETWEEN ? AND ? AND n > 0 \
         GROUP BY model_id, bucket",
    )
    .bind(station_id)
    .bind(from_day)
    .bind(to_day)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Score {
            model: r.model_id,
            bucket: r.bucket,
            days: r.days,
            errors: Errors {
                n: r.n,
                sum_abs: r.sum_abs,
                sum_err: r.sum_err,
                n_dir: r.n_dir,
                sum_dir: r.sum_dir,
                hits: r.hits,
                misses: r.misses,
                false_alarms: r.false_alarms,
            },
        })
        .collect())
}

/// The span the ranking at `now` looks at: the last 30 UTC days including today.
pub fn ranking_days(now: i64) -> (u32, u32) {
    (
        time::yyyymmdd(now - (RANKING_DAYS - 1) * 86_400),
        time::yyyymmdd(now),
    )
}

/// Mix weights per lead-time bucket: 1/MAE² of each model at the station once there are 14
/// days of data, else all models alike. A model without data in a bucket gets the smallest
/// weight there.
#[derive(Debug, Clone, Default)]
pub struct Weights {
    by_bucket: [HashMap<String, f64>; BUCKETS as usize],
}

impl Weights {
    pub fn from_scores(scores: &[Score]) -> Self {
        let mut w = Self::default();
        let enough = scores.iter().any(|s| s.days >= MIN_DAYS);
        if !enough {
            return w;
        }
        for s in scores {
            if s.model == TAF || s.model == MIX {
                continue;
            }
            if let Some(mae) = s.errors.mae() {
                w.by_bucket[usize::from(s.bucket)]
                    .insert(s.model.clone(), 1.0 / mae.max(0.5).powi(2));
            }
        }
        w
    }

    /// Equal weights (1) until the station has enough data.
    pub fn weight(&self, model: &str, bucket: u8) -> f64 {
        let m = &self.by_bucket[usize::from(bucket.min(BUCKETS - 1))];
        if m.is_empty() {
            return 1.0;
        }
        m.get(model)
            .copied()
            .unwrap_or_else(|| m.values().copied().fold(f64::INFINITY, f64::min))
    }
}

/// Drops rows of days before `before_day` (yyyymmdd).
pub async fn purge(db: &SqlitePool, before_day: u32) -> sqlx::Result<u64> {
    Ok(sqlx::query("DELETE FROM verification_daily WHERE day < ?")
        .bind(before_day)
        .execute(db)
        .await?
        .rows_affected())
}

/// One line of the ranking "who hits here?", over the last 30 days.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ModelScore {
    /// Model id, "taf" or "mix"
    pub model: String,
    /// Hours compared
    #[ts(type = "number")]
    pub n: i64,
    #[ts(type = "number")]
    pub days: i64,
    /// Mean absolute error of the mean wind, kn
    pub mae: f64,
    /// Mean error, kn; above 0 the forecast was too strong
    pub bias: f64,
    /// Mean angle between forecast and measured direction, degrees
    pub dir_error: Option<f64>,
    #[ts(type = "number")]
    pub hits: i64,
    #[ts(type = "number")]
    pub misses: i64,
    #[ts(type = "number")]
    pub false_alarms: i64,
}

impl ModelScore {
    pub fn new(model: &str, days: i64, e: &Errors) -> Option<Self> {
        Some(Self {
            model: model.to_owned(),
            n: e.n,
            days,
            mae: e.mae()?,
            bias: e.sum_err / e.n as f64,
            dir_error: (e.n_dir > 0).then(|| e.sum_dir / e.n_dir as f64),
            hits: e.hits,
            misses: e.misses,
            false_alarms: e.false_alarms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_and_buckets() {
        assert_eq!(bucket(0), Some(0));
        assert_eq!(bucket(86_399), Some(0));
        assert_eq!(bucket(86_400), Some(1));
        assert_eq!(bucket(3 * 86_400), None);
        assert_eq!(bucket(-1), None);
        assert_eq!(angle(350.0, 10.0), 20.0);
        assert_eq!(angle(90.0, 270.0), 180.0);

        let mut e = Errors::default();
        e.add(16.0, Some(220.0), 12.0, Some(240.0)); // false alarm, 20°
        e.add(10.0, Some(0.0), 15.0, Some(90.0)); // miss, 90°
        e.add(15.0, None, 15.0, Some(90.0)); // hit, no direction
        e.add(3.0, Some(0.0), 2.0, Some(180.0)); // too little wind for the direction
        assert_eq!(
            (e.n, e.n_dir, e.hits, e.misses, e.false_alarms),
            (4, 2, 1, 1, 1)
        );
        assert_eq!(e.mae(), Some(2.5));
        assert_eq!(e.sum_err, 0.0);
        assert_eq!(e.sum_dir, 110.0);
    }

    #[test]
    fn weights_wait_for_14_days() {
        let score = |model: &str, days: i64, mae: f64| Score {
            model: model.into(),
            bucket: 0,
            days,
            errors: Errors {
                n: 10,
                sum_abs: mae * 10.0,
                ..Errors::default()
            },
        };
        let few = Weights::from_scores(&[score("a", 5, 1.0), score("b", 5, 2.0)]);
        assert_eq!((few.weight("a", 0), few.weight("b", 0)), (1.0, 1.0));
        let w = Weights::from_scores(&[
            score("a", 20, 1.0),
            score("b", 20, 2.0),
            score(MIX, 20, 0.5),
        ]);
        assert_eq!((w.weight("a", 0), w.weight("b", 0)), (1.0, 0.25));
        // Unknown in this bucket: the smallest weight; other buckets without data: alike.
        assert_eq!(w.weight("c", 0), 0.25);
        assert_eq!(w.weight("a", 2), 1.0);
    }
}
