//! Detail tiles for the map (`/api/map/basemap/{z}/{x}/{y}`): from the spot archive that holds
//! the tile, else from the main archive (files made before the spots had their own hold them
//! too). The tiles go out as stored, compressed as the archive says.

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

/// A tile as stored and its HTTP Content-Encoding (gzip for Protomaps builds).
pub struct Tile {
    pub data: Vec<u8>,
    pub encoding: Option<&'static str>,
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

    /// The tile from the spot that holds it, else from the main archive; None where neither has
    /// it (the map then shows the enlarged tile of the region).
    pub async fn tile(&self, root: &Path, z: u8, x: u32, y: u32) -> Option<Tile> {
        let coord = TileCoord::new(z, x, y).ok()?;
        let mut files: Vec<PathBuf> = spots::on_disk(root)
            .await
            .into_iter()
            .filter(|(s, _)| super::tiles::holds(&s.area(), z, x, y))
            .map(|(s, _): (Spot, _)| spots::path(root, s))
            .collect();
        files.push(super::path(root));
        for file in files {
            let Some(reader) = self.get(&file).await else {
                continue;
            };
            if let Ok(Some(data)) = reader.get_tile(coord).await {
                return Some(Tile {
                    data: data.to_vec(),
                    encoding: encoding(reader.get_header().tile_compression),
                });
            }
        }
        None
    }
}
