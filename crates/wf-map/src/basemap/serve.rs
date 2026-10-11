//! The basemap's tiles for the map (`/api/map/basemap/{z}/{x}/{y}`), any zoom anywhere: from
//! the spot archive that holds the tile, else from the main archive (the world, the region
//! deeper; files made before the spots had their own hold them too). Those go out as stored,
//! compressed as the archive says. Where neither goes that deep, the deepest tile above it is
//! cut down to it (`mvt::overzoom`), so the map never shows a hole and needs one source only.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use pmtiles::{
    AsyncBackend, AsyncPmTilesReader, BackendResponse, Compression, PmtResult, TileCoord,
};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use super::spots::{self, Spot};

/// A local PMTiles file, read by range.
pub struct File(PathBuf);

impl AsyncBackend for File {
    async fn read(&self, offset: usize, length: usize) -> PmtResult<BackendResponse> {
        let mut f = tokio::fs::File::open(&self.0).await?;
        f.seek(std::io::SeekFrom::Start(offset as u64)).await?;
        let mut buf = Vec::with_capacity(length);
        f.take(length as u64).read_to_end(&mut buf).await?;
        Ok(BackendResponse::new(buf.into()))
    }
}

type Reader = AsyncPmTilesReader<File>;

/// Deepest tile served; the map enlarges it from there (as it does past a source's maxzoom).
const MAX_ZOOM: u8 = 16;

/// A tile and its HTTP Content-Encoding (gzip for Protomaps builds, none when cut from an
/// ancestor).
pub struct Tile {
    pub data: Vec<u8>,
    pub encoding: Option<&'static str>,
    /// Stored as it is; false when cut from an ancestor (a spot may bring the real one).
    pub exact: bool,
}

fn encoding(c: Compression) -> Option<&'static str> {
    match c {
        Compression::Gzip => Some("gzip"),
        Compression::Brotli => Some("br"),
        Compression::Zstd => Some("zstd"),
        _ => None,
    }
}

/// Open readers, one per file and modification time, so a replaced file is read anew.
#[derive(Default)]
pub struct Readers(Mutex<HashMap<PathBuf, (SystemTime, Arc<Reader>)>>);

impl Readers {
    async fn get(&self, path: &Path) -> Option<Arc<Reader>> {
        let modified = tokio::fs::metadata(path).await.ok()?.modified().ok()?;
        if let Some((t, r)) = self.0.lock().expect("readers").get(path)
            && *t == modified
        {
            return Some(r.clone());
        }
        let reader = Arc::new(
            AsyncPmTilesReader::try_from_source(File(path.to_path_buf()))
                .await
                .ok()?,
        );
        self.0
            .lock()
            .expect("readers")
            .insert(path.to_path_buf(), (modified, reader.clone()));
        Some(reader)
    }

    /// The tile from the spot that holds it, else from the main archive; past what they hold,
    /// the deepest tile above it cut down. None where nothing covers the place (an operator's
    /// file of one region), or for a tile that is not a vector tile.
    pub async fn tile(&self, root: &Path, z: u8, x: u32, y: u32) -> Option<Tile> {
        if z > MAX_ZOOM {
            return None;
        }
        TileCoord::new(z, x, y).ok()?;
        let spots = spots::on_disk(root).await;
        for dz in 0..=z {
            let (az, ax, ay) = (z - dz, x >> dz, y >> dz);
            let coord = TileCoord::new(az, ax, ay).ok()?;
            let files = spots
                .iter()
                .filter(|(s, _)| super::tiles::holds(&s.area(0), az, ax, ay))
                .map(|(s, _): &(Spot, _)| spots::path(root, *s))
                .chain(std::iter::once(super::path(root)));
            for file in files {
                let Some(reader) = self.get(&file).await else {
                    continue;
                };
                if dz == 0 {
                    if let Ok(Some(data)) = reader.get_tile(coord).await {
                        return Some(Tile {
                            data: data.to_vec(),
                            encoding: encoding(reader.get_header().tile_compression),
                            exact: true,
                        });
                    }
                } else if let Ok(Some(data)) = reader.get_tile_decompressed(coord).await {
                    let (cx, cy) = (x - (ax << dz), y - (ay << dz));
                    let cut = tokio::task::spawn_blocking(move || {
                        super::mvt::overzoom(&data, dz, cx, cy)
                    })
                    .await
                    .ok()?
                    .ok()?;
                    return Some(Tile {
                        data: cut,
                        encoding: None,
                        exact: false,
                    });
                }
            }
        }
        None
    }
}
