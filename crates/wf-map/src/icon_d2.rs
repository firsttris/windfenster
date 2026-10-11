//! ICON-D2 from DWD Open Data: one bz2-compressed GRIB2 file per run, hour and quantity on the
//! regular latitude/longitude grid (0.02°), under `weather/nwp/icon-d2/grib/{HH}/{var}/`.
//! A folder holds the newest run of its hour; files appear about 45 minutes after the start.

pub const BASE: &str = "https://opendata.dwd.de/weather/nwp/icon-d2/grib";
pub const MODEL: &str = "icon_d2";
/// Hours after the run: 0 to 48
pub const LAST_STEP: u32 = 48;
/// A run every three hours
pub const EVERY_S: i64 = 3 * 3_600;

pub const U: &str = "u_10m";
pub const V: &str = "v_10m";

pub fn file_name(run_at: i64, step: u32, var: &str) -> String {
    format!(
        "icon-d2_germany_regular-lat-lon_single-level_{}_{step:03}_2d_{var}.grib2.bz2",
        crate::store::run_id(run_at)
    )
}

pub fn folder_url(base: &str, run_at: i64, var: &str) -> String {
    format!(
        "{}/{:02}/{var}/",
        base.trim_end_matches('/'),
        run_at.rem_euclid(86_400) / 3_600
    )
}

/// The hours of a run listed in a folder's index page.
pub fn listed_steps(html: &str, run_at: i64, var: &str) -> Vec<u32> {
    let prefix = format!(
        "icon-d2_germany_regular-lat-lon_single-level_{}_",
        crate::store::run_id(run_at)
    );
    let suffix = format!("_2d_{var}.grib2.bz2");
    let mut steps: Vec<u32> = html
        .split("href=\"")
        .skip(1)
        .filter_map(|s| s.split('"').next())
        .filter_map(|name| {
            name.strip_prefix(prefix.as_str())?
                .strip_suffix(suffix.as_str())?
                .parse()
                .ok()
        })
        .filter(|s| *s <= LAST_STEP)
        .collect();
    steps.sort_unstable();
    steps.dedup();
    steps
}

/// The runs that may be out at `now`, newest first: the last day's.
pub fn candidate_runs(now: i64) -> Vec<i64> {
    let newest = now - now.rem_euclid(EVERY_S);
    (0..8).map(|k| newest - k * EVERY_S).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_folder_index() {
        // Lines of the real index of grib/06/v_10m/ (2026-10-11), and one of another run
        let html = r#"<h1>Index of /weather/nwp/icon-d2/grib/06/v_10m/</h1><hr><pre><a href="../">../</a>
<a href="icon-d2_germany_icosahedral_single-level_2026101106_000_2d_v_10m.grib2.bz2">icon-d2_germany_icosahedral_single-level_202610..&gt;</a> 11-Oct-2026 06:44:08              752550
<a href="icon-d2_germany_regular-lat-lon_single-level_2026101106_000_2d_v_10m.grib2.bz2">icon-d2_germany_regular-lat-lon_single-level_20..&gt;</a> 11-Oct-2026 06:44:23             1057692
<a href="icon-d2_germany_regular-lat-lon_single-level_2026101106_001_2d_v_10m.grib2.bz2">icon-d2_germany_regular-lat-lon_single-level_20..&gt;</a> 11-Oct-2026 06:47:57             1058662
<a href="icon-d2_germany_regular-lat-lon_single-level_2026101006_002_2d_v_10m.grib2.bz2">x</a>"#;
        let run = 1_791_698_400; // 2026-10-11 06 UTC
        assert_eq!(listed_steps(html, run, V), [0, 1]);
        assert!(listed_steps(html, run, U).is_empty());
        assert_eq!(
            file_name(run, 3, U),
            "icon-d2_germany_regular-lat-lon_single-level_2026101106_003_2d_u_10m.grib2.bz2"
        );
        assert_eq!(folder_url(BASE, run, U), format!("{BASE}/06/u_10m/"));
        let c = candidate_runs(run + 2 * 3_600);
        assert_eq!((c[0], c[1], c.len()), (run, run - EVERY_S, 8));
    }
}
