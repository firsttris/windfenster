//! The background loop: station lists weekly, model runs every five minutes, METARs every ten,
//! water gauges every ten, TAFs and DWD measurements every thirty, verification and cleanup
//! hourly.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use sqlx::SqlitePool;
use wf_core::{station, verification};

use crate::forecasts::{Forecasts, HOT_FOR_S};
use crate::measurements::Measurements;
use crate::stations::{self, Sources};

/// Measurements and TAFs are kept for 13 months: a full season to compare against.
pub const KEEP_OBSERVATIONS_S: i64 = 395 * 86_400;
/// Stations nobody tracks, opened lately or saved keep 11 days: the history chart reaches 10
/// days back (wf-api `history.rs`).
pub const KEEP_OTHERS_S: i64 = 11 * 86_400;
/// A station opened this recently keeps its whole history.
pub const OPENED_WITHIN_S: i64 = 30 * 86_400;
/// The reports' text is kept for 2 days; it is only shown for the latest.
pub const KEEP_RAW_S: i64 = 2 * 86_400;

/// The longest a step may take before the loop gives it up and goes on: one source that hangs
/// (each request may take a minute, a step makes many) would otherwise hold up the others: no
/// METARs, no new runs, no live values for the alarms while the DWD does not answer.
const STATION_LISTS: Duration = Duration::from_secs(900);
const QUICK: Duration = Duration::from_secs(120);
const PER_STATION: Duration = Duration::from_secs(300);
const REFRESH: Duration = Duration::from_secs(600);

/// Runs `step` for at most `limit`; a step given up on is logged and left for its next turn.
async fn bounded<T>(name: &str, limit: Duration, step: impl Future<Output = T>) -> Option<T> {
    let out = tokio::time::timeout(limit, step).await.ok();
    if out.is_none() {
        tracing::warn!("{name}: gave up after {limit:?}; again next turn");
    }
    out
}

pub struct Scheduler {
    pub db: SqlitePool,
    pub http: reqwest::Client,
    pub forecasts: Arc<Forecasts>,
    pub measurements: Arc<Measurements>,
    pub sources: Sources,
    /// Slugs of the stations to keep current without visitors.
    pub tracked: Vec<String>,
}

