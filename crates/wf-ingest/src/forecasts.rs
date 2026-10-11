//! Keeps the forecast series of places current: on request, and after every model run for
//! the places people looked at recently.

use std::collections::HashMap;

use sqlx::SqlitePool;
use tokio::sync::broadcast;
use wf_core::forecast::{self, Forecast, ModelSeries, Notice, Run, SeriesData};
use wf_core::geo::GridPoint;
use wf_core::model::{Model, ModelClass};

use crate::openmeteo::{self, OpenMeteo};

/// Places requested within this time are refreshed after each run.
pub const HOT_FOR_S: i64 = 7 * 86_400;
/// Runs are kept this long (for recomputing the verification).
pub const KEEP_RUNS_S: i64 = 10 * 86_400;
/// Free points are dropped this long after the last request.
pub const KEEP_POINTS_S: i64 = 2 * 86_400;

pub struct Forecasts {
    db: SqlitePool,
    om: OpenMeteo,
    models: Vec<Model>,
    updates: broadcast::Sender<GridPoint>,
}

impl Forecasts {
    pub fn new(db: SqlitePool, om: OpenMeteo, models: Vec<Model>) -> Self {
        Self {
            db,
            om,
            models,
            updates: broadcast::channel(256).0,
        }
    }

    pub fn models(&self) -> &[Model] {
        &self.models
    }

    /// Announces every place whose series changed.
    pub fn subscribe(&self) -> broadcast::Receiver<GridPoint> {
        self.updates.subscribe()
    }

    /// Active models whose domain contains the point.
    pub fn covering(&self, p: GridPoint) -> Vec<&Model> {
        self.models
            .iter()
            .filter(|m| m.covers(p.lat(), p.lon()))
            .collect()
    }

    /// Initialisation time of the newest run Open-Meteo has out for a model: from its
    /// `meta.json`, else from the model's schedule.
    async fn newest_run_at(&self, m: &Model, now: i64) -> i64 {
        if let Some(meta) = &m.meta_id {
            match self.om.latest_run(meta).await {
                Ok(Some(t)) => return t,
                Ok(None) => {}
                Err(e) => tracing::debug!("{}: no run metadata ({e})", m.id),
            }
        }
        openmeteo::scheduled_run(now, m.runs_per_day, availability_delay(m))
    }

    /// The newest known run; before the scheduler has looked, the one the model's schedule
    /// suggests (no request, so a page does not wait for run metadata).
    async fn current_run(&self, m: &Model, now: i64) -> sqlx::Result<Run> {
        if let Some(run) = forecast::latest_run(&self.db, &m.id).await? {
            return Ok(run);
        }
        let at = openmeteo::scheduled_run(now, m.runs_per_day, availability_delay(m));
        Ok(forecast::add_run(&self.db, &m.id, at).await?.0)
    }

    /// Records new runs; returns how many models have one.
    pub async fn detect_runs(&self, now: i64) -> sqlx::Result<usize> {
        let mut new = 0;
        for m in &self.models {
            let at = self.newest_run_at(m, now).await;
            let known = forecast::latest_run(&self.db, &m.id).await?;
            if known.is_none_or(|r| at > r.run_at) {
                forecast::add_run(&self.db, &m.id, at).await?;
                tracing::info!("{}: new run {at}", m.id);
                new += 1;
            }
        }
        Ok(new)
    }

    /// Brings a place up to date before it is shown: fetches the models whose newest run is
    /// missing there. Returns why that failed, if it did.
    pub async fn ensure(&self, p: GridPoint, pinned: bool) -> anyhow::Result<Option<Notice>> {
        let now = wf_core::time::now();
        forecast::touch_place(&self.db, p, pinned, now).await?;
        let state = state_map(forecast::series_state(&self.db, p).await?);
        let mut wanted = Vec::new();
        for m in self.covering(p) {
            let run = self.current_run(m, now).await?;
            if needs(&state, m, &run) {
                wanted.push((m, run));
            }
        }
        if wanted.is_empty() {
            return Ok(None);
        }
        Ok(match self.fetch(p, &wanted, now, true).await {
            Ok(()) => None,
            Err(openmeteo::Error::Budget(_)) => Some(Notice::BudgetExhausted),
            Err(e) => {
                tracing::warn!("forecast for {:.2},{:.2}: {e}", p.lat(), p.lon());
                Some(Notice::SourceUnavailable)
            }
        })
    }

