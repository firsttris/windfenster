//! The background loop: station lists weekly, model runs every five minutes, METARs every ten,
//! water gauges every ten, TAFs and DWD measurements every thirty, verification and cleanup
//! hourly.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use sqlx::SqlitePool;
use wf_core::{station, status, verification};

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
    pub tracked: Vec<String>,
}

/// The tasks this loop runs, as the admin page names them, with how often they run.
pub const TASKS: [(&str, i64); 8] = [
    ("stations", 7 * 86_400),
    ("metar", 540),
    ("dwd", 1_740),
    ("taf", 1_740),
    ("gauges", 540),
    ("runs", 300),
    ("refresh", 300),
    ("purge", 3_600),
];

/// Runs a step for at most `limit` and reports how it went on the status board, and its
/// source's state; returns its result, None when it was given up.
async fn step<T, E: std::fmt::Display>(
    task: &'static str,
    source: Option<&'static str>,
    limit: Duration,
    run: impl Future<Output = Result<T, E>>,
    count: impl Fn(&T) -> Option<i64>,
) -> Option<Result<T, E>> {
    let board = status::board();
    let report = board.start(task);
    let out = bounded(task, limit, run).await;
    match &out {
        Some(Ok(v)) => {
            report.ok(count(v));
            if let Some(s) = source {
                board.source_ok(s);
            }
        }
        Some(Err(e)) => {
            report.failed(format!("{e:#}"));
            if let Some(s) = source {
                board.source_failed(s, format!("{e:#}"));
            }
        }
        None => {
            report.failed(format!("gave up after {limit:?}"));
            if let Some(s) = source {
                board.source_failed(s, "no answer");
            }
        }
    }
    out
}

/// When each task is next due; 0 runs it on the next turn.
#[derive(Default)]
struct Due {
    stations: i64,
    /// The station lists were asked for on the admin page: fetch them even if not a week old
    force_stations: bool,
    metar: i64,
    dwd: i64,
    taf: i64,
    gauges: i64,
    purge: i64,
}

impl Due {
    /// Tasks asked to run now on the admin page run on this turn.
    fn take_asked(&mut self) {
        let board = status::board();
        if board.take("stations") {
            self.stations = 0;
            self.force_stations = true;
        }
        for (task, due) in [
            ("metar", &mut self.metar),
            ("dwd", &mut self.dwd),
            ("taf", &mut self.taf),
            ("gauges", &mut self.gauges),
            ("purge", &mut self.purge),
        ] {
            if board.take(task) {
                *due = 0;
            }
        }

        // Runs and refresh go every turn anyway
        board.take("runs");
        board.take("refresh");
    }
}

impl Scheduler {
    pub async fn run(self) {
        for (task, every) in TASKS {
            status::board().register(task, every);
        }
        let mut tick = tokio::time::interval(Duration::from_secs(300));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut due = Due::default();
        let mut mark_tracked = true;
        loop {
            tokio::select! {
                _ = tick.tick() => {}
                () = status::board().asked() => {}
            }
            due.take_asked();
            let now = wf_core::time::now();
            if now >= due.stations {
                // Checked hourly; a list is fetched again when it is a week old (or when asked).
                let go =
                    std::mem::take(&mut due.force_stations) || self.station_lists_due(now).await;
                due.stations = now + 3_600;
                if go {
                    self.import_station_lists().await;
                    mark_tracked = true;
                }
            }
            if mark_tracked {
                self.mark_tracked().await;
                mark_tracked = false;
            }
            self.measurements(&mut due, now).await;
            self.forecasts(now).await;
            if now >= due.purge {
                due.purge = now + 3_600;
                self.purge(now).await;
            }
        }
    }

    /// Whether the station lists are due: a week old.
    async fn station_lists_due(&self, now: i64) -> bool {
        match stations::due(&self.db, now).await {
            Ok(due) => due,
            Err(e) => {
                tracing::error!("station import check: {e}");
                false
            }
        }
    }

    async fn import_station_lists(&self) {
        let import = stations::import_all(&self.db, &self.http, &self.sources);
        if let Some(Err(e)) = step(
            "stations",
            Some("ourairports"),
            STATION_LISTS,
            import,
            |_| None,
        )
        .await
        {
            tracing::warn!("{e:#}");
        }
    }

    /// After every import: a station can come or go with a new list.
    async fn mark_tracked(&self) {
        match station::set_tracked(&self.db, &self.tracked).await {
            Ok(unknown) if !unknown.is_empty() => {
                tracing::warn!("tracked stations not found: {}", unknown.join(", "));
            }
            Ok(_) => {}
            Err(e) => tracing::error!("tracked stations: {e}"),
        }
    }

