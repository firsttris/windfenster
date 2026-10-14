//! A station's wind over ten whole years, for its statistics: hourly values from an archive,
//! folded into `wf_core::climate::WindStats` and kept only as that. Airports from the Iowa
//! Environmental Mesonet's METAR archive (`/cgi-bin/request/asos.py`, as in
//! akrherz/iem `pylib/iemweb/request/asos.py`), DWD stations from the DWD's climate archive of
//! hourly wind. Made once per station on request, one station at a time, and again once a newer
//! year has ended.

use std::collections::HashSet;
use std::io::Read;
use std::sync::Arc;
use std::time::Duration;

use sqlx::SqlitePool;
use wf_core::climate::{self, Sample, Stored, WindStats};
use wf_core::station::{Station, StationSource};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
    #[error("unzipping: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("reading the archive: {0}")]
    Read(#[from] std::io::Error),
    #[error("no hourly file for DWD station {0}")]
    NoDwdFile(String),
}

type Result<T> = std::result::Result<T, Error>;

/// Whole years the statistics cover
pub const YEARS: i32 = 10;

pub const IEM_BASE: &str = "https://mesonet.agron.iastate.edu";
pub const DWD_HOURLY_BASE: &str =
    "https://opendata.dwd.de/climate_environment/CDC/observations_germany/climate/hourly/wind";

/// The IEM allows one request a second per address; we keep well below.
const IEM_GAP: Duration = Duration::from_secs(2);
/// A busy IEM (429, 503): wait this long, a few times, then leave it for the next request
const IEM_RETRY: Duration = Duration::from_secs(30);
/// A year of METARs can take the IEM a while to gather.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(180);
/// After a failure, a station is tried again after this long
const RETRY_AFTER: Duration = Duration::from_secs(6 * 3_600);

#[derive(Debug, Clone)]
pub struct Urls {
    pub iem: String,
    pub dwd_hourly: String,
}

impl Default for Urls {
    fn default() -> Self {
        Self {
            iem: IEM_BASE.into(),
            dwd_hourly: DWD_HOURLY_BASE.into(),
        }
    }
}

/// A station's statistics as the API gives them
#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Ready(WindStats),
    /// Being made; ask again in a minute
    Pending,
    /// No archive for this station, or it had no hours, or it failed lately
    None,
}

pub struct Climate {
    db: SqlitePool,
    http: reqwest::Client,
    urls: Urls,
    /// One station at a time, the IEM's pace kept across them
    gate: tokio::sync::Mutex<()>,
    running: std::sync::Mutex<HashSet<i64>>,
    /// Between two requests to the IEM
    gap: Duration,
    /// Stations whose last attempt failed: when, and why
    failed: std::sync::Mutex<std::collections::HashMap<i64, (std::time::Instant, String)>>,
}

/// Statistics being made and those that failed lately, for the admin page
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Work {
    pub running: Vec<i64>,
    /// Station row id, seconds since it failed, why
    pub failed: Vec<(i64, u64, String)>,
}

