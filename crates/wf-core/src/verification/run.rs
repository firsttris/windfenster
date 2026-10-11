//! The hourly job: every forecast hour with a measurement, per station, model and day.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use sqlx::SqlitePool;

use super::{Errors, MIX, RANKING_DAYS, TAF, Weights, bucket, scores, store_day};
use crate::forecast::{self, LoadError, StoredSeries};
use crate::geo::GridPoint;
use crate::observation::{self, MATCH_S};
use crate::{mix, taf, time};

/// Verifies one UTC day (starting at `day0`) for every station with measurements and stored
/// forecasts, replacing that day's rows; returns how many stations were verified.
pub async fn verify_day(db: &SqlitePool, day0: i64) -> Result<usize, LoadError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forecast::SeriesData;
    use crate::observation::Observation;
    use crate::station::{NewStation, Station, StationSource};

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
