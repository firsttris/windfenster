//! The background loop: station lists weekly, model runs every five minutes, METARs every ten,
//! TAFs and DWD measurements every thirty, cleanup hourly.

use std::sync::Arc;
use std::time::Duration;

use sqlx::SqlitePool;
use wf_core::station;

use crate::forecasts::{Forecasts, HOT_FOR_S};
use crate::observations::Measurements;
use crate::stations::{self, Sources};

/// Measurements and TAFs are kept for 13 months: a full season to compare against.
pub const KEEP_OBSERVATIONS_S: i64 = 395 * 86_400;

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
                if let Err(e) =
                    wf_core::observation::purge(&self.db, now - KEEP_OBSERVATIONS_S).await
                {
                    tracing::error!("observation purge: {e}");
                }
                if let Err(e) = wf_core::taf::purge(&self.db, now - KEEP_OBSERVATIONS_S).await {
                    tracing::error!("TAF purge: {e}");
                }
            }
        }
    }
}