    async fn fetch(
        &self,
        p: GridPoint,
        wanted: &[(&Model, Run)],
        now: i64,
        interactive: bool,
    ) -> Result<(), openmeteo::Error> {
        let ids: Vec<&str> = wanted.iter().map(|(m, _)| m.id.as_str()).collect();
        let data = self.om.forecast(p, &ids, interactive).await?;
        let store = async {
            forecast::set_place_info(&self.db, p, data.timezone.as_deref(), data.elevation_m)
                .await?;
            let empty = SeriesData::default();
            for (m, run) in wanted {
                let series = data
                    .series
                    .iter()
                    .find(|(id, _)| *id == m.id)
                    .map_or(&empty, |(_, s)| s);
                forecast::store_series(&self.db, p, &m.id, run.id, series, now).await?;
            }
            Ok::<(), sqlx::Error>(())
        };
        if let Err(e) = store.await {
            tracing::error!("storing forecast: {e}");
            return Err(openmeteo::Error::Format(format!("storing: {e}")));
        }
        let _ = self.updates.send(p);
        Ok(())
    }

    /// After new runs: fetches them for the hot places, most recently requested first, then
    /// marks the runs complete. A run stays pending (and is retried) while the source fails.
    pub async fn refresh(&self, now: i64) -> anyhow::Result<()> {
        let mut pending = Vec::new();
        for m in &self.models {
            if let Some(run) = forecast::latest_run(&self.db, &m.id).await?
                && !run.complete
            {
                pending.push((m, run));
            }
        }
        if pending.is_empty() {
            return Ok(());
        }
        let mut retry = false;
        for p in forecast::hot_places(&self.db, now - HOT_FOR_S).await? {
            let state = state_map(forecast::series_state(&self.db, p).await?);
            let wanted: Vec<(&Model, Run)> = pending
                .iter()
                .filter(|(m, run)| m.covers(p.lat(), p.lon()) && needs(&state, m, run))
                .map(|(m, run)| (*m, *run))
                .collect();
            if wanted.is_empty() {
                continue;
            }
            match self.fetch(p, &wanted, now, false).await {
                Ok(()) => {}
                Err(openmeteo::Error::Budget(limit)) => {
                    tracing::warn!(
                        "Open-Meteo budget of {limit} calls used up; refresh stops for today"
                    );
                    break;
                }
                Err(e @ openmeteo::Error::Rejected(_)) => {
                    tracing::error!("refresh: {e}");
                }
                Err(e) => {
                    tracing::warn!("refresh: {e}");
                    retry = true;
                    break;
                }
            }
        }
        if !retry {
            for (_, run) in pending {
                forecast::complete_run(&self.db, run.id, now).await?;
            }
        }
        Ok(())
    }

    pub async fn purge(&self, now: i64) -> sqlx::Result<()> {
        let (runs, places) =
            forecast::purge(&self.db, now - KEEP_RUNS_S, now - KEEP_POINTS_S).await?;
        if runs + places > 0 {
            tracing::info!("purged {runs} old runs and {places} free points");
        }
        Ok(())
    }

    /// The forecast of a place as stored.
    pub async fn load(&self, p: GridPoint, notice: Option<Notice>) -> anyhow::Result<Forecast> {
        let info = forecast::place_info(&self.db, p).await?;
        let stored = forecast::latest_series(&self.db, p).await?;
        let empty: Vec<&str> = stored
            .iter()
            .filter(|s| s.data.is_empty())
            .map(|s| s.model_id.as_str())
            .collect();
        let models: Vec<Model> = self
            .covering(p)
            .into_iter()
            .filter(|m| !empty.contains(&m.id.as_str()))
            .cloned()
            .collect();
        let order = |id: &str| models.iter().position(|m| m.id == id);
        let mut series: Vec<ModelSeries> = stored
            .into_iter()
            .filter(|s| !s.data.is_empty() && order(&s.model_id).is_some())
            .map(ModelSeries::from)
            .collect();
        series.sort_by_key(|s| order(&s.model));
        Ok(Forecast {
            lat: p.lat(),
            lon: p.lon(),
            timezone: info.timezone,
            elevation_m: info.elevation_m,
            models,
            series,
            notice,
        })
    }
}

/// About how long after its start a run is out on Open-Meteo.
fn availability_delay(m: &Model) -> i64 {
    match m.class {
        ModelClass::Super => 2 * 3_600,
        ModelClass::Global => 7 * 3_600,
    }
}

fn state_map(rows: Vec<(String, i64, bool)>) -> HashMap<String, (i64, bool)> {
    rows.into_iter()
        .map(|(m, run_at, empty)| (m, (run_at, empty)))
        .collect()
}

/// Whether a place lacks a model's run. A model that had no values there is not asked again.
fn needs(state: &HashMap<String, (i64, bool)>, m: &Model, run: &Run) -> bool {
    match state.get(&m.id) {
        Some((_, true)) => false,
        Some((run_at, false)) => *run_at < run.run_at,
        None => true,
    }
}
