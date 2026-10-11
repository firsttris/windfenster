//! Station lists: airports with a weather report (OurAirports for names and places,
//! aviationweather.gov for which ones send METARs and TAFs) and the DWD's wind stations.

use std::collections::HashMap;
use std::io::Read;

use anyhow::{Context, Result};
use serde::Deserialize;
use sqlx::SqlitePool;
use wf_core::station::{self, NewStation, Station, StationSource};

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
            Err(e) => errors.push(format!("{source:?}: {e:#}")),
        }
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

pub async fn fetch_airports(
    http: &reqwest::Client,
    base: &str,
    awc_stations_url: &str,
) -> Result<Vec<NewStation>> {
    let base = base.trim_end_matches('/');
    let gz = http
        .get(awc_stations_url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await
        .context("stations.cache.json.gz")?;
    let mut json = String::new();
    flate2::read::GzDecoder::new(&gz[..])
        .read_to_string(&mut json)
        .context("unpacking stations.cache.json.gz")?;
    let reporting = parse_awc_stations(&json)?;
    let airports = get_text(http, &format!("{base}/airports.csv"))
        .await
        .context("airports.csv")?;
    let regions = get_text(http, &format!("{base}/regions.csv"))
        .await
        .context("regions.csv")?;
    parse_airports(&airports, &regions, &reporting)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AwcStation {
    icao_id: Option<String>,
    #[serde(default)]
    site_type: Vec<String>,
}

/// ICAO codes of stations that send METARs, each with whether it also sends TAFs.
pub fn parse_awc_stations(json: &str) -> Result<HashMap<String, bool>> {
    let list: Vec<AwcStation> = serde_json::from_str(json).context("stations.cache.json")?;
    let has = |s: &AwcStation, t: &str| s.site_type.iter().any(|x| x == t);
    Ok(list
        .iter()
        .filter(|s| has(s, "METAR"))
        .filter_map(|s| Some((s.icao_id.clone()?, has(s, "TAF"))))
        .collect())
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

/// Airfields in coverage that send METARs (`reporting`: ICAO code → sends TAFs too).
pub fn parse_airports(
    airports_csv: &str,
    regions_csv: &str,
    reporting: &HashMap<String, bool>,
) -> Result<Vec<NewStation>> {
    let regions: HashMap<String, String> = csv::Reader::from_reader(regions_csv.as_bytes())
        .deserialize::<RegionRow>()
        .filter_map(Result::ok)
        .map(|r| (r.code, r.name))
        .collect();
    let mut out = Vec::new();
    for row in csv::Reader::from_reader(airports_csv.as_bytes()).deserialize::<AirportRow>() {
        let Ok(a) = row else { continue };
        if !matches!(
            a.kind.as_str(),
            "large_airport" | "medium_airport" | "small_airport"
        ) || !in_coverage(
            &a.continent,
            &a.iso_country,
            a.latitude_deg,
            a.longitude_deg,
        ) {
            continue;
        }
        let Some(code) = icao(&a.icao_code).or_else(|| icao(&a.gps_code)) else {
            continue;
        };
        let Some(&has_taf) = reporting.get(code) else {
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
                has_taf,
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
                has_taf: false,
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
5,"EDFE","small_airport","Frankfurt-Egelsbach Airport",49.9608,8.6436,384,"EU","DE","DE-HE","Egelsbach","no","EDFE","","EDFE","","","",""
6,"EDNY","medium_airport","Friedrichshafen Airport",47.671299,9.51149,1367,"EU","DE","DE-BW","Friedrichshafen","yes","EDNY","FDH","EDNY","","","",""
2,"KJFK","large_airport","John F Kennedy International Airport",40.639447,-73.779317,13,"NA","US","US-NY","New York","yes","KJFK","JFK","KJFK","","","",""
3,"OIII","large_airport","Tehran Imam Khomeini",35.4,51.15,3305,"AS","IR","IR-07","Tehran","yes","OIIE","IKA","OIIE","","","",""
4,"XX01","medium_airport","No Code",48.0,9.0,,"EU","DE","DE-BW","","no","","","","","","",""
"#;
        let regions = "\"id\",\"code\",\"local_code\",\"name\",\"continent\",\"iso_country\",\"wikipedia_link\",\"keywords\"\n1,\"DE-BW\",\"BW\",\"Baden-Württemberg\",\"EU\",\"DE\",\"\",\"\"\n2,\"GR-L\",\"L\",\"South Aegean\",\"EU\",\"GR\",\"\",\"\"\n";
        // EDNY sends no METAR in this made-up list, the small field EDXX neither.
        let reporting: HashMap<String, bool> = [
            ("EDSB", true),
            ("LGRP", true),
            ("EDFE", false),
            ("KJFK", true),
            ("OIIE", true),
        ]
        .into_iter()
        .map(|(c, t)| (c.to_owned(), t))
        .collect();
        let list = parse_airports(airports, regions, &reporting).unwrap();
        let codes: Vec<&str> = list.iter().map(|s| s.station.code.as_str()).collect();
        assert_eq!(codes, ["EDSB", "LGRP", "EDFE"]);
        assert!(list[0].station.has_taf && !list[2].station.has_taf);
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
        assert!(parse_airports("\"id\",\"ident\"\n", regions, &reporting).is_err());
    }

    #[test]
    fn awc_stations() {
        // Entries as in the real stations.cache.json (2026-10-11).
        let json = r#"[{"id":"32012","icaoId":null,"iataId":null,"faaId":null,"wmoId":null,"site":"Woods Hole Stratus Wave Station","lat":19.691,"lon":-85.567,"elev":0,"state":"","country":null,"priority":4,"siteType":[]},
            {"id":"EDAC","icaoId":"EDAC","iataId":"AOC","faaId":null,"wmoId":null,"site":"Altenburg Arpt","lat":50.982,"lon":12.506,"elev":193,"state":"ID","country":"DE","priority":5,"siteType":["METAR","TAF"]},
            {"id":"EDFE","icaoId":"EDFE","iataId":null,"faaId":null,"wmoId":null,"site":"Egelsbach","lat":49.96,"lon":8.644,"elev":118,"state":"HE","country":"DE","priority":6,"siteType":["METAR"]},
            {"id":"XTAF","icaoId":"XTAF","iataId":null,"faaId":null,"wmoId":null,"site":"Only TAF","lat":1,"lon":1,"elev":1,"state":"","country":null,"priority":9,"siteType":["TAF"]}]"#;
        let r = parse_awc_stations(json).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!((r["EDAC"], r["EDFE"]), (true, false));
    }

    #[test]
    fn dwd() {
        // Lines from the real file (2026-10-11; trailing spaces cut, Latin-1 decoded), plus one
        // station that stopped reporting.
        let text = "Stations_id von_datum bis_datum Stationshoehe geoBreite geoLaenge Stationsname Bundesland Abgabe\r\n\
----------- --------- --------- ------------- --------- --------- ----------------------------------------- ---------- ------\r\n\
00011 19920917 20261011            680     47.9736    8.5205 Donaueschingen (Landeplatz)              Baden-Württemberg                        Frei\r\n\
00183 19911101 20261011             42     54.6791   13.4344 Arkona                                   Mecklenburg-Vorpommern                   Frei\r\n\
01550 19910101 20261011            719     47.4830   11.0621 Garmisch-Partenkirchen                   Bayern                                   Frei\r\n\
04177 20081028 20261011            116     48.9726    8.3301 Rheinstetten                             Baden-Württemberg                        Frei\r\n\
00044 20070209 20120101             44     52.9336    8.2370 Großenkneten                             Niedersachsen                            Frei\r\n";
        let list = parse_dwd(text, 20260901);
        let names: Vec<&str> = list.iter().map(|s| s.station.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Donaueschingen (Landeplatz)",
                "Arkona",
                "Garmisch-Partenkirchen",
                "Rheinstetten"
            ]
        );
        let r = &list[3].station;
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
