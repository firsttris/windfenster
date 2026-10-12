//! One alarm, one round: the forecast windows of today and tomorrow against what was told, and
//! the station's latest measurements.

use chrono_tz::Tz;
use wf_core::alarm::{self, Hour, Job, Kind, Notice, Rule, Window, window};
use wf_core::gear::{self, GearItem};
use wf_core::{observation, station};
use wf_map::index::MapIndex;
use wf_map::series::MapSeries;

use crate::text::{Ctx, Lang};
use crate::{Alarms, Error};

/// Windows starting within this are told.
const AHEAD_S: i64 = 48 * 3_600;
/// Messages go out between these local hours only.
const AWAKE: std::ops::Range<u32> = 7..21;
/// A measurement older than this is not "now".
const FRESH_S: i64 = 45 * 60;
/// The measurement before it, within this, must have had the wind too: one gust is no alarm.
const STEADY_S: i64 = 75 * 60;

pub fn yyyymmdd(at: i64) -> i64 {
    Ctx {
        lang: Lang::De,
        tz: Tz::UTC,
        unit: Default::default(),
        spot: "",
    }
    .day(at)
}

pub async fn job(a: &Alarms, index: Option<&MapIndex>, job: &Job, now: i64) -> Result<(), Error> {
    let ctx = Ctx {
        lang: Lang::of(&job.locale),
        tz: job.time_zone.parse().unwrap_or(Tz::Europe__Berlin),
        unit: job.unit,
        spot: &job.favorite.name,
    };
    if !AWAKE.contains(&ctx.hour(now)) {
        return Ok(());
    }
    let gear = if job.settings.gear {
        Some(gear::list(&a.db, job.account_id).await?).filter(|g| !g.is_empty())
    } else {
        None
    };
    let f = &job.favorite;
    let rule = Rule {
        min_kn: job.settings.min_kn,
        sector: f.sector,
        gear: gear.as_deref(),
        lat: f.lat,
        lon: f.lon,
    };
    if job.settings.forecast
        && let (Some(map), Some(index)) = (&a.map, index)
    {
        let series = wf_map::series::series(map.root(), index, f.lat, f.lon).await;
        forecast(a, job, &ctx, &rule, &hours(&series), now).await?;
    }
    if job.settings.live
        && let Some(slug) = &f.slug
    {
        live(a, job, &ctx, &rule, slug, now).await?;
    }
    Ok(())
}

/// The spot's hours as the app shows them (`web/src/lib/spot-series.ts`): each model while it
/// has hours there, the finer before the coarser (ICON-D2, ICON global, ECMWF).
pub fn hours(series: &MapSeries) -> Vec<Hour> {
    let mut covered: Vec<(i64, i64)> = Vec::new();
    let mut out: Vec<Hour> = Vec::new();
    for model in wf_map::MODELS {
        let Some(layer) = series.layers.iter().find(|l| l.model == *model) else {
            continue;
        };
        let (Some(first), Some(last)) = (layer.points.first(), layer.points.last()) else {
            continue;
        };
        out.extend(
            layer
                .points
                .iter()
                .filter(|p| !covered.iter().any(|&(a, b)| (a..=b).contains(&p.valid_at)))
                .map(|p| Hour {
                    at: p.valid_at,
                    wind: f64::from(p.wind),
                    dir: f64::from(p.dir),
                }),
        );
        covered.push((first.valid_at, last.valid_at));
    }
    out.sort_by_key(|h| h.at);
    out
}

