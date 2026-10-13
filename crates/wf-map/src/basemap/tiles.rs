//! Which tiles the basemap keeps: the whole world from afar, a region deeper, the spots from
//! close up.

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

impl Area {
    pub fn bounds(&self) -> [f64; 4] {
        [self.west, self.south, self.east, self.north]
    }
}

/// The world up to zoom 7 by default: coasts, countries, towns (`WF_BASEMAP_WORLD_ZOOM`)
pub const WORLD_ZOOM: u8 = 7;
/// The region (Europe by default, `WF_BASEMAP_REGION`) up to zoom 8 by default: coasts, towns
/// and main roads, about 190 MB (`WF_BASEMAP_ZOOM`)
pub const REGION_ZOOM: u8 = 8;
/// Around a spot, up to zoom 14: streets, lakes, the beach
pub const SPOT_ZOOM: u8 = 14;
/// How far around a spot the detail reaches
pub const SPOT_KM: f64 = 20.0;
/// Web Mercator's edge, degrees north and south
pub const MAX_LAT: f64 = 85.051_129;

/// The whole world up to `zoom`.
pub fn world(zoom: u8) -> Area {
    Area {
        west: -180.0,
        south: -MAX_LAT,
        east: 180.0,
        north: MAX_LAT,
        min_zoom: 0,
        max_zoom: zoom,
    }
}

/// A box (west, south, east, north) up to `zoom`.
pub fn region([west, south, east, north]: [f64; 4], zoom: u8) -> Area {
    Area {
        west,
        south,
        east,
        north,
        min_zoom: 0,
        max_zoom: zoom,
    }
}

