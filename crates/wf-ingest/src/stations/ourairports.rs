//! Airports with a weather report: names and places from OurAirports, which of them send
//! METARs and TAFs from aviationweather.gov's station list.

use std::collections::HashMap;
use std::io::Read;

use serde::Deserialize;
use wf_core::station::{NewStation, Station, StationSource};

use super::{Error, Result, get_bytes, get_text};

pub async fn fetch_airports(
    http: &reqwest::Client,
    base: &str,
    awc_stations_url: &str,
) -> Result<Vec<NewStation>> {
    let base = base.trim_end_matches('/');
    let what = "stations.cache.json.gz";
    let gz = get_bytes(http, awc_stations_url, what).await?;
    let mut json = String::new();
    flate2::read::GzDecoder::new(&gz[..])
        .read_to_string(&mut json)
        .map_err(|source| Error::Unpack { what, source })?;
    let reporting = parse_awc_stations(&json).map_err(|source| Error::Json { what, source })?;
    let airports = get_text(http, &format!("{base}/airports.csv"), "airports.csv").await?;
    let regions = get_text(http, &format!("{base}/regions.csv"), "regions.csv").await?;
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
pub fn parse_awc_stations(json: &str) -> serde_json::Result<HashMap<String, bool>> {
    let list: Vec<AwcStation> = serde_json::from_str(json)?;
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
        .filter_map(std::result::Result::ok)
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
        return Err(Error::Empty("airports"));
    }
    Ok(out)
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
    fn coverage() {
        assert!(in_coverage("EU", "DE", 48.8, 8.1));
        assert!(in_coverage("AS", "GR", 36.4, 28.1)); // Rhodes
        assert!(in_coverage("AF", "ES", 28.0, -15.4)); // Gran Canaria
        assert!(!in_coverage("AS", "IR", 35.4, 51.2));
        assert!(!in_coverage("EU", "RU", 55.0, 60.0)); // east of the Urals
    }
}
