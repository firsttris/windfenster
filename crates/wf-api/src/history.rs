//! A station's measurements over a day, as Windfinder shows them: today and the days before.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use wf_core::observation::{self, Observation};
use wf_core::station::{self, StationSource};
use wf_ingest::scheduler::KEEP_OBSERVATIONS_S;

use crate::{ApiError, AppState};

/// A request covers at most this much: a day, with room for a 25-hour day and margins.
const MAX_SPAN_S: i64 = 2 * 86_400;

/// The page offers today and the ten days before; the METAR API reaches back about eight.
const METAR_BACK_S: i64 = 10 * 86_400;

#[derive(Deserialize)]
pub struct HistoryQuery {
    from: i64,
    to: i64,
}

/// `GET /api/stations/{slug}/history?from=&to=`
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StationHistory {
    /// Measurements in [from, to), oldest first
    pub observations: Vec<Observation>,
    /// The oldest stored measurement of the station, so the page knows how far back to offer
    #[ts(type = "number | null")]
    pub oldest: Option<i64>,
}

/// Whether past days of a station are too thin and the source may still have them: the METAR
/// API for the last ten days, the DWD's archive up to yesterday.
fn wants_backfill(source: StationSource, from: i64, to: i64, count: usize, now: i64) -> bool {
    let today = now - now.rem_euclid(86_400);
    let interval = match source {
        StationSource::Metar if from >= now - METAR_BACK_S => 1_800,
        StationSource::Dwd if from < today => 600,
        _ => return false,
    };
    // Up to now at most; half of what the interval gives counts as complete.
    let span = to.min(now) - from;
    span > 0 && (count as i64) < span / interval / 2
}

pub async fn history(
    State(s): State<AppState>,
    Path(slug): Path<String>,
    query: Result<Query<HistoryQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<impl IntoResponse, ApiError> {
    let Query(q) = query.map_err(|_| ApiError::InvalidRange)?;
    if q.to <= q.from || q.to - q.from > MAX_SPAN_S {
        return Err(ApiError::InvalidRange);
    }
    let st = station::by_slug(&s.db, &slug)
        .await?
        .ok_or(ApiError::NotFound)?;
    let id = station::id_of(&s.db, &slug)
        .await?
        .ok_or(ApiError::NotFound)?;
    let now = wf_core::time::now();
    let mut observations = observation::between(&s.db, id, q.from, q.to).await?;
    if let Some(m) = &s.measurements
        && q.from >= now - KEEP_OBSERVATIONS_S
        && wants_backfill(st.source, q.from, q.to, observations.len(), now)
        && !s.backfilled.contains_key(&id)
    {
        // Once per station within the cache's lifetime, however many ask at the same time.
        s.backfilled
            .get_with(id, async {
                match m.backfill(id, &st, now - KEEP_OBSERVATIONS_S).await {
                    Ok(n) => tracing::info!("backfilled {n} measurements of {}", st.slug),
                    Err(e) => tracing::warn!("backfilling {}: {e}", st.slug),
                }
            })
            .await;
        observations = observation::between(&s.db, id, q.from, q.to).await?;
    }
    // A finished day only changes by a late backfill.
    let max_age = if q.to < now - 3_600 { 3_600 } else { 60 };
    Ok((
        [(header::CACHE_CONTROL, format!("public, max-age={max_age}"))],
        Json(StationHistory {
            observations,
            oldest: observation::oldest(&s.db, id).await?,
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backfills_thin_past_days() {
        // 2026-10-11 12:00 UTC
        let now = 1_791_720_000;
        let yesterday = now - 12 * 3_600 - 86_400;
        let day = (yesterday, yesterday + 86_400);
        assert!(wants_backfill(StationSource::Metar, day.0, day.1, 3, now));
        assert!(!wants_backfill(StationSource::Metar, day.0, day.1, 40, now));
        assert!(wants_backfill(StationSource::Dwd, day.0, day.1, 30, now));
        assert!(!wants_backfill(StationSource::Dwd, day.0, day.1, 140, now));
        // Beyond ten days: nothing to fetch from the METAR API
        let old = yesterday - 11 * 86_400;
        assert!(!wants_backfill(
            StationSource::Metar,
            old,
            old + 86_400,
            0,
            now
        ));
        // Today: the DWD archive ends yesterday; the live fetch covers it
        let today = now - 12 * 3_600;
        assert!(!wants_backfill(
            StationSource::Dwd,
            today,
            today + 86_400,
            0,
            now
        ));
    }
}
