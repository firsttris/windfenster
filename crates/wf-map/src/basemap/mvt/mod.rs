//! Mapbox Vector Tiles (MVT 2.1, a protobuf message) read and written just far enough to cut a
//! tile out of one of its ancestors: where the basemap stops, the server answers a deeper tile
//! with the part of the deepest tile it holds, enlarged, as MapLibre does past a source's
//! maxzoom. Layers keep their keys and values as they are; geometry is scaled and clipped to
//! the new tile plus a margin (lines with Liang–Barsky, rings with Sutherland–Hodgman), so
//! outlines at the tile's edges fall outside what is drawn.
//! Format: github.com/mapbox/vector-tile-spec/tree/master/2.1

mod clip;
mod wire;

use clip::{Rect, area2, clip_line, clip_ring, snapped};
use wire::{decode, encode};

/// Beyond the tile's edges kept of lines and rings, in tile units (of 4096)
const MARGIN: f64 = 64.0;

#[derive(Debug, thiserror::Error, PartialEq)]
#[error("not a vector tile: {0}")]
pub struct Error(&'static str);

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    Unknown,
    Point,
    Line,
    Polygon,
}

type Path = Vec<(f64, f64)>;

#[derive(Debug, Clone, PartialEq)]
struct Feature {
    id: Option<u64>,
    /// Packed key and value indices, as stored
    tags: Vec<u8>,
    kind: Kind,
    /// Points (one each), lines, or rings (closing point not repeated)
    paths: Vec<Path>,
}

#[derive(Debug, Clone, PartialEq)]
struct Layer {
    version: u64,
    name: Vec<u8>,
    features: Vec<Feature>,
    /// Encoded keys and values, as stored
    keys: Vec<Vec<u8>>,
    values: Vec<Vec<u8>>,
    extent: u64,
}

fn cut_feature(
    f: &Feature,
    scale: f64,
    (ox, oy): (f64, f64),
    rect: Rect,
    extent: f64,
) -> Option<Feature> {
    let moved = |path: &Path| -> Path {
        path.iter()
            .map(|&(x, y)| (x * scale - ox, y * scale - oy))
            .collect()
    };
    let paths: Vec<Path> = match f.kind {
        Kind::Point => f
            .paths
            .iter()
            .map(moved)
            .filter(|p| p[0].0 >= 0.0 && p[0].0 < extent && p[0].1 >= 0.0 && p[0].1 < extent)
            .map(snapped)
            .collect(),
        Kind::Line => f
            .paths
            .iter()
            .flat_map(|l| clip_line(&moved(l), rect))
            .map(snapped)
            .filter(|l| l.len() > 1)
            .collect(),
        Kind::Polygon => {
            // An outer ring that is gone takes its holes with it.
            let mut out = Vec::new();
            let mut keep = false;
            for ring in &f.paths {
                let outer = area2(ring) > 0.0;
                let cut = clip_ring(&moved(ring), rect)
                    .map(snapped)
                    .filter(|r| r.len() >= 3 && area2(r) != 0.0);
                if outer {
                    keep = cut.is_some();
                }
                if let (true, Some(r)) = (keep, cut) {
                    out.push(r);
                }
            }
            out
        }
        Kind::Unknown => Vec::new(),
    };
    (!paths.is_empty()).then(|| Feature { paths, ..f.clone() })
}

/// The tile `dz` zooms below `tile` at column `cx`, row `cy` among its descendants (0 to
/// 2^dz − 1): the ancestor's features scaled up and cut to it. `tile` as stored, uncompressed.
pub fn overzoom(tile: &[u8], dz: u8, cx: u32, cy: u32) -> Result<Vec<u8>, Error> {
    let layers = decode(tile)?;
    let scale = f64::from(1u32 << dz);
    let out: Vec<Layer> = layers
        .into_iter()
        .filter_map(|l| {
            let extent = l.extent as f64;
            let origin = (f64::from(cx) * extent, f64::from(cy) * extent);
            let margin = MARGIN * extent / 4096.0;
            let rect = (-margin, -margin, extent + margin, extent + margin);
            let features: Vec<Feature> = l
                .features
                .iter()
                .filter_map(|f| cut_feature(f, scale, origin, rect, extent))
                .collect();
            (!features.is_empty()).then_some(Layer { features, ..l })
        })
        .collect();
    Ok(encode(&out))
}

