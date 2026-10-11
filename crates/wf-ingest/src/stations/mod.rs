//! Station lists: airports with a weather report (OurAirports for names and places,
//! aviationweather.gov for which ones send METARs and TAFs) and the DWD's wind stations.

mod dwd;
mod ourairports;

use sqlx::SqlitePool;
use wf_core::station::{self, StationSource};

pub use dwd::{fetch_dwd, parse_dwd};
pub use ourairports::{fetch_airports, parse_airports, parse_awc_stations};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{what}: {source}")]
    Fetch {
        what: &'static str,
        source: reqwest::Error,
    },
    #[error("{what}: {source}")]
    Json {
        what: &'static str,
        source: serde_json::Error,
    },
    #[error("unpacking {what}: {source}")]
    Unpack {
        what: &'static str,
        source: std::io::Error,
    },
    /// A list came empty: its format has probably changed.
    #[error("no {0} found; has the format changed?")]
    Empty(&'static str),
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
    /// Some lists failed; the others were imported.
    #[error("station import: {}", .0.iter().map(|(s, e)| format!("{s:?}: {e}")).collect::<Vec<_>>().join("; "))]
    Partial(Vec<(StationSource, Error)>),
}

type Result<T> = std::result::Result<T, Error>;

/// OurAirports' data (public domain), mirrored on GitHub.
pub const OURAIRPORTS_BASE: &str =
    "https://raw.githubusercontent.com/davidmegginson/ourairports-data/main";

/// Every station aviationweather.gov knows, with the reports it sends.
pub const AWC_STATIONS_URL: &str = "https://aviationweather.gov/data/cache/stations.cache.json.gz";

/// DWD stations with 10-minute wind values (current year).
pub const DWD_STATIONS_URL: &str = "https://opendata.dwd.de/climate_environment/CDC/observations_germany/climate/10_minutes/wind/now/zehn_now_ff_Beschreibung_Stationen.txt";

/// Lists are re-imported once a week.
pub const IMPORT_EVERY_S: i64 = 7 * 86_400;

#[derive(Debug, Clone)]
pub struct Sources {
    pub ourairports_base: String,
    pub awc_stations_url: String,
    pub dwd_url: String,
}

impl Default for Sources {
    fn default() -> Self {
        Self {
            ourairports_base: OURAIRPORTS_BASE.into(),
            awc_stations_url: AWC_STATIONS_URL.into(),
            dwd_url: DWD_STATIONS_URL.into(),
        }
    }
}

/// A list shorter than this is taken as a broken download and does not remove stations.
const PLAUSIBLE: [(StationSource, usize); 2] =
    [(StationSource::Metar, 300), (StationSource::Dwd, 100)];

/// Imports both lists; each one failing on its own is logged, not fatal. Stations a complete
/// list no longer has are removed with their measurements.
pub async fn import_all(db: &SqlitePool, http: &reqwest::Client, sources: &Sources) -> Result<()> {
    let now = wf_core::time::now();
    let mut errors = Vec::new();
    let (airports, dwd) = tokio::join!(
        fetch_airports(http, &sources.ourairports_base, &sources.awc_stations_url),
        fetch_dwd(http, &sources.dwd_url, now),
    );
    for (source, list) in [(StationSource::Metar, airports), (StationSource::Dwd, dwd)] {
        match list {
            Ok(list) => {
                let n = station::upsert(db, &list, now).await?;
                tracing::info!("imported {n} {source:?} stations");
                if PLAUSIBLE.iter().any(|(s, min)| *s == source && n >= *min) {
                    let gone = station::remove_unseen(db, source, now).await?;
                    if gone > 0 {
                        tracing::info!("removed {gone} {source:?} stations no longer listed");
                    }
                }
            }
            Err(e) => errors.push((source, e)),
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(Error::Partial(errors))
    }
}

/// Whether a source's list is older than [`IMPORT_EVERY_S`].
pub async fn due(db: &SqlitePool, now: i64) -> sqlx::Result<bool> {
    for source in [StationSource::Metar, StationSource::Dwd] {
        if station::last_import(db, source).await? < now - IMPORT_EVERY_S {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn get_bytes(http: &reqwest::Client, url: &str, what: &'static str) -> Result<Vec<u8>> {
    let fetch = |source| Error::Fetch { what, source };
    let res = http
        .get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(fetch)?;
    Ok(res.bytes().await.map_err(fetch)?.to_vec())
}

async fn get_text(http: &reqwest::Client, url: &str, what: &'static str) -> Result<String> {
    Ok(decode(&get_bytes(http, url, what).await?))
}

/// UTF-8, or Latin-1 as older DWD files use.
pub(crate) fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}
