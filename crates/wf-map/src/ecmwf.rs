//! ECMWF open data (CC BY 4.0): the IFS at 0.25°, a run every six hours, one GRIB2 file per
//! step with a JSON index of its fields under `{YYYYMMDD}/{HH}z/ifs/0p25/oper/`. The wind at
//! 10 m is fetched by byte range (two fields of about 0.7 MB instead of the whole file) and cut
//! to the map's region. Steps come out in order, a few hours after the run.

use serde::Deserialize;

pub const BASE: &str = "https://data.ecmwf.int/forecasts";
pub const MODEL: &str = "ecmwf_ifs025";
/// Hours after the run kept for the map: five days, every three hours
pub const LAST_STEP: u32 = 120;
pub const STEP_H: u32 = 3;
pub const EVERY_S: i64 = 6 * 3_600;

pub const U: &str = "10u";
pub const V: &str = "10v";
/// The strongest gust since the step before, m/s
pub const GUST: &str = "10fg";

/// The run as its file names give it: YYYYMMDDHH0000
fn stamp(run_at: i64) -> String {
    format!("{}0000", crate::store::run_id(run_at))
}

pub fn folder_url(base: &str, run_at: i64) -> String {
    format!(
        "{}/{}/{:02}z/ifs/0p25/oper/",
        base.trim_end_matches('/'),
        wf_core::time::yyyymmdd(run_at),
        run_at.rem_euclid(86_400) / 3_600
    )
}

/// The step's GRIB2 file (`grib2`) or its index (`index`).
pub fn file_url(base: &str, run_at: i64, step: u32, ext: &str) -> String {
    format!(
        "{}{}-{step}h-oper-fc.{ext}",
        folder_url(base, run_at),
        stamp(run_at)
    )
}

/// The steps the map takes, in the order they come out.
pub fn steps() -> impl Iterator<Item = u32> {
    (0..=LAST_STEP).step_by(STEP_H as usize)
}

/// The runs that may be out at `now`, newest first: the last two days'.
pub fn candidate_runs(now: i64) -> Vec<i64> {
    let newest = now - now.rem_euclid(EVERY_S);
    (0..8).map(|k| newest - k * EVERY_S).collect()
}

/// Where a field lies in the step's GRIB2 file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub offset: u64,
    pub length: u64,
}

impl Range {
    /// The HTTP Range header for it.
    pub fn header(&self) -> String {
        format!("bytes={}-{}", self.offset, self.offset + self.length - 1)
    }
}

#[derive(Deserialize)]
struct Entry {
    param: String,
    #[serde(default)]
    levtype: Option<String>,
    #[serde(rename = "_offset")]
    offset: u64,
    #[serde(rename = "_length")]
    length: u64,
}

/// A surface field's place in the file, from the index (one JSON object per line).
pub fn field_range(index: &str, param: &str) -> Option<Range> {
    index
        .lines()
        .filter_map(|l| serde_json::from_str::<Entry>(l).ok())
        .find(|e| e.param == param && e.levtype.as_deref().is_none_or(|t| t == "sfc"))
        .map(|e| Range {
            offset: e.offset,
            length: e.length,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_wind_in_the_index() {
        // Lines of the real index of 20261011000000-3h-oper-fc
        let index = r#"{"domain": "g", "date": "20261011", "time": "0000", "expver": "0001", "class": "od", "type": "fc", "stream": "oper", "levtype": "sfc", "step": "3", "param": "tp", "_offset": 0, "_length": 603297}
{"domain": "g", "date": "20261011", "time": "0000", "expver": "0001", "class": "od", "type": "fc", "stream": "oper", "step": "3", "levtype": "sfc", "param": "10u", "_offset": 9916301, "_length": 740364}
{"domain": "g", "date": "20261011", "time": "0000", "expver": "0001", "class": "od", "type": "fc", "stream": "oper", "step": "3", "levtype": "sfc", "param": "10v", "_offset": 14316608, "_length": 736385}"#;
        let u = field_range(index, U).unwrap();
        assert_eq!(u.header(), "bytes=9916301-10656664");
        assert_eq!(field_range(index, V).unwrap().offset, 14_316_608);
        assert_eq!(field_range(index, "2t"), None);
        let run = 1_791_676_800; // 2026-10-11 00 UTC
        assert_eq!(
            file_url(BASE, run, 3, "index"),
            "https://data.ecmwf.int/forecasts/20261011/00z/ifs/0p25/oper/20261011000000-3h-oper-fc.index"
        );
        assert_eq!(steps().count(), 41);
        let c = candidate_runs(run + 5 * 3_600);
        assert_eq!((c[0], c[1]), (run, run - EVERY_S));
    }
}