/// The box `SPOT_KM` around a place, from `from_zoom` (just past the world's) to `SPOT_ZOOM`.
pub fn spot(lat: f64, lon: f64, from_zoom: u8) -> Area {
    let dlat = SPOT_KM / 111.32;
    let dlon = dlat / lat.to_radians().cos().max(0.1);
    Area {
        west: lon - dlon,
        south: lat - dlat,
        east: lon + dlon,
        north: lat + dlat,
        min_zoom: from_zoom,
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

/// Whether the areas hold all of an archive that goes to `source_max_zoom`: one of them is
/// the whole world (to a hundredth of a degree, as typed in `WF_BASEMAP_REGION`) from zoom 0
/// at least that deep. Then the archive is fetched whole instead of tile by tile.
pub fn whole_source(areas: &[Area], source_max_zoom: u8) -> bool {
    const EDGE: f64 = 0.01;
    areas.iter().any(|a| {
        a.min_zoom == 0
            && a.max_zoom >= source_max_zoom
            && a.west <= -180.0 + EDGE
            && a.east >= 180.0 - EDGE
            && a.south <= -MAX_LAT + EDGE
            && a.north >= MAX_LAT - EDGE
    })
}

/// Every tile of the areas, once, in the order of their ids (as the archive stores them),
/// made one by one: the world at zoom 15 is over a billion tiles, too many to list first (the
/// list of ids took 12.5 GB before the kernel ended the server).
pub fn wanted(areas: &[Area]) -> impl Iterator<Item = TileCoord> + '_ {
    let max = areas.iter().map(|a| a.max_zoom).max().unwrap_or(0);
    (0..=max).flat_map(move |z| Level::new(areas, z))
}

/// The wanted tiles of one zoom, by going down the quadtree. PMTiles numbers a zoom's tiles
/// along a Hilbert curve, which runs through every aligned square of tiles in one stretch of
/// ids: a square the areas leave out is skipped whole, one they cover whole is a run of ids,
/// and only squares on an area's edge are split. The work follows the tiles made, not the
/// zoom's size.
struct Level {
    z: u8,
    /// The areas' tiles at this zoom: x0, y0, x1, y1, inclusive
    boxes: Vec<(u32, u32, u32, u32)>,
    /// Squares still to look at, (depth, x, y), the next on top
    stack: Vec<(u8, u32, u32)>,
    /// Ids of a square covered whole, still to give
    run: std::ops::Range<u64>,
}

impl Level {
    fn new(areas: &[Area], z: u8) -> Self {
        let boxes: Vec<_> = areas
            .iter()
            .filter(|a| (a.min_zoom..=a.max_zoom).contains(&z))
            .map(|a| {
                let (x0, y0) = tile_of(z, a.west, a.north);
                let (x1, y1) = tile_of(z, a.east, a.south);
                (x0, y0, x1, y1)
            })
            .collect();
        let stack = if boxes.is_empty() {
            vec![]
        } else {
            vec![(0, 0, 0)]
        };
        Level {
            z,
            boxes,
            stack,
            run: 0..0,
        }
    }

    /// The id of tile x, y at this zoom
    fn id(&self, x: u32, y: u32) -> u64 {
        TileCoord::new(self.z, x, y).map_or(0, |c| TileId::from(c).value())
    }
}

impl Iterator for Level {
    type Item = TileCoord;

    fn next(&mut self) -> Option<TileCoord> {
        loop {
            if let Some(id) = self.run.next() {
                return TileId::new(id).ok().map(TileCoord::from);
            }
            let (depth, x, y) = self.stack.pop()?;
            // The square's tiles at zoom z
            let side = 1u32 << (self.z - depth);
            let (tx0, ty0) = (x * side, y * side);
            let (tx1, ty1) = (tx0 + side - 1, ty0 + side - 1);
            let meets = |&(x0, y0, x1, y1): &(u32, u32, u32, u32)| {
                tx0 <= x1 && x0 <= tx1 && ty0 <= y1 && y0 <= ty1
            };
            let covers = |&(x0, y0, x1, y1): &(u32, u32, u32, u32)| {
                x0 <= tx0 && tx1 <= x1 && y0 <= ty0 && ty1 <= y1
            };
            if !self.boxes.iter().any(meets) {
                continue;
            }
            if self.boxes.iter().any(covers) {
                // The square is one stretch of ids, aligned to its size within the zoom.
                let first = ((1u64 << (2 * self.z)) - 1) / 3;
                let size = 1u64 << (2 * (self.z - depth));
                let start = first + (self.id(tx0, ty0) - first) / size * size;
                self.run = start..start + size;
                continue;
            }
            // On an edge: its four quarters, the lowest ids on top
            let mut quarters = [(0, 0), (1, 0), (0, 1), (1, 1)].map(|(dx, dy)| {
                let (qx, qy) = (2 * x + dx, 2 * y + dy);
                let half = side / 2;
                (self.id(qx * half, qy * half), qx, qy)
            });
            quarters.sort_unstable_by_key(|q| std::cmp::Reverse(q.0));
            for (_, qx, qy) in quarters {
                self.stack.push((depth + 1, qx, qy));
            }
        }
    }
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
        let tiles: Vec<_> = wanted(&[a, a]).collect();
        // One tile per zoom up to 3 for so small a box
        assert_eq!(tiles.len(), 4);
        let ids: Vec<u64> = tiles.iter().map(|c| TileId::from(*c).value()).collect();
        assert!(ids.windows(2).all(|w| w[0] < w[1]));
        // A spot is 40 km wide: at zoom 14 (1.6 km a tile there) some 25 × 25 tiles
        let s: Vec<_> = wanted(&[Area {
            min_zoom: 14,
            ..spot(48.78, 8.08, REGION_ZOOM + 1)
        }])
        .collect();
        assert!((500..900).contains(&s.len()), "{}", s.len());
        let airpark = spot(48.78, 8.08, REGION_ZOOM + 1);
        assert!(holds(&airpark, 12, 2139, 1410));
        assert!(!holds(&airpark, 12, 2200, 1410));
        assert!(!holds(&airpark, 8, 133, 88));
    }

    /// The way it was done before: every id listed, sorted, made unique.
    fn by_list(areas: &[Area]) -> Vec<TileCoord> {
        let mut ids: Vec<u64> = Vec::new();
        for a in areas {
            for z in a.min_zoom..=a.max_zoom {
                let (x0, y0) = tile_of(z, a.west, a.north);
                let (x1, y1) = tile_of(z, a.east, a.south);
                for x in x0..=x1 {
                    for y in y0..=y1 {
                        ids.push(TileId::from(TileCoord::new(z, x, y).unwrap()).value());
                    }
                }
            }
        }
        ids.sort_unstable();
        ids.dedup();
        ids.into_iter()
            .map(|id| TileCoord::from(TileId::new(id).unwrap()))
            .collect()
    }

    #[test]
    fn gives_the_same_tiles_as_a_sorted_list() {
        let europe = region([-25.0, 27.0, 45.0, 72.0], 9);
        let rhine = region([7.5, 47.5, 9.0, 49.5], 12);
        let cases: Vec<Vec<Area>> = vec![
            vec![world(6)],
            vec![world(4), europe],
            vec![world(3), europe, rhine, spot(48.78, 8.08, 10)],
            vec![spot(36.4, 28.1, 9), spot(36.45, 28.2, 9)],
            vec![region([170.0, -50.0, 179.9, -30.0], 8)],
            vec![],
        ];
        for areas in cases {
            assert_eq!(
                wanted(&areas).collect::<Vec<_>>(),
                by_list(&areas),
                "{areas:?}"
            );
        }
    }

    #[test]
    fn makes_the_world_one_tile_at_a_time() {
        // The world at zoom 15 starts at once and counts right at a zoom we can walk through
        let deep = world(15);
        assert_eq!(wanted(std::slice::from_ref(&deep)).take(3).count(), 3);
        let n = wanted(&[world(9)]).count() as u64;
        assert_eq!(n, (0..=9).map(|z| 1u64 << (2 * z)).sum::<u64>());
        // A spot at zoom 14 is found without walking the zoom's 268 million ids
        let t = std::time::Instant::now();
        let s = wanted(&[spot(48.78, 8.08, 9)]).count();
        assert!(
            s < 5_000 && t.elapsed() < std::time::Duration::from_secs(1),
            "{s} {:?}",
            t.elapsed()
        );
    }

    #[test]
    fn knows_when_a_cut_is_the_whole_source() {
        let planet = [-180.0, -85.05, 180.0, 85.05];
        let europe = [-25.0, 27.0, 45.0, 72.0];
        // The world to 15 as a region over the world (WF_BASEMAP_REGION), Protomaps goes to 15
        assert!(whole_source(&[world(7), region(planet, 15)], 15));
        // The relief: the world to 12, Mapterhorn goes to 12
        assert!(whole_source(&[world(12)], 12));
        // Europe to 15, or the world to 14 only: a cut
        assert!(!whole_source(&[world(7), region(europe, 15)], 15));
        assert!(!whole_source(&[world(14)], 15));
        assert!(!whole_source(
            &[world(7), region([-179.0, -85.05, 180.0, 85.05], 15)],
            15
        ));
    }
}
