//! The background loop: station lists weekly, model runs every five minutes, cleanup hourly.

use std::sync::Arc;
use std::time::Duration;

use sqlx::SqlitePool;

use crate::forecasts::Forecasts;
use crate::stations::{self, Sources};

pub struct Scheduler {
    pub db: SqlitePool,
    pub http: reqwest::Client,
    pub forecasts: Arc<Forecasts>,
    pub sources: Sources,
}

impl Scheduler {
    pub async fn run(self) {
        let mut tick = tokio::time::interval(Duration::from_secs(300));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut next_import = 0;
        let mut next_purge = 0;
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
                    }
                    Ok(false) => {}
                    Err(e) => tracing::error!("station import check: {e}"),
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
            }
        }
    }
}
