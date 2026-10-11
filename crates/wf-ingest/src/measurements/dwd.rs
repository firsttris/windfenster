//! The DWD's 10-minute products: mean wind and direction, and the gusts, one zip each.

use std::collections::BTreeMap;
use std::io::Read;

use wf_core::observation::Observation;
use wf_core::units::MS_PER_KN;

/// The first file of a zip archive, read as Latin-1 (the DWD's encoding).
pub(super) fn unzip_first(bytes: &[u8]) -> Result<String, zip::result::ZipError> {
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