impl Scheduler {
    pub async fn run(self) {
        let mut tick = tokio::time::interval(Duration::from_secs(300));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut next_import = 0;
        let mut next_metar = 0;
        let mut next_dwd = 0;
        let mut next_taf = 0;
        let mut next_gauges = 0;
        let mut next_purge = 0;
        let mut mark_tracked = true;
        loop {
            tick.tick().await;
            let now = wf_core::time::now();
            if now >= next_import {
                // Checked hourly; a list is fetched again when it is a week old.
                next_import = now + 3_600;
                match stations::due(&self.db, now).await {
                    Ok(true) => {
                        let import = stations::import_all(&self.db, &self.http, &self.sources);
                        if let Some(Err(e)) = bounded("station lists", STATION_LISTS, import).await
                        {
                            tracing::warn!("{e:#}");
                        }
                        mark_tracked = true;
                    }
                    Ok(false) => {}
                    Err(e) => tracing::error!("station import check: {e}"),
                }
            }
            if mark_tracked {
                // After every import: a station can come or go with a new list.
                match station::set_tracked(&self.db, &self.tracked).await {
                    Ok(unknown) if !unknown.is_empty() => {
                        tracing::warn!("tracked stations not found: {}", unknown.join(", "));
                    }
                    Ok(_) => {}
                    Err(e) => tracing::error!("tracked stations: {e}"),
                }
                mark_tracked = false;
            }
            if now >= next_metar {
                // Less a minute, so a tick that runs a little early does not skip a turn.
                next_metar = now + 540;
                match bounded("METARs", QUICK, self.measurements.import_metars()).await {
                    Some(Ok(n)) => tracing::debug!("stored {n} METARs"),
                    Some(Err(e)) => tracing::warn!("METAR import: {e:#}"),
                    None => {}
                }
            }
            if now >= next_dwd {
                next_dwd = now + 1_740;
                let dwd = self.measurements.import_dwd(now - HOT_FOR_S);
                match bounded("DWD measurements", PER_STATION, dwd).await {
                    Some(Ok(n)) => tracing::debug!("stored {n} DWD measurements"),
                    Some(Err(e)) => tracing::warn!("DWD measurements: {e:#}"),
                    None => {}
                }
            }
            if now >= next_taf {
                next_taf = now + 1_740;
                let tafs = self.measurements.import_tafs(now - HOT_FOR_S);
                match bounded("TAFs", PER_STATION, tafs).await {
                    Some(Ok(n)) => tracing::debug!("stored {n} TAFs"),
                    Some(Err(e)) => tracing::warn!("TAFs: {e:#}"),
                    None => {}
                }
            }
            if now >= next_gauges {
                // PEGELONLINE publishes every 15 minutes; every other tick.
                next_gauges = now + 540;
                match bounded("gauges", QUICK, self.measurements.import_gauges()).await {
                    Some(Ok(n)) => tracing::debug!("stored {n} gauges"),
                    Some(Err(e)) => tracing::warn!("gauges: {e:#}"),
                    None => {}
                }
            }
            let runs = self.forecasts.detect_runs(now);
            if let Some(Err(e)) = bounded("run detection", QUICK, runs).await {
                tracing::error!("run detection: {e}");
            }
            let refresh = self.forecasts.refresh(now);
            if let Some(Err(e)) = bounded("refresh", REFRESH, refresh).await {
                tracing::error!("refresh: {e:#}");
            }
            if now >= next_purge {
                next_purge = now + 3_600;
                if let Err(e) = self.forecasts.purge(now).await {
                    tracing::error!("purge: {e}");
                }
                let keep = wf_core::observation::Keep {
                    all: now - KEEP_OTHERS_S,
                    opened_since: now - OPENED_WITHIN_S,
                    kept: now - KEEP_OBSERVATIONS_S,
                    raw: now - KEEP_RAW_S,
                };
                match wf_core::observation::purge(&self.db, keep).await {
                    Ok(n) if n > 0 => tracing::info!("dropped {n} old measurements"),
                    Ok(_) => {}
                    Err(e) => tracing::error!("observation purge: {e}"),
                }
                if let Err(e) = wf_core::taf::purge(&self.db, keep).await {
                    tracing::error!("TAF purge: {e}");
                }
                let keep_from = wf_core::time::yyyymmdd(now - KEEP_OBSERVATIONS_S);
                if let Err(e) = verification::purge(&self.db, keep_from).await {
                    tracing::error!("verification purge: {e}");
                }
                // Sessions and links that ran out, else gone only at someone's next sign-in
                if let Err(e) = wf_core::account::prune_sessions(&self.db, now).await {
                    tracing::error!("session purge: {e}");
                }
                if let Err(e) = wf_core::account::prune_email_confirmations(&self.db, now).await {
                    tracing::error!("confirmation link purge: {e}");
                }
                let usage_from = wf_core::usage::day(now - KEEP_OBSERVATIONS_S);
                if let Err(e) = wf_core::usage::purge(&self.db, usage_from).await {
                    tracing::error!("usage purge: {e}");
                }
                // Today so far, and the two days before for measurements that came late.
                let today = now - now.rem_euclid(86_400);
                for day0 in [today - 2 * 86_400, today - 86_400, today] {
                    match verification::verify_day(&self.db, day0).await {
                        Ok(n) => tracing::debug!(
                            "verified {n} stations for {}",
                            wf_core::time::yyyymmdd(day0)
                        ),
                        Err(e) => tracing::error!("verification: {e:#}"),
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn gives_up_a_step_that_hangs_and_keeps_one_that_answers() {
        let hangs = std::future::pending::<u32>();
        let short = Duration::from_millis(20);
        assert_eq!(bounded("hangs", short, hangs).await, None);
        assert_eq!(bounded("answers", QUICK, async { 7 }).await, Some(7));
    }
}
