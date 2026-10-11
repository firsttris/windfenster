//! Measured wind: all METARs from aviationweather.gov's cache file, DWD 10-minute values for
//! the stations people look at, and one station's recent values on request.

use std::collections::BTreeMap;
use std::io::Read;

use anyhow::{Context, Result};
use serde::Deserialize;
use sqlx::SqlitePool;
use wf_core::observation::{self, Observation};
use wf_core::station::{self, Station, StationSource};
use wf_core::units::MS_PER_KN;

pub const AWC_BASE: &str = "https://aviationweather.gov";

pub const DWD_BASE: &str =
    "https://opendata.dwd.de/climate_environment/CDC/observations_germany/climate/10_minutes";

#[derive(Debug, Clone)]
pub struct Urls {
    /// aviationweather.gov: the METAR cache file and the METAR API.
    pub awc_base: String,
    /// The DWD's 10-minute products.
    pub dwd_base: String,
}

impl Default for Urls {
    fn default() -> Self {
        Self {
            awc_base: AWC_BASE.into(),
            dwd_base: DWD_BASE.into(),
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
            .context("unpacking metars.cache.csv.gz")?;
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

    /// One station's recent values now: the METARs of the last 24 hours, or the DWD's values
    /// of today (UTC). Returns how many were stored.
    pub async fn fetch_station(&self, id: i64, st: &Station) -> Result<usize> {
        let rows = match st.source {
            StationSource::Metar => {
                let body = self
                    .get(&format!(
                        "{}/api/data/metar?ids={}&format=json&hours=24",
                        self.urls.awc_base, st.code
                    ))
                    .await?;
                parse_metar_json(&body)?
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
        let wind = unzip_first(
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
            Ok(zip) => unzip_first(&zip).unwrap_or_default(),
            Err(_) => String::new(),
        };
        Ok(merge_dwd(&wind, &gusts))
    }
}

/// Parses aviationweather.gov's METAR cache CSV. Wind is in knots already (m/s reports are
/// converted); a direction of 0 with wind means variable (VRB), north is 360.
pub fn parse_metar_csv(text: &str) -> Vec<(String, Observation)> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(text.as_bytes());
    let Ok(headers) = reader.headers().cloned() else {
        return Vec::new();
    };
    let col = |name: &str| headers.iter().position(|h| h == name);
    let (Some(raw), Some(id), Some(time), Some(dir), Some(spd), Some(gust), Some(temp)) = (
        col("raw_text"),
        col("station_id"),
        col("observation_time"),
        col("wind_dir_degrees"),
        col("wind_speed_kt"),
        col("wind_gust_kt"),
        col("temp_c"),
    ) else {
        return Vec::new();
    };
    let num = |r: &csv::StringRecord, i: usize| r.get(i).and_then(|v| v.trim().parse::<f32>().ok());
    reader
        .records()
        .filter_map(Result::ok)
        .filter_map(|r| {
            let at = wf_core::time::parse_iso(r.get(time)?)?;
            let wind = num(&r, spd);
            let dir = num(&r, dir).filter(|d| *d > 0.0 || wind == Some(0.0));
            Some((
                r.get(id)?.to_owned(),
                Observation {
                    at,
                    wind,
                    gust: num(&r, gust),
                    dir: if wind == Some(0.0) { None } else { dir },
                    temp: num(&r, temp),
                    raw: r.get(raw).map(str::to_owned),
                },
            ))
        })
        .collect()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MetarJson {
    icao_id: String,
    obs_time: i64,
    temp: Option<f32>,
    /// Degrees, or "VRB"
    wdir: Option<serde_json::Value>,
    wspd: Option<f32>,
    wgst: Option<f32>,
    raw_ob: Option<String>,
}

/// Parses the METAR API's JSON (`/api/data/metar?format=json`). Wind in knots; the direction
/// is a number or "VRB", 0 with wind is variable as well.
pub fn parse_metar_json(body: &[u8]) -> Result<Vec<(String, Observation)>> {
    let list: Vec<MetarJson> = serde_json::from_slice(body).context("METAR JSON")?;
    Ok(list
        .into_iter()
        .map(|m| {
            let calm = m.wspd == Some(0.0);
            let dir = m
                .wdir
                .and_then(|d| d.as_f64())
                .map(|d| d as f32)
                .filter(|d| *d > 0.0 && !calm);
            (
                m.icao_id,
                Observation {
                    at: m.obs_time,
                    wind: m.wspd,
                    gust: m.wgst,
                    dir,
                    temp: m.temp,
                    raw: m.raw_ob,
                },
            )
        })
        .collect())
}

fn unzip_first(bytes: &[u8]) -> Result<String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    let mut file = zip.by_index(0)?;
    let mut raw = Vec::new();
    file.read_to_end(&mut raw)?;
    Ok(raw.iter().map(|&b| b as char).collect())
}

/// Columns of a DWD product file (`;`-separated, -999 for missing) by time.
fn dwd_columns(text: &str, names: &[&str]) -> BTreeMap<i64, Vec<Option<f32>>> {
    let mut lines = text.lines();
    let Some(header) = lines.next() else {
        return BTreeMap::new();
    };
    let cols: Vec<&str> = header.split(';').map(str::trim).collect();
    let Some(time) = cols.iter().position(|c| *c == "MESS_DATUM") else {
        return BTreeMap::new();
    };
    let wanted: Vec<Option<usize>> = names
        .iter()
        .map(|n| cols.iter().position(|c| c == n))
        .collect();
    lines
        .filter_map(|line| {
            let f: Vec<&str> = line.split(';').map(str::trim).collect();
            let at = wf_core::time::parse_compact(f.get(time)?)?;
            let values = wanted
                .iter()
                .map(|i| {
                    i.and_then(|i| f.get(i))
                        .and_then(|v| v.parse::<f32>().ok())
                        .filter(|v| *v > -999.0)
                })
                .collect();
            Some((at, values))
        })
        .collect()
}

/// Joins the wind file (FF_10 m/s, DD_10) and the gust file (FX_10 m/s) by time.
pub fn merge_dwd(wind: &str, gusts: &str) -> Vec<Observation> {
    let gusts = dwd_columns(gusts, &["FX_10"]);
    let kn = |ms: f32| ((ms / MS_PER_KN as f32) * 10.0).round() / 10.0;
    dwd_columns(wind, &["FF_10", "DD_10"])
        .into_iter()
        .map(|(at, v)| {
            let wind = v[0].map(kn);
            Observation {
                at,
                wind,
                gust: gusts.get(&at).and_then(|g| g[0]).map(kn),
                dir: if wind == Some(0.0) { None } else { v[1] },
                temp: None,
                raw: None,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metar_cache_rows() {
        // Header and rows from the real file (2026-10-11), columns after temp cut.
        let csv = "raw_text,station_id,observation_time,latitude,longitude,temp_c,dewpoint_c,wind_dir_degrees,wind_speed_kt,wind_gust_kt\n\
\"METAR EDSB 110850Z AUTO 21009KT CAVOK 13/10 Q1015\",EDSB,2026-10-11T08:50:00.000Z,48.7790,8.0810,13,10,210,9,\n\
\"METAR KDBN 110855Z AUTO VRB05KT 10SM 20/20 A2994 RMK AO2 LTG\",KDBN,2026-10-11T08:55:00.000Z,32.5,-82.8,20,20,0,5,\n\
\"METAR KFMH 110856Z 00000KT 10SM CLR 06/05 A3047\",KFMH,2026-10-11T08:56:00.000Z,41.6,-70.5,6,5,0,0,\n\
\"METAR KSEE 110855Z AUTO 17005G11KT 10SM SCT030 20/18 A2945 RMK AO2\",KSEE,2026-10-11T08:55:00.000Z,32.8,-116.9,20,18,170,5,11\n\
\"METAR LXXX 110850Z 36012KT 9999 15/10 Q1010\",LXXX,2026-10-11T08:50:00.000Z,48,8,15,10,360,12,\n";
        let rows = parse_metar_csv(csv);
        assert_eq!(rows.len(), 5);
        let (id, edsb) = &rows[0];
        assert_eq!(id, "EDSB");
        assert_eq!(edsb.at, 1_791_708_600);
        assert_eq!(
            (edsb.wind, edsb.dir, edsb.gust, edsb.temp),
            (Some(9.0), Some(210.0), None, Some(13.0))
        );
        assert_eq!(
            edsb.raw.as_deref(),
            Some("METAR EDSB 110850Z AUTO 21009KT CAVOK 13/10 Q1015")
        );
        assert_eq!((rows[1].1.wind, rows[1].1.dir), (Some(5.0), None)); // VRB
        assert_eq!((rows[2].1.wind, rows[2].1.dir), (Some(0.0), None)); // calm
        assert_eq!(rows[3].1.gust, Some(11.0));
        assert_eq!(rows[4].1.dir, Some(360.0)); // north
    }

    #[test]
    fn metar_api_rows() {
        // From the real answer of /api/data/metar?ids=EDSB,LGRP&format=json (2026-10-11), cut.
        let body = br#"[{"icaoId":"LGRP","receiptTime":"2026-10-11T08:54:50.248Z","obsTime":1791708600,"reportTime":"2026-10-11T09:00:00.000Z","temp":25,"dewp":19,"wdir":340,"wspd":4,"visib":"6+","altim":1012,"qcField":16,"metarType":"METAR","rawOb":"METAR LGRP 110850Z 34004KT 280V020 9999 FEW018 25/19 Q1012 NOSIG","lat":36.405,"lon":28.086,"elev":3,"name":"Rhodes/Diagoras Arpt, K, GR","cover":"FEW","clouds":[{"cover":"FEW","base":1800}],"fltCat":"VFR"},
            {"icaoId":"LGRP","obsTime":1791706800,"temp":24,"wdir":"VRB","wspd":4,"rawOb":"METAR LGRP 110820Z VRB04KT 9999 FEW018 24/19 Q1012 NOSIG"},
            {"icaoId":"EDSB","obsTime":1791708600,"temp":13,"wdir":210,"wspd":9,"wgst":null,"rawOb":"METAR EDSB 110850Z AUTO 21009KT CAVOK 13/10 Q1015"}]"#;
        let rows = parse_metar_json(body).unwrap();
        assert_eq!(rows.len(), 3);
        let (id, o) = &rows[0];
        assert_eq!(
            (id.as_str(), o.at, o.wind, o.dir, o.temp),
            ("LGRP", 1_791_708_600, Some(4.0), Some(340.0), Some(25.0))
        );
        assert_eq!((rows[1].1.wind, rows[1].1.dir), (Some(4.0), None)); // VRB
        assert_eq!(rows[2].1.gust, None);
        assert!(parse_metar_json(b"[]").unwrap().is_empty());
    }

    #[test]
    fn dwd_products() {
        // From the real files of Rheinstetten (04177), 2026-10-11.
        let wind = "STATIONS_ID;MESS_DATUM;  QN;FF_10;DD_10;eor\n\
       4177;202610110000;    2;   3.8; 230;eor\n\
       4177;202610110810;    2;   4.3; 230;eor\n\
       4177;202610110820;    2;-999; -999;eor\n";
        let gusts = "STATIONS_ID;MESS_DATUM;  QN;FX_10;FNX_10;FMX_10;DX_10;eor\n\
       4177;202610110000;    2;   5.3;   2.4;   3.8; 230;eor\n\
       4177;202610110810;    2;   6.1;   2.4;   4.7; 230;eor\n";
        let obs = merge_dwd(wind, gusts);
        assert_eq!(obs.len(), 3);
        assert_eq!(obs[0].at, 1_791_676_800);
        assert_eq!(
            (obs[0].wind, obs[0].gust, obs[0].dir),
            (Some(7.4), Some(10.3), Some(230.0))
        );
        assert_eq!((obs[1].wind, obs[1].gust), (Some(8.4), Some(11.9)));
        assert_eq!((obs[2].wind, obs[2].gust, obs[2].dir), (None, None, None));
        assert!(merge_dwd(wind, "").iter().all(|o| o.gust.is_none()));
    }
}
