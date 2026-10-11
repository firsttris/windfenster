//! The detailed basemap: a Protomaps PMTiles file (vector tiles from OpenStreetMap) the operator
//! puts at `{root}/basemap.pmtiles`. The app reads its zoom range and bounds from the header and
//! fetches tiles from it by byte range.

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
    })
}

/// The file's header, if there is a basemap.
pub async fn info(root: &Path) -> Option<BasemapInfo> {
    let mut file = tokio::fs::File::open(path(root)).await.ok()?;
    let mut bytes = [0u8; HEADER];
    file.read_exact(&mut bytes).await.ok()?;
    read_header(&bytes)
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
            })
        );
        assert_eq!(read_header(&bytes[..100]), None);
        assert_eq!(read_header(b"not a pmtiles file at all"), None);
    }
}
