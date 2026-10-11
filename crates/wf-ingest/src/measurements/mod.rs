//! Measured wind, airport forecasts and water gauges: all METARs from aviationweather.gov's
//! cache file, TAFs and DWD 10-minute values for the stations people look at, one station's
//! recent values on request, and every PEGELONLINE gauge with its latest values.

mod dwd;
mod metar;
mod pegelonline;
mod taf;

use std::io::Read;

use sqlx::SqlitePool;
use wf_core::gauge;
use wf_core::observation::{self, Observation};
use wf_core::station::{self, Station, StationSource};
use wf_core::taf::Taf;

pub use dwd::merge_dwd;
pub use metar::{parse_metar_csv, parse_metar_json};
pub use pegelonline::{parse_gauges, parse_measurements};
pub use taf::parse_taf_json;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
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
    #[error("unzipping: {0}")]
    Zip(#[from] zip::result::ZipError),
}

type Result<T> = std::result::Result<T, Error>;

fn json(what: &'static str) -> impl FnOnce(serde_json::Error) -> Error {
    move |source| Error::Json { what, source }
}

pub const AWC_BASE: &str = "https://aviationweather.gov";

/// TAFs asked for in one request.
const TAF_BATCH: usize = 40;

pub const DWD_BASE: &str =
    "https://opendata.dwd.de/climate_environment/CDC/observations_germany/climate/10_minutes";

pub const PEGELONLINE_BASE: &str = "https://www.pegelonline.wsv.de/webservices/rest-api/v2";

/// A gauge list shorter than this is taken as a broken answer and removes nothing.
const PLAUSIBLE_GAUGES: usize = 300;

#[derive(Debug, Clone)]
pub struct Urls {
    /// aviationweather.gov: the METAR cache file and the METAR API.
    pub awc_base: String,
    /// The DWD's 10-minute products.
    pub dwd_base: String,
    /// PEGELONLINE's REST API (v2).
    pub pegelonline_base: String,
}

impl Default for Urls {
    fn default() -> Self {
        Self {
            awc_base: AWC_BASE.into(),
            dwd_base: DWD_BASE.into(),
            pegelonline_base: PEGELONLINE_BASE.into(),
        }
    }
}

pub struct Measurements {
    db: SqlitePool,
    http: reqwest::Client,
    urls: Urls,
}

impl Measurements {
    pub fn new(db: SqlitePool, http: reqwest::Client, urls: Urls) -> Self {
        Self { db, http, urls }
    }

    /// Downloads the METAR cache (every current METAR worldwide, about 250 KB, refreshed every
    /// minute) and stores the reports of our stations; returns how many.
    pub async fn import_metars(&self) -> Result<usize> {
        let gz = self
            .get(&format!(
                "{}/data/cache/metars.cache.csv.gz",
                self.urls.awc_base
            ))
            .await?;
        let mut text = String::new();
        flate2::read::GzDecoder::new(&gz[..])
            .read_to_string(&mut text)
            .map_err(|source| Error::Unpack {
                what: "metars.cache.csv.gz",
                source,
            })?;
        let ids = observation::station_ids(&self.db, StationSource::Metar).await?;
        let rows: Vec<(i64, Observation)> = parse_metar_csv(&text)
            .into_iter()
            .filter_map(|(icao, o)| ids.get(&icao).map(|id| (*id, o)))
            .collect();
        observation::store(&self.db, &rows).await?;
        Ok(rows.len())
    }

    /// Fetches today's 10-minute values of the DWD stations that are tracked or were opened
    /// since `since`; returns how many were stored.
    pub async fn import_dwd(&self, since: i64) -> Result<usize> {
        let mut n = 0;
        for (id, st) in station::watched(&self.db, StationSource::Dwd, since).await? {
            match self.fetch_dwd(&st.code).await {
                Ok(rows) => n += self.store(id, rows).await?,
                Err(e) => tracing::warn!("DWD {}: {e:#}", st.code),
            }
        }
        Ok(n)
    }

    /// Fetches the TAFs of the stations with one that are tracked or were opened since `since`;
    /// returns how many were stored.
    pub async fn import_tafs(&self, since: i64) -> Result<usize> {
        let stations: Vec<(i64, Station)> = station::watched(&self.db, StationSource::Metar, since)
            .await?
            .into_iter()
            .filter(|(_, st)| st.has_taf)
            .collect();
        let mut n = 0;
        for batch in stations.chunks(TAF_BATCH) {
            let codes: Vec<&str> = batch.iter().map(|(_, st)| st.code.as_str()).collect();
            match self.fetch_tafs(&codes).await {
                Ok(tafs) => {
                    let rows: Vec<(i64, Taf)> = tafs
                        .into_iter()
                        .filter_map(|(icao, t)| {
                            batch
                                .iter()
                                .find(|(_, st)| st.code == icao)
                                .map(|(id, _)| (*id, t))
                        })
                        .collect();
                    wf_core::taf::store(&self.db, &rows).await?;
                    n += rows.len();
                }
                Err(e) => tracing::warn!("TAFs {}: {e:#}", codes.join(",")),
            }
        }
        Ok(n)
    }