    async fn measurements(&self, due: &mut Due, now: i64) {
        let count = |n: &usize| Some(*n as i64);
        if now >= due.metar {
            // Less a minute, so a tick that runs a little early does not skip a turn.
            due.metar = now + 540;
            let metars = self.measurements.import_metars();
            if let Some(Err(e)) = step("metar", Some("awc"), QUICK, metars, count).await {
                tracing::warn!("METAR import: {e:#}");
            }
        }
        if now >= due.dwd {
            due.dwd = now + 1_740;
            let dwd = self.measurements.import_dwd(now - HOT_FOR_S);
            if let Some(Err(e)) = step("dwd", Some("dwd"), PER_STATION, dwd, count).await {
                tracing::warn!("DWD measurements: {e:#}");
            }
        }
        if now >= due.taf {
            due.taf = now + 1_740;
            let tafs = self.measurements.import_tafs(now - HOT_FOR_S);
            if let Some(Err(e)) = step("taf", Some("awc"), PER_STATION, tafs, count).await {
                tracing::warn!("TAFs: {e:#}");
            }
        }
        if now >= due.gauges {
            // PEGELONLINE publishes every 15 minutes; every other tick.
            due.gauges = now + 540;
            let gauges = self.measurements.import_gauges();
            if let Some(Err(e)) = step("gauges", Some("pegelonline"), QUICK, gauges, count).await {
                tracing::warn!("gauges: {e:#}");
            }
        }
    }

    async fn forecasts(&self, now: i64) {
        let runs = self.forecasts.detect_runs(now);
        if let Some(Err(e)) = step("runs", None, QUICK, runs, |n| Some(*n as i64)).await {
            tracing::error!("run detection: {e}");
        }
        let refresh = self.forecasts.refresh(now);
        if let Some(Err(e)) = step("refresh", None, REFRESH, refresh, |n| Some(*n as i64)).await {
            tracing::error!("refresh: {e:#}");
        }
    }

    /// Hourly: old forecasts, measurements, TAFs, counters and sessions go; the last three days
    /// are verified.
    async fn purge(&self, now: i64) {
        let report = status::board().start("purge");
        let mut failed = Vec::new();
        if let Err(e) = self.forecasts.purge(now).await {
            failed.push(format!("forecasts: {e}"));
        }
        let keep = wf_core::observation::Keep {
            all: now - KEEP_OTHERS_S,
            opened_since: now - OPENED_WITHIN_S,
            kept: now - KEEP_OBSERVATIONS_S,
            raw: now - KEEP_RAW_S,
        };
        let mut dropped = 0;
        match wf_core::observation::purge(&self.db, keep).await {
            Ok(n) => {
                dropped = n;
                if n > 0 {
                    tracing::info!("dropped {n} old measurements");
                }
            }
            Err(e) => failed.push(format!("measurements: {e}")),
        }
        if let Err(e) = wf_core::taf::purge(&self.db, keep).await {
            failed.push(format!("TAFs: {e}"));
        }
        let keep_from = wf_core::time::yyyymmdd(now - KEEP_OBSERVATIONS_S);
        if let Err(e) = verification::purge(&self.db, keep_from).await {
            failed.push(format!("verification: {e}"));
        }
        // Sessions and links that ran out, else gone only at someone's next sign-in
        if let Err(e) = wf_core::account::prune_sessions(&self.db, now).await {
            failed.push(format!("sessions: {e}"));
        }
        if let Err(e) = wf_core::account::prune_email_confirmations(&self.db, now).await {
            failed.push(format!("confirmation links: {e}"));
        }
        let usage_from = wf_core::usage::day(now - KEEP_OBSERVATIONS_S);
        if let Err(e) = wf_core::usage::purge(&self.db, usage_from).await {
            failed.push(format!("usage: {e}"));
        }
        if let Err(e) = wf_core::alarm::delivery::forget(&self.db, now - DELIVERIES_S).await {
            failed.push(format!("alarm deliveries: {e}"));
        }
        // Today so far, and the two days before for measurements that came late.
        let today = now - now.rem_euclid(86_400);
        for day0 in [today - 2 * 86_400, today - 86_400, today] {
            match verification::verify_day(&self.db, day0).await {
                Ok(n) => tracing::debug!(
                    "verified {n} stations for {}",
                    wf_core::time::yyyymmdd(day0)
                ),
                Err(e) => failed.push(format!("verification: {e:#}")),
            }
        }
        if failed.is_empty() {
            report.ok(Some(dropped as i64));
        } else {
            for f in &failed {
                tracing::error!("purge: {f}");
            }
            report.partly(Some(dropped as i64), failed.join("; "));
        }
    }
}

/// Alarm deliveries are kept a month for the admin page.
const DELIVERIES_S: i64 = 30 * 86_400;

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
