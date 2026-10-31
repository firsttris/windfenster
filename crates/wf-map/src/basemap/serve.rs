//! The basemap's tiles for the map (`/api/map/basemap/{z}/{x}/{y}`), any zoom anywhere: from
//! the spot archive that holds the tile, else from the main archive (the world, the region
//! deeper; files made before the spots had their own hold them too). Those go out as stored,
//! compressed as the archive says. Where neither goes that deep, the deepest tile above it is
//! cut down to it (`mvt::overzoom`), so the map never shows a hole and needs one source only.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use pmtiles::{
    AsyncBackend, AsyncPmTilesReader, BackendResponse, Compression, PmtResult, TileCoord,
};

use super::spots::{self, Spot};

/// A local PMTiles file, read by range from the handle opened with the reader. When the file
/// is replaced (renamed over), this reader still reads the file it opened, so its cached
/// directories always match the data; the next check opens the new file with a new reader.
pub struct File(Arc<std::fs::File>);

impl File {
    pub(crate) fn of(file: std::fs::File) -> Self {
        File(Arc::new(file))
    }
}

impl AsyncBackend for File {
    async fn read(&self, offset: usize, length: usize) -> PmtResult<BackendResponse> {
        use std::os::unix::fs::FileExt;
        let file = self.0.clone();
        let buf = tokio::task::spawn_blocking(move || {
            let mut buf = vec![0; length];
            let n = file.read_at(&mut buf, offset as u64)?;
            // The last range may run past the end of the file
            buf.truncate(n);
            Ok::<_, std::io::Error>(buf)
        })
        .await
        .map_err(std::io::Error::other)??;
        Ok(BackendResponse::new(buf.into()))
    }
}

/// With its directories cached: a tile is one read then, not a leaf directory read and
/// unpacked every time (Protomaps' leaves are tens to hundreds of KB).
type Reader = AsyncPmTilesReader<File, super::dircache::Bounded>;

/// How long the list of spot files and a file's modification time are trusted before the disk
/// is asked again: a map pan asks for 20 to 40 tiles, each up to 7 levels deep.
const RECHECK: Duration = Duration::from_secs(10);

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

/// Open readers, one per file as it is on disk, so a replaced file is read anew; and the
/// spot files on disk. Both checked against the disk at most every `RECHECK`.
#[derive(Default)]
pub struct Readers {
    files: Mutex<HashMap<PathBuf, Open>>,
    spots: Mutex<Option<(Instant, SpotList)>>,
}

/// The spot files on disk with their modification times, newest first
type SpotList = Arc<Vec<(Spot, SystemTime)>>;

/// What tells a file from the one it replaced: a rename gives a new inode, and the time alone
/// can be the same for two files written within the file system's clock tick.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Identity {
    inode: u64,
    len: u64,
    modified: SystemTime,
}

impl Identity {
    fn of(meta: &std::fs::Metadata) -> Option<Self> {
        use std::os::unix::fs::MetadataExt;
        Some(Self {
            inode: meta.ino(),
            len: meta.len(),
            modified: meta.modified().ok()?,
        })
    }
}

struct Open {
    identity: Identity,
    checked: Instant,
    reader: Arc<Reader>,
}

impl Readers {
    async fn get(&self, path: &Path) -> Option<Arc<Reader>> {
        if let Some(open) = self.files.lock().expect("readers").get(path)
            && open.checked.elapsed() < RECHECK
        {
            return Some(open.reader.clone());
        }
        let Ok(meta) = tokio::fs::metadata(path).await else {
            // Gone (a spot removed): let go of its reader, whose open file would keep its room
            // on the disk taken
            self.files.lock().expect("readers").remove(path);
            return None;
        };
        let identity = Identity::of(&meta)?;
        if let Some(open) = self.files.lock().expect("readers").get_mut(path)
            && open.identity == identity
        {
            open.checked = Instant::now();
            return Some(open.reader.clone());
        }
        let reader = Arc::new(
            AsyncPmTilesReader::try_from_cached_source(
                File(Arc::new(std::fs::File::open(path).ok()?)),
                super::dircache::Bounded::new(),
            )
            .await
            .ok()?,
        );
        let open = Open {
            identity,
            checked: Instant::now(),
            reader: reader.clone(),
        };
        self.files
            .lock()
            .expect("readers")
            .insert(path.to_path_buf(), open);
        Some(reader)
    }