/// A tile of one layer of places (for tests elsewhere)
pub(crate) fn places(points: &[(u32, u32)]) -> Vec<u8> {
    let features = points
        .iter()
        .map(|&(x, y)| Feature {
            id: None,
            tags: Vec::new(),
            kind: Kind::Point,
            paths: vec![vec![(f64::from(x), f64::from(y))]],
        })
        .collect();
    encode(&[Layer {
        version: 2,
        name: b"places".to_vec(),
        features,
        keys: Vec::new(),
        values: Vec::new(),
        extent: 4096,
    }])
}

/// The places of such a tile; None for something else
pub(crate) fn places_in(tile: &[u8]) -> Option<Vec<(f64, f64)>> {
    let layers = decode(tile).ok()?;
    Some(
        layers
            .iter()
            .filter(|l| l.name == b"places")
            .flat_map(|l| l.features.iter().flat_map(|f| f.paths.iter().map(|p| p[0])))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(features: Vec<Feature>) -> Vec<u8> {
        encode(&[Layer {
            version: 2,
            name: b"water".to_vec(),
            features,
            keys: vec![b"kind".to_vec()],
            // A string value: field 1, "lake"
            values: vec![vec![10, 4, b'l', b'a', b'k', b'e']],
            extent: 4096,
        }])
    }

    pub(super) fn feature(kind: Kind, paths: Vec<Path>) -> Feature {
        Feature {
            id: Some(7),
            tags: vec![0, 0],
            kind,
            paths,
        }
    }

    #[test]
    fn enlarges_the_quarter_of_a_tile() {
        // A lake (outer ring clockwise in y-down) over the whole parent tile, a road across it
        // and two places: one in the north-west quarter, one in the south-east.
        let lake = vec![(0.0, 0.0), (4096.0, 0.0), (4096.0, 4096.0), (0.0, 4096.0)];
        assert!(area2(&lake) > 0.0);
        let parent = tile(vec![
            feature(Kind::Polygon, vec![lake]),
            feature(Kind::Line, vec![vec![(0.0, 1024.0), (4096.0, 1024.0)]]),
            feature(
                Kind::Point,
                vec![vec![(1000.0, 1000.0)], vec![(3000.0, 3000.0)]],
            ),
        ]);
        // The north-west child: 0…2048 of the parent is 0…4096 of it
        let child = decode(&overzoom(&parent, 1, 0, 0).unwrap()).unwrap();
        let f = &child[0].features;
        // The lake: the parent's edge at 0, cut 64 past the child's other edges, still an
        // outer ring
        let mut ring = f[0].paths[0].clone();
        assert!(area2(&ring) > 0.0);
        ring.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(
            ring,
            vec![(0.0, 0.0), (0.0, 4160.0), (4160.0, 0.0), (4160.0, 4160.0)]
        );
        assert_eq!(f[1].paths, vec![vec![(0.0, 2048.0), (4160.0, 2048.0)]]);
        assert_eq!(f[2].paths, vec![vec![(2000.0, 2000.0)]]);
        // Keys, values, the name and the ids stay.
        assert_eq!(child[0].name, b"water");
        assert_eq!(child[0].values[0], vec![10, 4, b'l', b'a', b'k', b'e']);
        assert_eq!(f[0].id, Some(7));
        // The south-east child of the south-east child (two zooms down) holds only the lake.
        let deep = decode(&overzoom(&parent, 2, 3, 3).unwrap()).unwrap();
        assert_eq!(deep[0].features.len(), 1);
        assert_eq!(deep[0].features[0].kind, Kind::Polygon);
    }

    #[test]
    fn drops_holes_of_rings_that_are_gone() {
        // An island far off in the north-east with a pond: nothing of it in the north-west child
        let island = vec![
            (3000.0, 100.0),
            (4000.0, 100.0),
            (4000.0, 1000.0),
            (3000.0, 1000.0),
        ];
        let pond: Path = island
            .iter()
            .rev()
            .map(|&(x, y)| (x - 100.0, y + 100.0))
            .collect();
        assert!(area2(&pond) < 0.0);
        let parent = tile(vec![feature(Kind::Polygon, vec![island, pond])]);
        assert!(
            decode(&overzoom(&parent, 1, 0, 0).unwrap())
                .unwrap()
                .is_empty()
        );
    }
}