impl Climate {
    pub fn new(db: SqlitePool, http: reqwest::Client, urls: Urls) -> Self {
        Self {
            db,
            http,
            urls,
            gate: tokio::sync::Mutex::new(()),
            gap: IEM_GAP,
            running: std::sync::Mutex::new(HashSet::new()),
            failed: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// What is being made, and what failed within the last hours.
    pub fn work(&self) -> Work {
        let running = self
            .running
            .lock()
            .map(|r| r.iter().copied().collect())
            .unwrap_or_default();
        let failed = self
            .failed
            .lock()
            .map(|f| {
                f.iter()
                    .filter(|(_, (at, _))| at.elapsed() < RETRY_AFTER)
                    .map(|(id, (at, e))| (*id, at.elapsed().as_secs(), e.clone()))
                    .collect()
            })
            .unwrap_or_default();
        Work { running, failed }
    }

    /// Forgets that a station failed, so the next look tries it again at once.
    pub fn retry(&self, id: i64) {
        if let Ok(mut f) = self.failed.lock() {
            f.remove(&id);
        }
    }

    /// With another pause between IEM requests (tests: none)
    pub fn with_gap(self, gap: Duration) -> Self {
        Self { gap, ..self }
    }

    /// The statistics of a station (row `id`) if made for the last whole year; else starts
    /// making them in the background and says so.
    pub async fn status(self: &Arc<Self>, id: i64, station: &Station, now: i64) -> Result<Status> {
        if station.source == StationSource::Owm {
            return Ok(Status::None);
        }
        let want = climate::last_whole_year(now);
        let stored = climate::stored(&self.db, id).await?;
        if let Some(s) = &stored
            && s.last_year >= want
        {
            return Ok(s.stats.clone().map_or(Status::None, Status::Ready));
        }
        let failed_lately = self
            .failed
            .lock()
            .map(|f| f.get(&id).is_some_and(|(at, _)| at.elapsed() < RETRY_AFTER))
            .unwrap_or(false);
        if failed_lately {
            // An older year's statistics are better than none meanwhile
            return Ok(stored
                .and_then(|s| s.stats)
                .map_or(Status::None, Status::Ready));
        }
        if self
            .running
            .lock()
            .map(|mut r| r.insert(id))
            .unwrap_or(false)
        {
            let me = Arc::clone(self);
            let station = station.clone();
            let source = if station.source == StationSource::Dwd {
                "dwd"
            } else {
                "iem"
            };
            tokio::spawn(async move {
                match me.make(id, &station, want, now).await {
                    Ok(()) => wf_core::status::board().source_ok(source),
                    Err(e) => {
                        tracing::warn!("wind statistics of {}: {e}", station.slug);
                        wf_core::status::board().source_failed(source, &e);
                        if let Ok(mut f) = me.failed.lock() {
                            f.insert(id, (std::time::Instant::now(), format!("{e:#}")));
                        }
                    }
                }
                if let Ok(mut r) = me.running.lock() {
                    r.remove(&id);
                }
            });
        }
        Ok(Status::Pending)
    }

    async fn make(&self, id: i64, station: &Station, last_year: i32, now: i64) -> Result<()> {
        let _turn = self.gate.lock().await;
        let first_year = last_year - YEARS + 1;
        let (samples, source) = match station.source {
            StationSource::Metar => (self.iem(&station.code, first_year, last_year).await?, "iem"),
            StationSource::Dwd => (self.dwd(&station.code, first_year, last_year).await?, "dwd"),
            StationSource::Owm => return Ok(()),
        };
        let stats = (!samples.is_empty())
            .then(|| climate::summarize(&samples, station.lon, source, first_year, last_year));
        tracing::info!(
            "wind statistics of {}: {} hours, {first_year}–{last_year}",
            station.slug,
            samples.len()
        );
        climate::store(
            &self.db,
            id,
            &Stored {
                stats,
                last_year,
                computed_at: now,
            },
        )
        .await?;
        Ok(())
    }

    /// An airport's routine METARs, a year per request
    async fn iem(&self, code: &str, first_year: i32, last_year: i32) -> Result<Vec<Sample>> {
        let mut out = Vec::new();
        for year in first_year..=last_year {
            let url = iem_url(&self.urls.iem, code, year);
            let mut tries = 0;
            let body = loop {
                tokio::time::sleep(self.gap).await;
                let res = self.http.get(&url).timeout(REQUEST_TIMEOUT).send().await?;
                let status = res.status().as_u16();
                if (status == 429 || status == 503) && tries < 3 {
                    tries += 1;
                    tokio::time::sleep(IEM_RETRY).await;
                    continue;
                }
                break res.error_for_status()?.text().await?;
            };
            out.extend(parse_iem(&body));
        }
        Ok(out)
    }

    /// A DWD station's hourly wind: the long file to the end of last year, and the recent one
    async fn dwd(&self, code: &str, first_year: i32, last_year: i32) -> Result<Vec<Sample>> {
        let base = &self.urls.dwd_hourly;
        let listing = self.text(&format!("{base}/historical/")).await?;
        let name =
            historical_name(&listing, code).ok_or_else(|| Error::NoDwdFile(code.to_owned()))?;
        let mut out = parse_dwd_hourly(&product(
            &self.bytes(&format!("{base}/historical/{name}")).await?,
        )?);
        // The recent file covers about the last 500 days; a missing one is no failure.
        if let Ok(recent) = self
            .bytes(&format!("{base}/recent/stundenwerte_FF_{code}_akt.zip"))
            .await
        {
            out.extend(parse_dwd_hourly(&product(&recent)?));
        }
        let (from, to) = (year_start(first_year), year_start(last_year + 1));
        out.retain(|s| s.at >= from && s.at < to);
        Ok(out)
    }

    async fn text(&self, url: &str) -> Result<String> {
        Ok(self
            .http
            .get(url)
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?)
    }

    async fn bytes(&self, url: &str) -> Result<Vec<u8>> {
        let res = self
            .http
            .get(url)
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await?
            .error_for_status()?;
        Ok(res.bytes().await?.to_vec())
    }
}

fn year_start(year: i32) -> i64 {
    wf_core::time::unix(i64::from(year), 1, 1, 0, 0, 0)
}

/// The IEM's request for one station and year: wind speed (kn) and direction of the routine
/// reports (type 3), times in UTC, missing values as M
pub fn iem_url(base: &str, code: &str, year: i32) -> String {
    format!(
        "{base}/cgi-bin/request/asos.py?station={code}&data=sknt&data=drct\
         &sts={year}-01-01T00:00:00Z&ets={next}-01-01T00:00:00Z&tz=UTC&format=onlycomma\
         &missing=M&report_type=3",
        next = year + 1
    )
}

/// The IEM's comma-separated answer: `station,valid,sknt,drct` with `valid` as
/// `YYYY-MM-DD HH:MM` in UTC and `M` for missing; rows without a speed are left out.
pub fn parse_iem(body: &str) -> Vec<Sample> {
    let mut lines = body.lines();
    let Some(head) = lines.next() else {
        return vec![];
    };
    let cols: Vec<&str> = head.split(',').map(str::trim).collect();
    let at = |name: &str| cols.iter().position(|c| *c == name);
    let (Some(valid), Some(sknt), Some(drct)) = (at("valid"), at("sknt"), at("drct")) else {
        return vec![];
    };
    lines
        .filter_map(|line| {
            let f: Vec<&str> = line.split(',').map(str::trim).collect();
            let t = wf_core::time::parse_iso(&format!("{}Z", f.get(valid)?.replace(' ', "T")))?;
            let kn: f32 = f.get(sknt)?.parse().ok()?;
            let dir = f
                .get(drct)
                .and_then(|d| d.parse::<f32>().ok())
                .filter(|d| (0.0..=360.0).contains(d));
            Some(Sample { at: t, kn, dir })
        })
        .collect()
}

/// The long file's name for a station in the folder listing, e.g.
/// `stundenwerte_FF_04177_20081101_20251231_hist.zip`
pub fn historical_name(listing: &str, code: &str) -> Option<String> {
    let prefix = format!("stundenwerte_FF_{code}_");
    listing
        .split('"')
        .find(|s| s.starts_with(&prefix) && s.ends_with("_hist.zip"))
        .map(str::to_owned)
}

/// The data file of a DWD archive (`produkt_ff_stunde_…`), Latin-1
fn product(zip: &[u8]) -> Result<String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip))?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        if file.name()?.starts_with("produkt") {
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            return Ok(bytes.iter().map(|&b| char::from(b)).collect());
        }
    }
    Err(Error::Zip(zip::result::ZipError::FileNotFound))
}

