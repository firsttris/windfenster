//! How well each model, the TAF and the mix hit at a station: errors per hour with a
//! measurement, summed per UTC day and lead time, and the scores over 30 days that pick the
//! model and weight the mix.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use serde::Serialize;
use sqlx::SqlitePool;
use ts_rs::TS;

use crate::forecast::{self, StoredSeries};
use crate::geo::GridPoint;
use crate::observation::{self, MATCH_S};
use crate::{mix, taf, time};

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

/// Verifies one UTC day (starting at `day0`) for every station with measurements and stored
/// forecasts, replacing that day's rows; returns how many stations were verified.
pub async fn verify_day(db: &SqlitePool, day0: i64) -> anyhow::Result<usize> {
    let day1 = day0 + 86_400;
    let day = time::yyyymmdd(day0);
    let stations: Vec<(i64, f64, f64)> = sqlx::query_as(
        "SELECT DISTINCT s.id, s.lat, s.lon FROM observation o JOIN station s ON s.id = o.station_id \
         WHERE o.observed_at >= ? AND o.observed_at < ? AND o.wind_kn IS NOT NULL",
    )
    .bind(day0 - MATCH_S)
    .bind(day1 + MATCH_S)
    .fetch_all(db)
    .await?;
    let mut verified = 0;
    for (id, lat, lon) in stations {
        let Some(p) = GridPoint::new(lat, lon) else {
            continue;
        };
        let series = forecast::series_between(db, p, day0 - 72 * 3_600, day1).await?;
        let tafs = taf::between(db, id, day0, day1).await?;
        if series.is_empty() && tafs.is_empty() {
            continue;
        }
        let obs: Vec<_> = observation::since(db, id, day0 - MATCH_S)
            .await?
            .into_iter()
            .filter(|o| o.at <= day1 + MATCH_S)
            .collect();
        let hours: Vec<(i64, f32, Option<f32>)> = (day0..day1)
            .step_by(3_600)
            .filter_map(|t| {
                let o = observation::nearest(&obs, t, MATCH_S)?;
                Some((t, o.wind?, o.dir))
            })
            .collect();
        if hours.is_empty() {
            continue;
        }
        let mut rows: HashMap<(String, u8), Errors> = HashMap::new();
        let mut add =
            |model: &str, b: u8, fc: f32, fc_dir: Option<f32>, ow: f32, od: Option<f32>| {
                rows.entry((model.to_owned(), b))
                    .or_default()
                    .add(fc, fc_dir, ow, od);
            };
        // Only hours after a run was fetched: what someone could have seen beforehand.
        for s in &series {
            for &(t, ow, od) in hours.iter().filter(|h| h.0 >= s.fetched_at) {
                if let (Some(b), Some(i)) = (bucket(t - s.run_at), s.data.index(t))
                    && let Some(fc) = s.data.wind[i]
                {
                    add(&s.model_id, b, fc, s.data.dir[i], ow, od);
                }
            }
        }
        for t in &tafs {
            for &(at, ow, od) in &hours {
                if let Some(b) = bucket(at - t.issued_at)
                    && let Some(h) = t.hours.iter().find(|h| h.at == at)
                    && let Some(fc) = h.wind
                {
                    add(TAF, b, fc, h.dir, ow, od);
                }
            }
        }
        verify_mix(db, id, &series, &hours, &mut add).await?;
        store_day(db, id, day, &rows).await?;
        verified += 1;
    }
    Ok(verified)
}

