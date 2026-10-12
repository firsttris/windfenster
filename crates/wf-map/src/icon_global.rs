//! ICON global from DWD Open Data: one bz2-compressed GRIB2 file per run, hour and quantity on
//! the model's icosahedral grid (R03B07, 2,949,120 triangles of about 13 km), under
//! `weather/nwp/icon/grib/{HH}/{var}/`; the cells' centres come as the time-invariant fields
//! CLAT and CLON (degrees) in each run's `clat/` and `clon/`. The map takes the 00 and 12 UTC
//! runs, hourly to +78 h and three-hourly to +120 h (ECMWF goes on from there), and puts each
//! field on a regular 0.125° grid, every point taking the value of the nearest cell.

pub const BASE: &str = "https://opendata.dwd.de/weather/nwp/icon/grib";
pub const MODEL: &str = "icon_global";
/// Hourly to here, …
pub const HOURLY_TO: u32 = 78;
/// … three-hourly to here
pub const LAST_STEP: u32 = 120;
/// The runs taken: 00 and 12 UTC (06 and 18 UTC would double the downloads)
pub const EVERY_S: i64 = 12 * 3_600;

pub const U: &str = "u_10m";
pub const V: &str = "v_10m";
/// The strongest gust since the last output hour, m/s
pub const GUST: &str = "vmax_10m";
pub const CLAT: &str = "clat";
pub const CLON: &str = "clon";

/// The regular grid the map uses: 0.125° (about the model's 13 km), rows south to north
pub const STEP_DEG: f64 = 0.125;
pub const NX: usize = 2_880;
pub const NY: usize = 1_441;

pub fn file_name(run_at: i64, step: u32, var: &str) -> String {
    format!(
        "icon_global_icosahedral_single-level_{}_{step:03}_{}.grib2.bz2",
        crate::store::run_id(run_at),
        var.to_ascii_uppercase()
    )
}

/// The file with the cells' centres (`CLAT` or `CLON`) of a run
pub fn invariant_name(run_at: i64, var: &str) -> String {
    format!(
        "icon_global_icosahedral_time-invariant_{}_{}.grib2.bz2",
        crate::store::run_id(run_at),
        var.to_ascii_uppercase()
    )
}

pub fn folder_url(base: &str, run_at: i64, var: &str) -> String {
    format!(
        "{}/{:02}/{var}/",
        base.trim_end_matches('/'),
        run_at.rem_euclid(86_400) / 3_600
    )
}

/// The hours the map takes
pub fn wanted(step: u32) -> bool {
    step <= HOURLY_TO || (step <= LAST_STEP && step.is_multiple_of(3))
}

/// The hours of a run listed in a folder's index page, of those the map takes.
pub fn listed_steps(html: &str, run_at: i64, var: &str) -> Vec<u32> {
    let prefix = format!(
        "icon_global_icosahedral_single-level_{}_",
        crate::store::run_id(run_at)
    );
    let suffix = format!("_{}.grib2.bz2", var.to_ascii_uppercase());
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
        .filter(|s| wanted(*s))
        .collect();
    steps.sort_unstable();
    steps.dedup();
    steps
}

/// The runs that may be out at `now`, newest first: the last two days'.
pub fn candidate_runs(now: i64) -> Vec<i64> {
    let newest = now - now.rem_euclid(EVERY_S);
    (0..4).map(|k| newest - k * EVERY_S).collect()
}

/// Size of the buckets the cells are sorted into, degrees
const BUCKET_DEG: f64 = 0.5;

/// No cell near a point (never with the model's own centres, which lie 0.12° apart)
pub const NONE: u32 = u32::MAX;