    async fn fetch_tafs(&self, codes: &[&str]) -> Result<Vec<(String, Taf)>> {
        let body = self
            .get(&format!(
                "{}/api/data/taf?ids={}&format=json",
                self.urls.awc_base,
                codes.join(",")
            ))
            .await?;
        parse_taf_json(&body, wf_core::time::now()).map_err(json("TAF JSON"))
    }

    /// One station's recent values now: the METARs of the last 24 hours and the TAF, or the
    /// DWD's values of today (UTC). Returns how many measurements were stored.
    pub async fn fetch_station(&self, id: i64, st: &Station) -> Result<usize> {
        if st.has_taf {
            let tafs = self.fetch_tafs(&[st.code.as_str()]).await?;
            let rows: Vec<(i64, Taf)> = tafs
                .into_iter()
                .filter(|(icao, _)| *icao == st.code)
                .map(|(_, t)| (id, t))
                .collect();
            wf_core::taf::store(&self.db, &rows).await?;
        }
        let rows = match st.source {
            StationSource::Metar => {
                let body = self
                    .get(&format!(
                        "{}/api/data/metar?ids={}&format=json&hours=24",
                        self.urls.awc_base, st.code
                    ))
                    .await?;
                parse_metar_json(&body)
                    .map_err(json("METAR JSON"))?
                    .into_iter()
                    .filter(|(icao, _)| *icao == st.code)
                    .map(|(_, o)| o)
                    .collect()
            }
            StationSource::Dwd => self.fetch_dwd(&st.code).await?,
            // OpenWindMap is not connected yet.
            StationSource::Owm => return Ok(0),
        };
        self.store(id, rows).await
    }

    /// Fetches every gauge with its latest level, flow and water temperature (one request of
    /// about 750 KB); returns how many were stored.
    pub async fn import_gauges(&self) -> Result<usize> {
        let body = self
            .get(&format!(
                "{}/stations.json?includeTimeseries=true&includeCurrentMeasurement=true",
                self.urls.pegelonline_base
            ))
            .await?;
        let gauges = parse_gauges(&body).map_err(json("PEGELONLINE stations"))?;
        let now = wf_core::time::now();
        let n = gauge::upsert(&self.db, &gauges, now).await?;
        if n >= PLAUSIBLE_GAUGES {
            gauge::remove_unseen(&self.db, now - 7 * 86_400).await?;
        }
        Ok(n)
    }

    /// A gauge's water level over the last seven days (time, cm), oldest first.
    pub async fn gauge_history(&self, uuid: &str) -> Result<Vec<(i64, f32)>> {
        let body = self
            .get(&format!(
                "{}/stations/{uuid}/W/measurements.json?start=P7D",
                self.urls.pegelonline_base
            ))
            .await?;
        parse_measurements(&body).map_err(json("PEGELONLINE measurements"))
    }

    async fn store(&self, id: i64, rows: Vec<Observation>) -> Result<usize> {
        let rows: Vec<(i64, Observation)> = rows.into_iter().map(|o| (id, o)).collect();
        Ok(observation::store(&self.db, &rows).await?)
    }

    async fn get(&self, url: &str) -> Result<Vec<u8>> {
        let res = self.http.get(url).send().await?.error_for_status()?;
        Ok(res.bytes().await?.to_vec())
    }

    /// Today's 10-minute mean wind, direction and gusts of one DWD station.
    async fn fetch_dwd(&self, code: &str) -> Result<Vec<Observation>> {
        let base = &self.urls.dwd_base;
        let wind = dwd::unzip_first(
            &self
                .get(&format!(
                    "{base}/wind/now/10minutenwerte_wind_{code}_now.zip"
                ))
                .await?,
        )?;
        // Gusts are a separate product; without them the mean still counts.
        let gusts = match self
            .get(&format!(
                "{base}/extreme_wind/now/10minutenwerte_extrema_wind_{code}_now.zip"
            ))
            .await
        {
            Ok(zip) => dwd::unzip_first(&zip).unwrap_or_default(),
            Err(_) => String::new(),
        };
        Ok(merge_dwd(&wind, &gusts))
    }
}
