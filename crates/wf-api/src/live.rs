//! What a station measures now, and its airport forecast (TAF).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::header;
use axum::response::IntoResponse;
use serde::Serialize;
use ts_rs::TS;
use wf_core::observation::{self, Observation};
use wf_core::station::{self, StationSource};
use wf_core::taf::{self, Taf};

use crate::{ApiError, AppState};

/// How long a visitor waits for a stale station's measurements and TAF to be fetched.
const FETCH_LIMIT: std::time::Duration = std::time::Duration::from_secs(15);

/// Measurements shown: the last 24 hours.
const WINDOW_S: i64 = 24 * 3_600;

/// TAFs come every three or six hours (amendments any time); asked for again after this.
const TAF_STALE_S: i64 = 35 * 60;

/// `GET /api/stations/{slug}/live`
#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StationLive {
    /// The last 24 hours, oldest first; empty when the station sent nothing.
    pub observations: Vec<Observation>,
    /// The TAF in force, for stations that publish one.
    pub taf: Option<Taf>,
}

/// Whether to ask the source now instead of waiting for the scheduler: the newest value is
/// older than the source's interval allows, or (METAR) the day's history has a gap at the
/// start, as after a fresh start.
fn wants_fetch(source: StationSource, obs: &[Observation], now: i64) -> bool {
    let (stale_after, history) = match source {
        // Half-hourly or hourly reports, the cache file is read every ten minutes.
        StationSource::Metar => (45 * 60, true),
        // Ten-minute values, published about every half hour; the file holds today (UTC).
        StationSource::Dwd => (40 * 60, false),
        StationSource::Owm => return false,
    };
    match (obs.first(), obs.last()) {
        (Some(first), Some(last)) => {
            last.at < now - stale_after || (history && first.at > now - WINDOW_S + 3 * 3_600)
        }
        _ => true,
    }
}

pub async fn live(
    State(s): State<AppState>,
    Path(slug): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let st = station::by_slug(&s.db, &slug)
        .await?
        .ok_or(ApiError::NotFound)?;
    let id = station::id_of(&s.db, &slug)
        .await?
        .ok_or(ApiError::NotFound)?;
    let now = wf_core::time::now();
    let mut observations = observation::since(&s.db, id, now - WINDOW_S).await?;
    let taf_stale = st.has_taf
        && taf::last_fetch(&s.db, id)
            .await?
            .is_none_or(|at| at < now - TAF_STALE_S);
    if let Some(m) = &s.measurements
        && (taf_stale || wants_fetch(st.source, &observations, now))
        && !s.fetched.contains_key(&id)
    {
        // Once per station within the cache's lifetime, however many ask at the same time.
        s.fetched
            .get_with(id, async {
                // The visitor waits for it: a source that hangs is given up after a while, the
                // scheduler fetches the station on its next turn
                match tokio::time::timeout(FETCH_LIMIT, m.fetch_station(id, &st)).await {
                    Ok(Ok(_)) => {}
                    Ok(Err(e)) => tracing::warn!("measurements of {}: {e:#}", st.slug),
                    Err(_) => tracing::warn!("measurements of {}: gave up waiting", st.slug),
                }
            })
            .await;
        observations = observation::since(&s.db, id, now - WINDOW_S).await?;
    }
    let taf = if st.has_taf {
        taf::current(&s.db, id, now).await?
    } else {
        None
    };
    Ok((
        [(header::CACHE_CONTROL, "public, max-age=60")],
        Json(StationLive { observations, taf }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(t: i64) -> Observation {
        Observation {
            at: t,
            wind: Some(10.0),
            gust: None,
            dir: None,
            temp: None,
            raw: None,
        }
    }

    #[test]
    fn fetches_when_stale_or_incomplete() {
        let now = 100_000;
        assert!(wants_fetch(StationSource::Metar, &[], now));
        let day = [at(now - WINDOW_S + 1_800), at(now - 1_200)];
        assert!(!wants_fetch(StationSource::Metar, &day, now));
        // Only the newest report, as right after a start.
        assert!(wants_fetch(StationSource::Metar, &[at(now - 1_200)], now));
        assert!(!wants_fetch(StationSource::Dwd, &[at(now - 1_200)], now));
        assert!(wants_fetch(StationSource::Dwd, &[at(now - 3_000)], now));
    }
}