    /// The spot files on disk, listed at most every `RECHECK`.
    async fn spots(&self, root: &Path) -> SpotList {
        if let Some((at, list)) = self.spots.lock().expect("spots").as_ref()
            && at.elapsed() < RECHECK
        {
            return list.clone();
        }
        let list = Arc::new(spots::on_disk(root).await);
        // Readers of spot files no longer on disk go with them
        let dir = spots::dir(root);
        self.files.lock().expect("readers").retain(|path, _| {
            !path.starts_with(&dir) || list.iter().any(|(s, _)| spots::path(root, *s) == *path)
        });
        *self.spots.lock().expect("spots") = Some((Instant::now(), list.clone()));
        list
    }

    /// Spots were made or removed: list them again on the next tile.
    pub fn forget_spots(&self) {
        *self.spots.lock().expect("spots") = None;
    }

    /// The tile from the spot that holds it, else from the main archive; past what they hold,
    /// the deepest tile above it cut down. None where nothing covers the place (an operator's
    /// file of one region), or for a tile that is not a vector tile.
    pub async fn tile(&self, root: &Path, z: u8, x: u32, y: u32) -> Option<Tile> {
        if z > MAX_ZOOM {
            return None;
        }
        TileCoord::new(z, x, y).ok()?;
        let spots = self.spots(root).await;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing;

    /// An archive with one world tile holding a place at `x`
    fn archive(x: u32, extra: usize) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        let mut w = pmtiles::PmTilesWriter::new(pmtiles::TileType::Mvt)
            .tile_compression(Compression::None)
            .metadata(format!(r#"{{"pad":"{}"}}"#, "x".repeat(extra)))
            .create(&mut out)
            .unwrap();
        w.add_tile(
            TileCoord::new(0, 0, 0).unwrap(),
            &testing::vector_tile(&[(x, 0)]),
        )
        .unwrap();
        w.finalize().unwrap();
        out.into_inner()
    }

    #[tokio::test]
    async fn a_replaced_archive_never_mixes_the_old_directory_with_the_new_data() {
        let dir = tempfile::tempdir().unwrap();
        let main = super::super::path(dir.path());
        std::fs::create_dir_all(main.parent().unwrap()).unwrap();
        std::fs::write(&main, archive(1, 0)).unwrap();
        let readers = Readers::default();
        let place = async || testing::points_in(&readers.tile(dir.path(), 0, 0, 0).await?.data);
        assert_eq!(place().await, Some(vec![(1.0, 0.0)]));

        // Made anew as the server does: written beside it, renamed over it; laid out otherwise,
        // so the old directory's offsets point into other bytes of the new file
        let part = main.with_extension("part");
        std::fs::write(&part, archive(2, 5_000)).unwrap();
        std::fs::rename(&part, &main).unwrap();
        // Within the recheck time the reader still reads the file it opened: the old tile, whole
        assert_eq!(place().await, Some(vec![(1.0, 0.0)]));
        // After it, the new one
        readers
            .files
            .lock()
            .unwrap()
            .values_mut()
            .for_each(|o| o.checked -= RECHECK);
        assert_eq!(place().await, Some(vec![(2.0, 0.0)]));
    }

    #[tokio::test]
    async fn lets_go_of_removed_spot_files() {
        let dir = tempfile::tempdir().unwrap();
        let spot = Spot::of(48.78, 8.08);
        let file = spots::path(dir.path(), spot);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, archive(1, 0)).unwrap();
        let readers = Readers::default();
        assert!(readers.tile(dir.path(), 0, 0, 0).await.is_some());
        assert!(readers.files.lock().unwrap().contains_key(&file));
        std::fs::remove_file(&file).unwrap();
        readers.forget_spots();
        assert!(readers.tile(dir.path(), 0, 0, 0).await.is_none());
        assert!(readers.files.lock().unwrap().is_empty());
    }
}