/// The mix as it stood whenever new runs came in: the newest fetched run of every model,
/// weighted with the scores of the 30 days before that day. Lead times count from the newest
/// run in it, like a model's from its run.
async fn verify_mix(
    db: &SqlitePool,
    station_id: i64,
    series: &[StoredSeries],
    hours: &[(i64, f32, Option<f32>)],
    add: &mut impl FnMut(&str, u8, f32, Option<f32>, f32, Option<f32>),
) -> sqlx::Result<()> {
    let mut issues: Vec<i64> = series.iter().map(|s| s.fetched_at).collect();
    issues.sort_unstable();
    issues.dedup();
    let mut weights: HashMap<u32, Weights> = HashMap::new();
    for &issued in &issues {
        let day = time::yyyymmdd(issued);
        if let Entry::Vacant(e) = weights.entry(day) {
            let scores = scores(
                db,
                station_id,
                time::yyyymmdd(issued - RANKING_DAYS * 86_400),
                time::yyyymmdd(issued - 86_400),
            )
            .await?;
            e.insert(Weights::from_scores(&scores));
        }
        let w = &weights[&day];
        let mut newest: HashMap<&str, &StoredSeries> = HashMap::new();
        for s in series.iter().filter(|s| s.fetched_at <= issued) {
            if newest
                .get(s.model_id.as_str())
                .is_none_or(|n| n.run_at < s.run_at)
            {
                newest.insert(&s.model_id, s);
            }
        }
        let Some(run_at) = newest.values().map(|s| s.run_at).max() else {
            continue;
        };
        let inputs: Vec<(&str, &forecast::SeriesData)> =
            newest.iter().map(|(m, s)| (*m, &s.data)).collect();
        // Like a model's runs, every state of the mix counts for the hours after it.
        for &(t, ow, od) in hours.iter().filter(|h| h.0 >= issued) {
            let Some(b) = bucket(t - run_at) else {
                continue;
            };
            let v = mix::at(&inputs, t, |m| w.weight(m, b));
            if let Some(fc) = v.wind {
                add(MIX, b, fc, v.dir, ow, od);
            }
        }
    }
    Ok(())
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
    use crate::forecast::SeriesData;
    use crate::observation::Observation;
    use crate::station::{NewStation, Station, StationSource};

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

    #[tokio::test]
    async fn verifies_a_day() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db = crate::db::connect(&dir.path().join("t.db")).await?;
        crate::db::migrate(&db).await?;
        crate::station::upsert(
            &db,
            &[NewStation {
                station: Station {
                    slug: "edsb".into(),
                    source: StationSource::Metar,
                    code: "EDSB".into(),
                    name: "Baden".into(),
                    country: None,
                    region: None,
                    municipality: None,
                    lat: 48.7794,
                    lon: 8.0805,
                    elevation_m: None,
                    has_taf: false,
                },
                keywords: None,
            }],
            1,
        )
        .await?;
        let id = crate::station::id_of(&db, "edsb").await?.unwrap();
        // 2026-10-11 00 UTC
        let day0 = 1_791_676_800;
        // Measured 10 kn from 230° at 08:50 and 09:50
        let obs = |at: i64| Observation {
            at,
            wind: Some(10.0),
            gust: None,
            dir: Some(230.0),
            temp: None,
            raw: None,
        };
        observation::store(
            &db,
            &[
                (id, obs(day0 + 8 * 3_600 + 3_000)),
                (id, obs(day0 + 9 * 3_600 + 3_000)),
            ],
        )
        .await?;
        // Two models from the 00 UTC run: a says 12 kn, b says 7 kn, for 24 hours
        let p = GridPoint::new(48.7794, 8.0805).unwrap();
        for (model, wind) in [("icon_d2", 12.0), ("ecmwf_ifs025", 7.0)] {
            let (run, _) = forecast::add_run(&db, model, day0, false).await?;
            let data = SeriesData {
                start: day0,
                step_s: 3_600,
                wind: vec![Some(wind); 24],
                gust: vec![None; 24],
                dir: vec![Some(240.0); 24],
                temp: vec![None; 24],
                rain: vec![None; 24],
                cloud: vec![None; 24],
            };
            forecast::store_series(&db, p, model, run.id, &data, day0).await?;
        }
        assert_eq!(verify_day(&db, day0).await?, 1);
        let s = scores(&db, id, 20261011, 20261011).await?;
        let mae = |m: &str| {
            s.iter()
                .find(|x| x.model == m && x.bucket == 0)
                .and_then(|x| x.errors.mae())
        };
        // 09 and 10 UTC are matched (08:50, 09:50), so two hours each
        assert_eq!(s.iter().find(|x| x.model == "icon_d2").unwrap().errors.n, 2);
        assert_eq!(mae("icon_d2"), Some(2.0));
        assert_eq!(mae("ecmwf_ifs025"), Some(3.0));
        // Mix with equal weights: 9.5 kn, 0.5 off
        assert_eq!(mae(MIX), Some(0.5));
        // Again: the day's rows are replaced, not added
        verify_day(&db, day0).await?;
        assert_eq!(scores(&db, id, 20261011, 20261011).await?, s);
        Ok(())
    }
}
