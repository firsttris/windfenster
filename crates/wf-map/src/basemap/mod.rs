//! The detailed basemap: a Protomaps PMTiles file (vector tiles from OpenStreetMap) at
//! `{root}/basemap.pmtiles`, put there by the operator or made by `fetch`. The app reads its zoom range and bounds from the header and
//! fetches tiles from it by byte range.

pub mod fetch;
pub mod serve;
pub mod spots;
pub mod tiles;

use std::path::{Path, PathBuf};

use serde::Serialize;
use tokio::io::AsyncReadExt;
use ts_rs::TS;

/// What the app needs to know about the file.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BasemapInfo {
    pub min_zoom: u8,
    pub max_zoom: u8,
    /// west, south, east, north in degrees
    pub bounds: [f64; 4],
    /// The file holds all of `bounds` up to this zoom; deeper only the `detail` boxes. The map
    /// enlarges the tiles of this zoom elsewhere instead of showing nothing.
    pub full_zoom: u8,
    /// west, south, east, north of the boxes held up to `detail_zoom`, served by tile at
    /// `/api/map/basemap/{z}/{x}/{y}`
    pub detail: Vec<[f64; 4]>,
    pub detail_zoom: u8,
}

pub const FILE: &str = "basemap.pmtiles";
const HEADER: usize = 127;

pub fn path(root: &Path) -> PathBuf {
    root.join(FILE)
}

/// The PMTiles v3 header (127 bytes, little-endian): zooms at 100 and 101, bounds as
/// ten-millionths of a degree from 102. None for anything else.
pub fn read_header(bytes: &[u8]) -> Option<BasemapInfo> {
    if bytes.len() < HEADER || &bytes[..7] != b"PMTiles" || bytes[7] != 3 {
        return None;
    }
    let deg = |at: usize| {
        let v = i32::from_le_bytes(bytes[at..at + 4].try_into().expect("four bytes"));
        f64::from(v) / 1e7
    };
    Some(BasemapInfo {
        min_zoom: bytes[100],
        max_zoom: bytes[101],
        bounds: [deg(102), deg(106), deg(110), deg(114)],
        full_zoom: bytes[101],
        detail: Vec::new(),
        detail_zoom: bytes[101],
    })
}

/// The file's header, if there is a basemap; for one we made, also where there is more detail
/// (an operator's file counts as complete).
pub async fn info(root: &Path) -> Option<BasemapInfo> {
    let mut file = tokio::fs::File::open(path(root)).await.ok()?;
    let mut bytes = [0u8; HEADER];
    file.read_exact(&mut bytes).await.ok()?;
    let header = read_header(&bytes)?;
    Some(match fetch::read_note(root).await {
        Some(note) => {
            let spots = spots::on_disk(root).await;
            with_detail(header, &note, spots.iter().map(|(s, _)| *s))
        }
        None => header,
    })
}

/// Our file: the region to `REGION_ZOOM`, deeper the spots' own files (and, in a file made
/// before those, the spots it holds itself).
fn with_detail(
    header: BasemapInfo,
    note: &fetch::Note,
    spots: impl Iterator<Item = spots::Spot>,
) -> BasemapInfo {
    let boxes = note
        .spots
        .iter()
        .map(|&[lat, lon]| tiles::spot(lat, lon))
        .chain(spots.map(spots::Spot::area));
    let detail: Vec<[f64; 4]> = boxes.map(|a| [a.west, a.south, a.east, a.north]).collect();
    BasemapInfo {
        full_zoom: header.max_zoom.min(tiles::REGION_ZOOM),
        detail_zoom: if detail.is_empty() {
            header.max_zoom
        } else {
            tiles::SPOT_ZOOM
        },
        detail,
        ..header
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_header() {
        // The header of a Protomaps extract of Europe to zoom 8 (build 2026-10-10)
        let bytes: [u8; 127] = [
            80, 77, 84, 105, 108, 101, 115, 3, 127, 0, 0, 0, 0, 0, 0, 0, 205, 32, 0, 0, 0, 0, 0, 0,
            76, 33, 0, 0, 0, 0, 0, 0, 154, 4, 0, 0, 0, 0, 0, 0, 230, 37, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 230, 37, 0, 0, 0, 0, 0, 0, 62, 114, 101, 11, 0, 0, 0, 0, 131, 15, 0, 0,
            0, 0, 0, 0, 117, 11, 0, 0, 0, 0, 0, 0, 192, 10, 0, 0, 0, 0, 0, 0, 1, 2, 2, 1, 0, 8,
            128, 77, 25, 241, 128, 223, 23, 16, 128, 116, 210, 26, 0, 84, 234, 42, 0, 0, 225, 245,
            5, 192, 25, 129, 29,
        ];
        assert_eq!(
            read_header(&bytes),
            Some(BasemapInfo {
                min_zoom: 0,
                max_zoom: 8,
                bounds: [-25.0, 27.0, 45.0, 72.0],
                full_zoom: 8,
                detail: vec![],
                detail_zoom: 8,
            })
        );
        assert_eq!(read_header(&bytes[..100]), None);
        assert_eq!(read_header(b"not a pmtiles file at all"), None);
    }

    #[test]
    fn knows_where_our_file_has_detail() {
        let header = BasemapInfo {
            min_zoom: 0,
            max_zoom: 8,
            bounds: [-25.0, 27.0, 45.0, 72.0],
            full_zoom: 8,
            detail: vec![],
            detail_zoom: 8,
        };
        let note = fetch::Note {
            build: "20261011".into(),
            spots: vec![],
            made_at: 0,
        };
        let spot = spots::Spot::of(48.78, 8.08);
        let info = with_detail(header.clone(), &note, [spot].into_iter());
        assert_eq!(
            (info.full_zoom, info.detail_zoom),
            (tiles::REGION_ZOOM, tiles::SPOT_ZOOM)
        );
        let [west, south, east, north] = info.detail[0];
        // 20 km around the spot
        assert!((north - south - 2.0 * 20.0 / 111.32).abs() < 1e-9);
        assert!(west < 8.0 && east > 8.0);
        // No spots yet: nothing deeper than the region
        let bare = with_detail(header, &note, std::iter::empty());
        assert_eq!((bare.detail.len(), bare.detail_zoom), (0, 8));
    }
}
