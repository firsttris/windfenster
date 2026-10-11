//! The DWD's stations with 10-minute wind values.

use wf_core::station::{NewStation, Station, StationSource};

use super::{Error, Result, get_text};

pub async fn fetch_dwd(http: &reqwest::Client, url: &str, now: i64) -> Result<Vec<NewStation>> {
    let text = get_text(http, url, "DWD station list").await?;
    // Stations that delivered within the last 30 days.
    let list = parse_dwd(&text, wf_core::time::yyyymmdd(now - 30 * 86_400));
    if list.is_empty() {
        return Err(Error::Empty("active DWD stations"));
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

        let decode = super::super::decode;
        let latin1: Vec<u8> = "Gro\u{df}enkneten".chars().map(|c| c as u8).collect();
        assert_eq!(decode(&latin1), "Großenkneten");
        assert_eq!(decode("Großenkneten".as_bytes()), "Großenkneten");
    }
}