/// For each point of the regular grid (rows south to north, each west to east), the index of
/// the nearest cell, or `NONE`. Cells go into 0.5° buckets; a point looks at its bucket and the
/// ones around it (wider towards the poles, where a degree of longitude shrinks).
pub fn nearest_cells(clat: &[f32], clon: &[f32]) -> Vec<u32> {
    let bx = (360.0 / BUCKET_DEG) as usize;
    let by = (180.0 / BUCKET_DEG) as usize;
    let bucket = |lat: f64, lon: f64| {
        let x = (((lon + 180.0).rem_euclid(360.0)) / BUCKET_DEG) as usize;
        let y = ((lat + 90.0) / BUCKET_DEG) as usize;
        (x.min(bx - 1), y.min(by - 1))
    };
    // Cells per bucket, as one list with offsets
    let mut count = vec![0u32; bx * by + 1];
    for (&lat, &lon) in clat.iter().zip(clon) {
        let (x, y) = bucket(f64::from(lat), f64::from(lon));
        count[y * bx + x + 1] += 1;
    }
    for i in 1..count.len() {
        count[i] += count[i - 1];
    }
    let mut fill = count.clone();
    let mut cells = vec![0u32; clat.len()];
    for (i, (&lat, &lon)) in clat.iter().zip(clon).enumerate() {
        let (x, y) = bucket(f64::from(lat), f64::from(lon));
        let slot = &mut fill[y * bx + x];
        cells[*slot as usize] = i as u32;
        *slot += 1;
    }
    let mut out = Vec::with_capacity(NX * NY);
    for iy in 0..NY {
        let lat = -90.0 + iy as f64 * STEP_DEG;
        let cos = lat.to_radians().cos().max(1e-3);
        // Buckets enough to cover half a degree east and west
        let rx = ((1.0 / cos).ceil() as usize).min(bx / 2);
        for ix in 0..NX {
            let lon = -180.0 + ix as f64 * STEP_DEG;
            let (cx, cy) = bucket(lat, lon);
            let mut best = (f64::MAX, NONE);
            for y in cy.saturating_sub(1)..=(cy + 1).min(by - 1) {
                for dx in -(rx as isize)..=(rx as isize) {
                    let x = (cx as isize + dx).rem_euclid(bx as isize) as usize;
                    let b = y * bx + x;
                    for &c in &cells[count[b] as usize..count[b + 1] as usize] {
                        let dlat = f64::from(clat[c as usize]) - lat;
                        let dlon =
                            (f64::from(clon[c as usize]) - lon + 540.0).rem_euclid(360.0) - 180.0;
                        let d = dlat * dlat + (dlon * cos) * (dlon * cos);
                        if d < best.0 {
                            best = (d, c);
                        }
                    }
                }
            }
            out.push(best.1);
        }
    }
    out
}

/// A field on the regular grid, from the values of the cells.
pub fn regrid(values: &[f32], nearest: &[u32]) -> crate::grid::LatLonGrid {
    crate::grid::LatLonGrid {
        lon0: -180.0,
        lat0: -90.0,
        dlon: STEP_DEG,
        dlat: STEP_DEG,
        nx: NX,
        ny: NY,
        values: nearest
            .iter()
            .map(|&c| values.get(c as usize).copied().unwrap_or(f32::NAN))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_hours() {
        let run = 1_791_763_200; // 2026-10-12 00 UTC
        assert_eq!(
            file_name(run, 12, U),
            "icon_global_icosahedral_single-level_2026101200_012_U_10M.grib2.bz2"
        );
        assert_eq!(
            invariant_name(run, CLAT),
            "icon_global_icosahedral_time-invariant_2026101200_CLAT.grib2.bz2"
        );
        assert_eq!(
            folder_url(BASE, run + 12 * 3_600, V),
            format!("{BASE}/12/v_10m/")
        );
        // Lines of the real index of grib/00/u_10m/ (2026-10-12)
        let html = r#"<a href="icon_global_icosahedral_single-level_2026101200_000_U_10M.grib2.bz2">x</a>
<a href="icon_global_icosahedral_single-level_2026101200_001_U_10M.grib2.bz2">x</a>
<a href="icon_global_icosahedral_single-level_2026101200_081_U_10M.grib2.bz2">x</a>
<a href="icon_global_icosahedral_single-level_2026101200_123_U_10M.grib2.bz2">x</a>
<a href="icon_global_icosahedral_single-level_2026101112_002_U_10M.grib2.bz2">x</a>"#;
        assert_eq!(listed_steps(html, run, U), [0, 1, 81]);
        assert!((0..=120).filter(|s| wanted(*s)).count() == 93);
        let c = candidate_runs(run + 5 * 3_600);
        assert_eq!((c[0], c[1]), (run, run - EVERY_S));
    }

    #[test]
    fn every_point_takes_the_nearest_cell() {
        // Four cells: near Baden-Baden, near Rhodes, by the date line and near the south pole
        let clat = [48.78, 36.4, 10.0, -89.9];
        let clon = [8.08, 28.1, 179.95, 0.0];
        let near = nearest_cells(&clat, &clon);
        assert_eq!(near.len(), NX * NY);
        let at = |lat: f64, lon: f64| {
            let iy = ((lat + 90.0) / STEP_DEG).round() as usize;
            let ix = ((lon + 180.0) / STEP_DEG).round() as usize % NX;
            near[iy * NX + ix]
        };
        assert_eq!(at(48.75, 8.125), 0);
        assert_eq!(at(36.375, 28.125), 1);
        // Across the date line
        assert_eq!(at(10.0, -180.0), 2);
        assert_eq!(at(-90.0, 90.0), 3);
        // Far from every cell: none
        assert_eq!(at(0.0, 100.0), NONE);
        let g = regrid(&[1.0, 2.0, 3.0, 4.0], &near);
        assert!(g.wraps());
        assert!(g.get(0, NY / 2).is_nan());
        assert_eq!(
            g.get(
                ((8.125 + 180.0) / STEP_DEG) as usize,
                ((48.75 + 90.0) / STEP_DEG) as usize
            ),
            1.0
        );
    }
}
