//! Which tiles the basemap keeps: the map's region from afar, the spots from close up.

use pmtiles::{TileCoord, TileId};

/// A box of tiles over a range of zooms.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Area {
    pub west: f64,
    pub south: f64,
    pub east: f64,
    pub north: f64,
    pub min_zoom: u8,
    pub max_zoom: u8,
}

/// The map's region (as the wind layers', `crate::REGION`) up to zoom 8: coasts, towns and main
/// roads, about 190 MB.
pub const REGION_ZOOM: u8 = 8;
/// Around a spot, up to zoom 14: streets, lakes, the beach
pub const SPOT_ZOOM: u8 = 14;
/// How far around a spot the detail reaches
pub const SPOT_KM: f64 = 20.0;

pub fn region() -> Area {
    let [west, south, east, north] = crate::REGION;
    Area {
        west,
        south,
        east,
        north,
        min_zoom: 0,
        max_zoom: REGION_ZOOM,
    }
}

/// The box `SPOT_KM` around a place, from just past the region's zoom to `SPOT_ZOOM`.
pub fn spot(lat: f64, lon: f64) -> Area {
    let dlat = SPOT_KM / 111.32;
    let dlon = dlat / lat.to_radians().cos().max(0.1);
    Area {
        west: lon - dlon,
        south: lat - dlat,
        east: lon + dlon,
        north: lat + dlat,
        min_zoom: REGION_ZOOM + 1,
        max_zoom: SPOT_ZOOM,
    }
}

/// The column and row of the tile holding a place at zoom `z` (Web Mercator).
fn tile_of(z: u8, lon: f64, lat: f64) -> (u32, u32) {
    let n = f64::from(1u32 << z);
    let lat = lat.clamp(-85.0511, 85.0511).to_radians();
    let x = ((lon + 180.0) / 360.0 * n).floor();
    let y = ((1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / std::f64::consts::PI) / 2.0 * n).floor();
    let max = n - 1.0;
    (x.clamp(0.0, max) as u32, y.clamp(0.0, max) as u32)
}

/// Whether the area holds a tile.
pub fn holds(a: &Area, z: u8, x: u32, y: u32) -> bool {
    if z < a.min_zoom || z > a.max_zoom {
        return false;
    }
    let (x0, y0) = tile_of(z, a.west, a.north);
    let (x1, y1) = tile_of(z, a.east, a.south);
    (x0..=x1).contains(&x) && (y0..=y1).contains(&y)
}

/// Every tile of the areas, once, in the order of their ids (as the archive stores them).
pub fn wanted(areas: &[Area]) -> Vec<TileCoord> {
    let mut ids: Vec<u64> = Vec::new();
    for a in areas {
        for z in a.min_zoom..=a.max_zoom {
            let (x0, y0) = tile_of(z, a.west, a.north);
            let (x1, y1) = tile_of(z, a.east, a.south);
            for x in x0..=x1 {
                for y in y0..=y1 {
                    if let Ok(c) = TileCoord::new(z, x, y) {
                        ids.push(TileId::from(c).value());
                    }
                }
            }
        }
    }
    ids.sort_unstable();
    ids.dedup();
    ids.into_iter()
        .filter_map(|id| TileId::new(id).ok().map(TileCoord::from))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_tiles_of_a_place() {
        // Baden Airpark at zoom 12: x = (8.08 + 180) / 360 · 4096 = 2139.9,
        // y = (1 − asinh(tan 48.78°) / π) / 2 · 4096 = 1410.5
        assert_eq!(tile_of(12, 8.08, 48.78), (2139, 1410));
        assert_eq!(tile_of(0, 8.08, 48.78), (0, 0));
        assert_eq!(tile_of(2, -179.9, 85.0), (0, 0));
    }

    #[test]
    fn keeps_each_tile_once_in_id_order() {
        let a = Area {
            west: 7.9,
            south: 48.7,
            east: 8.2,
            north: 48.9,
            min_zoom: 0,
            max_zoom: 3,
        };
        let tiles = wanted(&[a, a]);
        // One tile per zoom up to 3 for so small a box
        assert_eq!(tiles.len(), 4);
        let ids: Vec<u64> = tiles.iter().map(|c| TileId::from(*c).value()).collect();
        assert!(ids.windows(2).all(|w| w[0] < w[1]));
        // A spot is 40 km wide: at zoom 14 (1.6 km a tile there) some 25 × 25 tiles
        let s = wanted(&[Area {
            min_zoom: 14,
            ..spot(48.78, 8.08)
        }]);
        assert!((500..900).contains(&s.len()), "{}", s.len());
        let airpark = spot(48.78, 8.08);
        assert!(holds(&airpark, 12, 2139, 1410));
        assert!(!holds(&airpark, 12, 2200, 1410));
        assert!(!holds(&airpark, 8, 133, 88));
    }
}