/// The DWD's hourly wind: `;`-separated, `MESS_DATUM` as `YYYYMMDDHH` in UTC, `F` in m/s, `D` in
/// degrees, -999 for missing
pub fn parse_dwd_hourly(text: &str) -> Vec<Sample> {
    let mut lines = text.lines();
    let Some(head) = lines.next() else {
        return vec![];
    };
    let cols: Vec<&str> = head.split(';').map(str::trim).collect();
    let at = |name: &str| cols.iter().position(|c| *c == name);
    let (Some(time), Some(f), Some(d)) = (at("MESS_DATUM"), at("F"), at("D")) else {
        return vec![];
    };
    lines
        .filter_map(|line| {
            let v: Vec<&str> = line.split(';').map(str::trim).collect();
            let t = wf_core::time::parse_compact(&format!("{}00", v.get(time)?))?;
            let ms: f32 = v.get(f)?.parse().ok().filter(|x: &f32| *x >= 0.0)?;
            let dir = v
                .get(d)
                .and_then(|x| x.parse::<f32>().ok())
                .filter(|x| (0.0..=360.0).contains(x));
            Some(Sample {
                at: t,
                kn: ms / wf_core::units::MS_PER_KN as f32,
                dir,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_iem_answer() {
        // As the IEM answered for EDSB on 1 July 2024
        let body = "station,valid,sknt,drct\nEDSB,2024-07-01 00:50,4.00,200.00\nEDSB,2024-07-01 01:50,5.00,M\nEDSB,2024-07-01 02:50,M,210.00\n";
        let got = parse_iem(body);
        assert_eq!(got.len(), 2);
        assert_eq!(
            got[0],
            Sample {
                at: 1_719_795_000,
                kn: 4.0,
                dir: Some(200.0)
            }
        );
        assert_eq!(got[1].dir, None);
        assert!(iem_url(IEM_BASE, "EDSB", 2024).contains(
            "station=EDSB&data=sknt&data=drct&sts=2024-01-01T00:00:00Z&ets=2025-01-01T00:00:00Z"
        ));
    }

    #[test]
    fn reads_the_dwd_archive() {
        let text = "STATIONS_ID;MESS_DATUM;QN_3;   F;   D;eor\n       4177;2008110100;   10;   4.1;  20;eor\n       4177;2008110101;   10;-999;  20;eor\n";
        let got = parse_dwd_hourly(text);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].at, wf_core::time::unix(2008, 11, 1, 0, 0, 0));
        assert!((got[0].kn - 7.97).abs() < 0.01);
        let listing = r#"<a href="stundenwerte_FF_04177_20081101_20251231_hist.zip">x</a><a href="stundenwerte_FF_04178_20081101_20251231_hist.zip">"#;
        assert_eq!(
            historical_name(listing, "04177").as_deref(),
            Some("stundenwerte_FF_04177_20081101_20251231_hist.zip")
        );
        assert_eq!(historical_name(listing, "00003"), None);
    }
}
