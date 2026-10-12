//! The background loop: station lists weekly, model runs every five minutes, METARs every ten,
//! water gauges every ten, TAFs and DWD measurements every thirty, verification and cleanup
//! hourly.

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
                        if let Err(e) =
                            stations::import_all(&self.db, &self.http, &self.sources).await
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
                match self.measurements.import_metars().await {
                    Ok(n) => tracing::debug!("stored {n} METARs"),
                    Err(e) => tracing::warn!("METAR import: {e:#}"),
                }
            }
            if now >= next_dwd {
                next_dwd = now + 1_740;
                match self.measurements.import_dwd(now - HOT_FOR_S).await {
                    Ok(n) => tracing::debug!("stored {n} DWD measurements"),
                    Err(e) => tracing::warn!("DWD measurements: {e:#}"),
                }
            }
            if now >= next_taf {
                next_taf = now + 1_740;
                match self.measurements.import_tafs(now - HOT_FOR_S).await {
                    Ok(n) => tracing::debug!("stored {n} TAFs"),
                    Err(e) => tracing::warn!("TAFs: {e:#}"),
                }
            }
            if now >= next_gauges {
                // PEGELONLINE publishes every 15 minutes; every other tick.
                next_gauges = now + 540;
                match self.measurements.import_gauges().await {
                    Ok(n) => tracing::debug!("stored {n} gauges"),
                    Err(e) => tracing::warn!("gauges: {e:#}"),
                }
            }
            if let Err(e) = self.forecasts.detect_runs(now).await {
                tracing::error!("run detection: {e}");
            }
            if let Err(e) = self.forecasts.refresh(now).await {
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
