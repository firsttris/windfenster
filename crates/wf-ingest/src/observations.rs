//! Measured wind and airport forecasts: all METARs from aviationweather.gov's cache file, TAFs
//! and DWD 10-minute values for the stations people look at, and one station's recent values on
//! request.

use std::collections::BTreeMap;
use std::io::Read;

use anyhow::{Context, Result};
use serde::Deserialize;
use sqlx::SqlitePool;
use wf_core::observation::{self, Observation};
use wf_core::station::{self, Station, StationSource};
use wf_core::taf::{self, Taf, TafChange, TafCloud, TafGroup};
use wf_core::units::MS_PER_KN;

pub const AWC_BASE: &str = "https://aviationweather.gov";

/// TAFs asked for in one request.
const TAF_BATCH: usize = 40;

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
                    taf::store(&self.db, &rows).await?;
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
        parse_taf_json(&body, wf_core::time::now())
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
            taf::store(&self.db, &rows).await?;
        }
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TafJson {
    icao_id: String,
    issue_time: String,
    valid_time_from: i64,
    valid_time_to: i64,
    #[serde(rename = "rawTAF")]
    raw_taf: String,
    #[serde(default)]
    fcsts: Vec<TafGroupJson>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TafGroupJson {
    time_from: i64,
    time_to: i64,
    time_bec: Option<i64>,
    fcst_change: Option<String>,
    probability: Option<u8>,
    wdir: Option<serde_json::Value>,
    wspd: Option<f32>,
    wgst: Option<f32>,
    /// Statute miles as a number, "6+" for 6 or more, "" when not given
    visib: Option<serde_json::Value>,
    wx_string: Option<String>,
    #[serde(default)]
    clouds: Vec<CloudJson>,
}

#[derive(Deserialize)]
struct CloudJson {
    cover: String,
    base: Option<f32>,
    #[serde(rename = "type")]
    kind: Option<String>,
}

/// Parses the TAF API's JSON (`/api/data/taf?format=json`): aviationweather.gov decodes the
/// groups already. Wind in knots, visibility in statute miles (turned into metres, 6+ = 9999).
pub fn parse_taf_json(body: &[u8], fetched_at: i64) -> Result<Vec<(String, Taf)>> {
    let list: Vec<TafJson> = serde_json::from_slice(body).context("TAF JSON")?;
    Ok(list
        .into_iter()
        .filter_map(|t| {
            let issued_at = wf_core::time::parse_iso(&t.issue_time)?;
            let groups = t
                .fcsts
                .into_iter()
                .map(|g| {
                    let change = match g.fcst_change.as_deref() {
                        Some("FM") => TafChange::From,
                        Some("BECMG") => TafChange::Becoming,
                        Some("TEMPO") => TafChange::Tempo,
                        Some("PROB") => TafChange::Prob,
                        _ => TafChange::Base,
                    };
                    let variable = g.wdir.as_ref().and_then(|d| d.as_str()) == Some("VRB");
                    let visibility_m = match &g.visib {
                        Some(serde_json::Value::String(s)) if s.ends_with('+') => Some(9999.0),
                        Some(v) => v
                            .as_f64()
                            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                            .map(|mi| {
                                ((mi * 1_609.344 / 100.0).round() * 100.0).min(9999.0) as f32
                            }),
                        None => None,
                    };
                    TafGroup {
                        change,
                        probability: g.probability,
                        from: g.time_from,
                        to: g.time_to,
                        by: g.time_bec,
                        dir: g.wdir.and_then(|d| d.as_f64()).map(|d| d as f32),
                        variable,
                        wind: g.wspd,
                        gust: g.wgst,
                        visibility_m,
                        weather: g.wx_string.filter(|w| !w.is_empty()),
                        clouds: g
                            .clouds
                            .into_iter()
                            .map(|c| TafCloud {
                                cover: c.cover,
                                base_ft: c.base,
                                kind: c.kind,
                            })
                            .collect(),
                    }
                })
                .collect();
            Some((
                t.icao_id,
                Taf::new(
                    issued_at,
                    t.valid_time_from,
                    t.valid_time_to,
                    t.raw_taf,
                    groups,
                    fetched_at,
                ),
            ))
        })
        .collect())
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
    fn taf_api_rows() {
        // From the real answer of /api/data/taf?ids=EDDF,LGRP&format=json (2026-10-11), cut.
        let body = br#"[{"icaoId":"EDDF","dbPopTime":"2026-10-11T06:34:25.763Z","bulletinTime":"2026-10-11T05:00:00.000Z","issueTime":"2026-10-11T06:31:00.000Z","validTimeFrom":1791698400,"validTimeTo":1791806400,"rawTAF":"TAF AMD EDDF 110631Z 1106/1212 20005KT CAVOK TEMPO 1106/1114 BKN025 BECMG 1107/1110 24012KT BECMG 1115/1118 26007KT BECMG 1119/1122 VRB03KT TEMPO 1202/1206 BKN045 PROB30 TEMPO 1202/1207 1200 BCFG","mostRecent":1,"remarks":"","lat":50.045,"lon":8.598,"elev":113,"prior":1,"name":"Frankfurt/Main Arpt","fcsts":[
            {"timeFrom":1791698400,"timeTo":1791702000,"timeBec":null,"fcstChange":null,"probability":null,"wdir":200,"wspd":5,"wgst":null,"wshearHgt":null,"wshearDir":null,"wshearSpd":null,"visib":"6+","altim":null,"vertVis":null,"wxString":"NSW","notDecoded":null,"clouds":[{"cover":"NSC","base":null,"type":null}],"icgTurb":[],"temp":[]},
            {"timeFrom":1791698400,"timeTo":1791727200,"timeBec":null,"fcstChange":"TEMPO","probability":null,"wdir":null,"wspd":null,"wgst":null,"visib":"","wxString":null,"clouds":[{"cover":"BKN","base":2500,"type":null}]},
            {"timeFrom":1791702000,"timeTo":1791730800,"timeBec":1791712800,"fcstChange":"BECMG","probability":null,"wdir":240,"wspd":12,"wgst":null,"visib":"6+","wxString":"NSW","clouds":[{"cover":"NSC","base":null,"type":null}]},
            {"timeFrom":1791745200,"timeTo":1791806400,"timeBec":1791756000,"fcstChange":"BECMG","probability":null,"wdir":"VRB","wspd":3,"wgst":null,"visib":"6+","wxString":"NSW","clouds":[{"cover":"NSC","base":null,"type":null}]},
            {"timeFrom":1791770400,"timeTo":1791788400,"timeBec":null,"fcstChange":"TEMPO","probability":30,"wdir":null,"wspd":null,"wgst":null,"visib":0.75,"wxString":"BCFG","clouds":[]}]},
          {"icaoId":"LGRP","issueTime":"2026-10-11T05:00:00.000Z","validTimeFrom":1791698400,"validTimeTo":1791784800,"rawTAF":"TAF LGRP 110500Z 1106/1206 25008KT 9999 FEW018 TEMPO 1200/1206 SCT010 PROB30 TEMPO 1204/1206 5000 RA FEW018TCU BKN020","fcsts":[
            {"timeFrom":1791777600,"timeTo":1791784800,"timeBec":null,"fcstChange":"TEMPO","probability":30,"wdir":null,"wspd":null,"wgst":null,"visib":3.11,"wxString":"RA","clouds":[{"cover":"FEW","base":1800,"type":"TCU"},{"cover":"BKN","base":2000,"type":null}]}]}]"#;
        let tafs = parse_taf_json(body, 7).unwrap();
        assert_eq!(tafs.len(), 2);
        let (icao, t) = &tafs[0];
        assert_eq!(
            (icao.as_str(), t.issued_at, t.fetched_at),
            ("EDDF", 1_791_700_260, 7)
        );
        assert_eq!(t.groups.len(), 5);
        let base = &t.groups[0];
        assert_eq!(
            (base.change, base.dir, base.wind, base.visibility_m),
            (TafChange::Base, Some(200.0), Some(5.0), Some(9999.0))
        );
        assert_eq!(base.clouds[0].cover, "NSC");
        assert_eq!(
            (
                t.groups[1].change,
                t.groups[1].wind,
                t.groups[1].visibility_m
            ),
            (TafChange::Tempo, None, None)
        );
        let becmg = &t.groups[3];
        assert_eq!(
            (becmg.change, becmg.variable, becmg.dir, becmg.by),
            (TafChange::Becoming, true, None, Some(1_791_756_000))
        );
        let fog = &t.groups[4];
        assert_eq!(
            (fog.probability, fog.visibility_m, fog.weather.as_deref()),
            (Some(30), Some(1200.0), Some("BCFG"))
        );
        // 30 hours of 1106/1212, with the BECMG to 24012KT done by 10 UTC.
        assert_eq!(t.hours.len(), 30);
        assert_eq!(
            (t.hours[0].wind, t.hours[5].wind, t.hours[5].dir),
            (Some(5.0), Some(12.0), Some(240.0))
        );
        let lgrp = &tafs[1].1;
        assert_eq!(lgrp.groups[0].visibility_m, Some(5000.0));
        assert_eq!(lgrp.groups[0].clouds[0].kind.as_deref(), Some("TCU"));
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
