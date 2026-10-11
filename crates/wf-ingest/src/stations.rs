//! Station lists: airports with a weather report (OurAirports) and the DWD's wind stations.

use std::collections::HashMap;

use anyhow::{Context, Result};
use serde::Deserialize;
use sqlx::SqlitePool;
use wf_core::station::{self, NewStation, Station, StationSource};

/// OurAirports' data (public domain), mirrored on GitHub.
pub const OURAIRPORTS_BASE: &str =
    "https://raw.githubusercontent.com/davidmegginson/ourairports-data/main";

/// DWD stations with 10-minute wind values (current year).
pub const DWD_STATIONS_URL: &str = "https://opendata.dwd.de/climate_environment/CDC/observations_germany/climate/10_minutes/wind/now/zehn_now_ff_Beschreibung_Stationen.txt";

/// Lists are re-imported once a week.
pub const IMPORT_EVERY_S: i64 = 7 * 86_400;

#[derive(Debug, Clone)]
pub struct Sources {
    pub ourairports_base: String,
    pub dwd_url: String,
}

impl Default for Sources {
    fn default() -> Self {
        Self {
            ourairports_base: OURAIRPORTS_BASE.into(),
            dwd_url: DWD_STATIONS_URL.into(),
        }
    }
}

/// Imports both lists; each one failing on its own is logged, not fatal.
pub async fn import_all(db: &SqlitePool, http: &reqwest::Client, sources: &Sources) -> Result<()> {
    let now = wf_core::time::now();
    let mut errors = Vec::new();
    match fetch_airports(http, &sources.ourairports_base).await {
        Ok(list) => {
            let n = station::upsert(db, &list, now).await?;
            tracing::info!("imported {n} airport stations (OurAirports)");
        }
        Err(e) => errors.push(format!("OurAirports: {e:#}")),
    }
    match fetch_dwd(http, &sources.dwd_url, now).await {
        Ok(list) => {
            let n = station::upsert(db, &list, now).await?;
            tracing::info!("imported {n} DWD wind stations");
        }
        Err(e) => errors.push(format!("DWD: {e:#}")),
    }
    if errors.is_empty() {
        Ok(())
    } else {
        anyhow::bail!("station import: {}", errors.join("; "))
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

async fn get_text(http: &reqwest::Client, url: &str) -> Result<String> {
    let res = http.get(url).send().await?.error_for_status()?;
    Ok(decode(&res.bytes().await?))
}

/// UTF-8, or Latin-1 as older DWD files use.
fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

// ---- OurAirports ----------------------------------------------------------------------

pub async fn fetch_airports(http: &reqwest::Client, base: &str) -> Result<Vec<NewStation>> {
    let base = base.trim_end_matches('/');
    let airports = get_text(http, &format!("{base}/airports.csv"))
        .await
        .context("airports.csv")?;
    let regions = get_text(http, &format!("{base}/regions.csv"))
        .await
        .context("regions.csv")?;
    parse_airports(&airports, &regions)
}

#[derive(Deserialize)]
struct AirportRow {
    #[serde(rename = "type")]
    kind: String,
    name: String,
    latitude_deg: f64,
    longitude_deg: f64,
    elevation_ft: Option<f64>,
    continent: String,
    iso_country: String,
    iso_region: String,
    municipality: String,
    #[serde(default)]
    icao_code: String,
    gps_code: String,
    keywords: String,
}

#[derive(Deserialize)]
struct RegionRow {
    code: String,
    name: String,
}

/// Countries outside OurAirports' "EU" continent whose coasts and islands are in scope
/// (Rhodes counts as Asia there, the Canaries as Africa).
const MEDITERRANEAN_EXTRA: [&str; 10] =
    ["GR", "CY", "TR", "ES", "PT", "IT", "MA", "TN", "EG", "IL"];

/// Europe including the Mediterranean, the Canaries and the Azores.
fn in_coverage(continent: &str, country: &str, lat: f64, lon: f64) -> bool {
    (27.0..=72.0).contains(&lat)
        && (-32.0..=45.0).contains(&lon)
        && (continent == "EU" || MEDITERRANEAN_EXTRA.contains(&country))
}

fn icao(code: &str) -> Option<&str> {
    (code.len() == 4 && code.bytes().all(|b| b.is_ascii_uppercase())).then_some(code)
}

/// Large and medium airports with an ICAO code in coverage: the ones with a weather report.
pub fn parse_airports(airports_csv: &str, regions_csv: &str) -> Result<Vec<NewStation>> {
    let regions: HashMap<String, String> = csv::Reader::from_reader(regions_csv.as_bytes())
        .deserialize::<RegionRow>()
        .filter_map(Result::ok)
        .map(|r| (r.code, r.name))
        .collect();
    let mut out = Vec::new();
    for row in csv::Reader::from_reader(airports_csv.as_bytes()).deserialize::<AirportRow>() {
        let Ok(a) = row else { continue };
        if !matches!(a.kind.as_str(), "large_airport" | "medium_airport")
            || !in_coverage(
                &a.continent,
                &a.iso_country,
                a.latitude_deg,
                a.longitude_deg,
            )
        {
            continue;
        }
        let Some(code) = icao(&a.icao_code).or_else(|| icao(&a.gps_code)) else {
            continue;
        };
        let non_empty = |s: String| (!s.trim().is_empty()).then_some(s);
        out.push(NewStation {
            station: Station {
                slug: code.to_ascii_lowercase(),
                source: StationSource::Metar,
                code: code.to_owned(),
                name: a.name,
                country: non_empty(a.iso_country),
                region: regions
                    .get(&a.iso_region)
                    .filter(|r| !r.starts_with('('))
                    .cloned(),
                municipality: non_empty(a.municipality),
                lat: a.latitude_deg,
                lon: a.longitude_deg,
                elevation_m: a.elevation_ft.map(|ft| (ft * 0.3048 * 10.0).round() / 10.0),
            },
            keywords: non_empty(a.keywords),
        });
    }
    if out.is_empty() {
        anyhow::bail!("no airports found; has the CSV format changed?");
    }
    Ok(out)
}

// ---- DWD --------------------------------------------------------------------------------

pub async fn fetch_dwd(http: &reqwest::Client, url: &str, now: i64) -> Result<Vec<NewStation>> {
    let text = get_text(http, url).await.context("DWD station list")?;
    // Stations that delivered within the last 30 days.
    let list = parse_dwd(&text, wf_core::time::yyyymmdd(now - 30 * 86_400));
    if list.is_empty() {
        anyhow::bail!("no active DWD stations found; has the format changed?");
    }
    Ok(list)
}

const STATES: [&str; 16] = [
    "Baden-Württemberg",
    "Bayern",
    "Berlin",
    "Brandenburg",
    "Bremen",
    "Hamburg",
    "Hessen",
    "Mecklenburg-Vorpommern",
    "Niedersachsen",
    "Nordrhein-Westfalen",
    "Rheinland-Pfalz",
    "Saarland",
    "Sachsen",
    "Sachsen-Anhalt",
    "Schleswig-Holstein",
    "Thüringen",
];

/// Parses `zehn_now_ff_Beschreibung_Stationen.txt`: id, from, to, height, lat, lon, name,
/// state and (in newer files) a release column, separated by spaces. Names can contain
/// spaces; every state is one word, which marks where the name ends.
pub fn parse_dwd(text: &str, active_since: u32) -> Vec<NewStation> {
    let mut out = Vec::new();
    for line in text.lines() {
        let t: Vec<&str> = line.split_whitespace().collect();
        if t.len() < 8 || !t[0].bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let (Ok(to), Ok(height), Ok(lat), Ok(lon)) = (
            t[2].parse::<u32>(),
            t[3].parse::<f64>(),
            t[4].parse::<f64>(),
            t[5].parse::<f64>(),
        ) else {
            continue;
        };
        if to < active_since {
            continue;
        }
        let Some(state_at) = (6..t.len()).rev().find(|&i| STATES.contains(&t[i])) else {
            continue;
        };
        if state_at == 6 {
            continue;
        }
        let id = t[0];
        out.push(NewStation {
            station: Station {
                slug: format!("dwd-{id}"),
                source: StationSource::Dwd,
                code: id.to_owned(),
                name: t[6..state_at].join(" "),
                country: Some("DE".into()),
                region: Some(t[state_at].to_owned()),
                municipality: None,
                lat,
                lon,
                elevation_m: Some(height),
            },
            keywords: None,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn airports() {
        // Rows as in OurAirports' airports.csv (header and values from the real file).
        let airports = r#""id","ident","type","name","latitude_deg","longitude_deg","elevation_ft","continent","iso_country","iso_region","municipality","scheduled_service","icao_code","iata_code","gps_code","local_code","home_link","wikipedia_link","keywords"
2270,"EDSB","large_airport","Karlsruhe Baden-Baden Airport",48.7794,8.0805,408,"EU","DE","DE-BW","Rheinmünster","yes","EDSB","FKB","EDSB","","","","Baden Airpark"
4279,"LGRP","large_airport","Rhodes International Airport ""Diagoras""",36.405399,28.086201,17,"AS","GR","GR-L","Rhodes","yes","LGRP","RHO","LGRP","","","",""
1,"EDXX","small_airport","Small Field",48.0,8.0,500,"EU","DE","DE-BW","","no","EDXX","","EDXX","","","",""
2,"KJFK","large_airport","John F Kennedy International Airport",40.639447,-73.779317,13,"NA","US","US-NY","New York","yes","KJFK","JFK","KJFK","","","",""
3,"OIII","large_airport","Tehran Imam Khomeini",35.4,51.15,3305,"AS","IR","IR-07","Tehran","yes","OIIE","IKA","OIIE","","","",""
4,"XX01","medium_airport","No Code",48.0,9.0,,"EU","DE","DE-BW","","no","","","","","","",""
"#;
        let regions = "\"id\",\"code\",\"local_code\",\"name\",\"continent\",\"iso_country\",\"wikipedia_link\",\"keywords\"\n1,\"DE-BW\",\"BW\",\"Baden-Württemberg\",\"EU\",\"DE\",\"\",\"\"\n2,\"GR-L\",\"L\",\"South Aegean\",\"EU\",\"GR\",\"\",\"\"\n";
        let list = parse_airports(airports, regions).unwrap();
        let codes: Vec<&str> = list.iter().map(|s| s.station.code.as_str()).collect();
        assert_eq!(codes, ["EDSB", "LGRP"]);
        let edsb = &list[0];
        assert_eq!(edsb.station.slug, "edsb");
        assert_eq!(edsb.station.region.as_deref(), Some("Baden-Württemberg"));
        assert_eq!(edsb.station.municipality.as_deref(), Some("Rheinmünster"));
        assert_eq!(edsb.station.elevation_m, Some(124.4));
        assert_eq!(edsb.keywords.as_deref(), Some("Baden Airpark"));
        assert_eq!(
            list[1].station.name,
            "Rhodes International Airport \"Diagoras\""
        );
        assert!(parse_airports("\"id\",\"ident\"\n", regions).is_err());
    }

    #[test]
    fn dwd() {
        // Layout of the DWD description file (fixed columns, Latin-1 in older files).
        let text = "Stations_id von_datum bis_datum Stationshoehe geoBreite geoLaenge Stationsname Bundesland Abgabe\n\
----------- --------- --------- ------------- --------- --------- ----------------------------------------- ---------- ------\n\
04177 19970101 20261010            116     48.9726    8.3301 Rheinstetten                             Baden-Württemberg                        Frei\n\
01975 19930101 20261010             14     53.6332    9.9881 Hamburg-Fuhlsbüttel                      Hamburg                                  Frei\n\
05100 20000101 20261010            151     49.7477    6.6582 Trier-Petrisberg                         Rheinland-Pfalz                          Frei\n\
00044 20070209 20120101             44     52.9336    8.2370 Großenkneten                             Niedersachsen                            Frei\n\
03987 19920101 20261010             81     52.3813   13.0622 Potsdam Telegrafenberg                   Brandenburg\n";
        let list = parse_dwd(text, 20260901);
        let names: Vec<&str> = list.iter().map(|s| s.station.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Rheinstetten",
                "Hamburg-Fuhlsbüttel",
                "Trier-Petrisberg",
                "Potsdam Telegrafenberg"
            ]
        );
        let r = &list[0].station;
        assert_eq!((r.slug.as_str(), r.code.as_str()), ("dwd-04177", "04177"));
        assert_eq!(r.region.as_deref(), Some("Baden-Württemberg"));
        assert_eq!(
            (r.lat, r.lon, r.elevation_m),
            (48.9726, 8.3301, Some(116.0))
        );

        let latin1: Vec<u8> = "Gro\u{df}enkneten".chars().map(|c| c as u8).collect();
        assert_eq!(decode(&latin1), "Großenkneten");
        assert_eq!(decode("Großenkneten".as_bytes()), "Großenkneten");
    }

    #[test]
    fn coverage() {
        assert!(in_coverage("EU", "DE", 48.8, 8.1));
        assert!(in_coverage("AS", "GR", 36.4, 28.1)); // Rhodes
        assert!(in_coverage("AF", "ES", 28.0, -15.4)); // Gran Canaria
        assert!(!in_coverage("AS", "IR", 35.4, 51.2));
        assert!(!in_coverage("EU", "RU", 55.0, 60.0)); // east of the Urals
    }
}