/// The best window of each of today and tomorrow, against what was told about that day.
async fn forecast(
    a: &Alarms,
    job: &Job,
    ctx: &Ctx<'_>,
    rule: &Rule<'_>,
    hours: &[Hour],
    now: i64,
) -> Result<(), Error> {
    let windows: Vec<Window> = window::windows(hours, rule)
        .into_iter()
        .filter(|w| w.start > now && w.start <= now + AHEAD_S)
        .collect();
    let today = ctx.day(now);
    let tomorrow = ctx.day(now + 86_400);
    for day in [today, tomorrow] {
        let best = windows
            .iter()
            .filter(|w| ctx.day(w.start) == day)
            .max_by(|x, y| x.peak_kn.total_cmp(&y.peak_kn))
            .copied();
        let told = alarm::told(&a.db, job.favorite.id, day, Kind::Window).await?;
        let Some(notice) = window::decide(told, best, now) else {
            continue;
        };
        let shown = match notice {
            Notice::New(w) | Notice::Changed { now: w, .. } => Some(w),
            Notice::Gone(_) => None,
        };
        let piece = shown.and_then(|w| fitting(rule, w.peak_kn));
        let text = ctx.notice(&notice, piece.as_ref(), now);
        let tag = format!("alarm-{}-{day}", job.favorite.id);
        if a.deliver(job, &text, &tag).await {
            alarm::tell(
                &a.db,
                job.favorite.id,
                day,
                Kind::Window,
                shown.as_ref(),
                now,
            )
            .await?;
        }
    }
    Ok(())
}

/// The station measures enough wind now, and did half an hour or so before: once a day.
async fn live(
    a: &Alarms,
    job: &Job,
    ctx: &Ctx<'_>,
    rule: &Rule<'_>,
    slug: &str,
    now: i64,
) -> Result<(), Error> {
    let day = ctx.day(now);
    if alarm::told(&a.db, job.favorite.id, day, Kind::Live)
        .await?
        .is_some()
    {
        return Ok(());
    }
    let Some(id) = station::id_of(&a.db, slug).await? else {
        return Ok(());
    };
    let measured: Vec<_> = observation::since(&a.db, id, now - FRESH_S - STEADY_S)
        .await?
        .into_iter()
        .filter_map(|o| {
            Some((
                o.at,
                f64::from(o.wind?),
                o.gust.map(f64::from),
                o.dir.map(f64::from),
            ))
        })
        .collect();
    let [.., before, last] = measured.as_slice() else {
        return Ok(());
    };
    // A variable direction meets no sector.
    let counts = |&(at, wind, _, dir): &(i64, f64, Option<f64>, Option<f64>)| {
        (rule.sector.is_none() || dir.is_some()) && rule.counts(at, wind, dir.unwrap_or(0.0))
    };
    if last.0 < now - FRESH_S || last.0 - before.0 > STEADY_S || !counts(last) || !counts(before) {
        return Ok(());
    }
    let text = ctx.live(last.1, last.2, last.3, fitting(rule, last.1).as_ref());
    if a.deliver(job, &text, &format!("live-{}-{day}", job.favorite.id))
        .await
    {
        alarm::tell(&a.db, job.favorite.id, day, Kind::Live, None, now).await?;
    }
    Ok(())
}

/// The piece for a wind, when the alarm goes by gear
fn fitting(rule: &Rule, wind: f64) -> Option<GearItem> {
    rule.gear.and_then(|g| gear::best_for(g, wind)).copied()
}

#[cfg(test)]
mod tests {
    use wf_map::series::{SeriesLayer, SeriesPoint};

    use super::*;

    fn layer(model: &str, hours: impl Iterator<Item = i64>) -> SeriesLayer {
        SeriesLayer {
            model: model.into(),
            points: hours
                .map(|h| SeriesPoint {
                    valid_at: h * 3_600,
                    run_at: 0,
                    wind: model.len() as f32,
                    gust: None,
                    dir: 0.0,
                })
                .collect(),
        }
    }

    #[test]
    fn the_finest_model_while_it_lasts() {
        let series = MapSeries {
            layers: vec![
                layer("icon_d2", 2..=4),
                layer("icon_global", 0..=8),
                layer("ecmwf_ifs025", (0..=15).step_by(3)),
            ],
        };
        let got: Vec<(i64, usize)> = hours(&series)
            .iter()
            .map(|h| (h.at / 3_600, h.wind as usize))
            .collect();
        // icon_d2 (7) at 2–4, icon_global (11) around it to 8, ECMWF (12) after
        assert_eq!(
            got,
            [
                (0, 11),
                (1, 11),
                (2, 7),
                (3, 7),
                (4, 7),
                (5, 11),
                (6, 11),
                (7, 11),
                (8, 11),
                (9, 12),
                (12, 12),
                (15, 12)
            ]
        );
    }
}
