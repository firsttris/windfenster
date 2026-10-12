//! A field on a regular latitude/longitude grid, decoded from a GRIB2 message.

use grib::{GridDefinitionTemplateValues, GridPointIndex};

#[derive(Debug, thiserror::Error)]
pub enum GridError {
    #[error("GRIB2: {0}")]
    Grib(#[from] grib::GribError),
    #[error("the file holds no GRIB2 message")]
    Empty,
    #[error("unsupported grid: {0}")]
    Unsupported(&'static str),
    #[error("{values} values for {points} grid points")]
    Size { values: usize, points: usize },
}

/// Values on a regular grid: rows from south to north, each from west to east; NaN where the
/// model has no value (outside its domain).
#[derive(Debug, Clone, PartialEq)]
pub struct LatLonGrid {
    /// Longitude of the first column (degrees east, −180…180) and latitude of the first row
    pub lon0: f64,
    pub lat0: f64,
    pub dlon: f64,
    pub dlat: f64,
    pub nx: usize,
    pub ny: usize,
    pub values: Vec<f32>,
}

impl LatLonGrid {
    pub fn get(&self, ix: usize, iy: usize) -> f32 {
        self.values[iy * self.nx + ix]
    }

    /// The model's domain as a ring of (lon, lat) points: the westernmost and easternmost value
    /// of every `every`-th row, west side northwards, east side back south. Exact for convex
    /// domains such as ICON-D2's rotated rectangle.
    pub fn outline(&self, every: usize) -> Vec<(f64, f64)> {
        let rows = (0..self.ny)
            .step_by(every.max(1))
            .chain((self.ny > 0).then_some(self.ny - 1))
            .collect::<std::collections::BTreeSet<_>>();
        let (mut west, mut east) = (Vec::new(), Vec::new());
        for iy in rows {
            let row = &self.values[iy * self.nx..(iy + 1) * self.nx];
            let (Some(a), Some(b)) = (
                row.iter().position(|v| v.is_finite()),
                row.iter().rposition(|v| v.is_finite()),
            ) else {
                continue;
            };
            let lat = self.lat0 + iy as f64 * self.dlat;
            west.push((self.lon0 + a as f64 * self.dlon, lat));
            east.push((self.lon0 + b as f64 * self.dlon, lat));
        }
        west.extend(east.into_iter().rev());
        west
    }

    /// Whether the rows go once around the world, so the last column's east neighbour is the
    /// first (a global model such as ECMWF's IFS).
    pub fn wraps(&self) -> bool {
        (self.nx as f64 * self.dlon - 360.0).abs() < 1e-6
    }

    /// Whether two grids lie on the same points.
    pub fn same_points(&self, other: &Self) -> bool {
        (self.nx, self.ny) == (other.nx, other.ny)
            && (self.lon0 - other.lon0).abs() < 1e-9
            && (self.lat0 - other.lat0).abs() < 1e-9
    }
}

/// −180 ≤ lon < 180, to the millionth of a degree GRIB2 counts in
fn normalize_lon(lon: f64) -> f64 {
    (((lon + 180.0).rem_euclid(360.0) - 180.0) * 1e6).round() / 1e6
}

/// Decodes the first message of a GRIB2 file on a regular latitude/longitude grid
/// (template 3.0), whatever direction it scans in.
pub fn decode(bytes: &[u8]) -> Result<LatLonGrid, GridError> {
    let grib2 = grib::from_bytes(bytes)?;
    let (_, msg) = grib2.iter().next().ok_or(GridError::Empty)?;
    let def = GridDefinitionTemplateValues::try_from(msg.grid_def())?;
    let GridDefinitionTemplateValues::Template0(t) = &def else {
        return Err(GridError::Unsupported(
            "not a regular latitude/longitude grid",
        ));
    };
    let ll = &t.lat_lon;
    let g = &ll.grid;
    let scan = def.scanning_mode();
    if !scan.is_consecutive_for_i() || scan.scans_alternating_rows() {
        return Err(GridError::Unsupported("rows not consecutive"));
    }
    // Angles in millionths of a degree unless a basic angle is given (it rarely is).
    let unit = match (
        g.initial_production_domain_basic_angle,
        g.basic_angle_subdivisions,
    ) {
        (0, _) | (_, 0) | (_, u32::MAX) => 1e-6,
        (angle, sub) => f64::from(angle) / f64::from(sub),
    };
    let (nx, ny) = (g.ni as usize, g.nj as usize);
    let dlon = f64::from(ll.i_direction_inc) * unit;
    let dlat = f64::from(ll.j_direction_inc) * unit;
    let (lat1, lat2) = (
        f64::from(g.first_point_lat) * unit,
        f64::from(g.last_point_lat) * unit,
    );
    let lon1 = f64::from(g.first_point_lon) * unit;
    let north_up = scan.scans_positively_for_j();
    let east = scan.scans_positively_for_i();
    let raw: Vec<f32> = grib::Grib2SubmessageDecoder::from(msg)?
        .dispatch()?
        .collect();
    if raw.len() != nx * ny {
        return Err(GridError::Size {
            values: raw.len(),
            points: nx * ny,
        });
    }
    let mut values = vec![f32::NAN; nx * ny];
    for j in 0..ny {
        let iy = if north_up { j } else { ny - 1 - j };
        for i in 0..nx {
            let ix = if east { i } else { nx - 1 - i };
            values[iy * nx + ix] = raw[j * nx + i];
        }
    }
    let lon_west = if east {
        lon1
    } else {
        lon1 - dlon * (nx - 1) as f64
    };
    Ok(LatLonGrid {
        lon0: normalize_lon(lon_west),
        lat0: lat1.min(lat2),
        dlon,
        dlat,
        nx,
        ny,
        values,
    })
}

/// The values of the first message of a GRIB2 file, in the file's order, whatever its grid: for
/// ICON global's triangles (template 3.101), whose places come as fields of their own.
pub fn decode_values(bytes: &[u8]) -> Result<Vec<f32>, GridError> {
    let grib2 = grib::from_bytes(bytes)?;
    let (_, msg) = grib2.iter().next().ok_or(GridError::Empty)?;
    Ok(grib::Grib2SubmessageDecoder::from(msg)?
        .dispatch()?
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::message;

    #[test]
    fn decodes_both_scanning_directions() {
        // 3 × 2 points, 0.5° apart, from 10°E 50°N; ICON-D2 scans northwards (0x40).
        let values = [Some(1), Some(2), Some(3), Some(4), None, Some(6)];
        let up = message(
            3,
            2,
            (50_000_000, 10_000_000),
            (50_500_000, 11_000_000),
            500_000,
            0x40,
            &values,
        );
        let g = decode(&up).unwrap();
        assert_eq!(
            (g.nx, g.ny, g.lon0, g.lat0, g.dlon, g.dlat),
            (3, 2, 10.0, 50.0, 0.5, 0.5)
        );
        assert_eq!((g.get(0, 0), g.get(2, 1)), (1.0, 6.0));
        assert!(g.get(1, 1).is_nan());
        // ECMWF scans southwards (0x00): the first row is the northern one.
        let down = message(
            3,
            2,
            (50_500_000, 359_000_000),
            (50_000_000, 0),
            500_000,
            0x00,
            &values,
        );
        let g = decode(&down).unwrap();
        assert_eq!((g.lon0, g.lat0), (-1.0, 50.0));
        assert_eq!((g.get(0, 1), g.get(0, 0)), (1.0, 4.0));
    }

    #[test]
    fn knows_a_global_grid() {
        let grid = |nx: usize, dlon: f64| LatLonGrid {
            lon0: -180.0,
            lat0: -90.0,
            dlon,
            dlat: 0.25,
            nx,
            ny: 721,
            values: vec![],
        };
        // ECMWF's 0.25° grid: 1440 columns, the 1441st would repeat the first.
        assert!(grid(1440, 0.25).wraps());
        assert!(!grid(1441, 0.25).wraps());
        assert!(!grid(281, 0.25).wraps());
    }

    #[test]
    fn outlines_the_domain() {
        // A diamond of values in a 5 × 3 grid
        let n = f32::NAN;
        let g = LatLonGrid {
            lon0: 0.0,
            lat0: 40.0,
            dlon: 1.0,
            dlat: 1.0,
            nx: 5,
            ny: 3,
            values: vec![n, n, 1.0, n, n, 1.0, 1.0, 1.0, 1.0, 1.0, n, n, 1.0, n, n],
        };
        assert_eq!(
            g.outline(1),
            [
                (2.0, 40.0),
                (0.0, 41.0),
                (2.0, 42.0),
                (2.0, 42.0),
                (4.0, 41.0),
                (2.0, 40.0)
            ]
        );
        // Every second row, and the last one
        assert_eq!(g.outline(2).len(), 4);
    }
}
