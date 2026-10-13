//! A PMTiles v3 writer whose memory does not grow with the tiles (spec:
//! github.com/protomaps/PMTiles/blob/main/spec/v3/spec.md). The pmtiles crate's writer keeps an
//! entry for every tile and a hash of every content until it finishes, some 15 GB for the
//! world at zoom 15. Here tiles come in id order (as `tiles::wanted` gives them), so:
//!
//! - their data goes straight into the target, after room kept for header, root and metadata;
//! - equal tiles in a row become one entry with a run length, and a content seen among the
//!   last ones (small tiles, mostly empty sea and land) is pointed to instead of written again;
//! - every `leaf` entries form a leaf directory, compressed into a side file at once; only the
//!   root's entries, one per leaf, stay in memory;
//! - at the end the leaves follow the data, the root (in further levels of leaves if it would
//!   not fit its 16 KiB) and the header go in front.

use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use flate2::Compression as Level;
use flate2::write::GzEncoder;
use pmtiles::{Compression, TileType};

const HEADER: u64 = 127;
/// Header and root together within the first 16 KiB, as readers fetch them in one go
const ROOT_MAX: usize = 16_384 - HEADER as usize;
/// Contents remembered to point to instead of writing again
const RECENT: usize = 100_000;
/// Contents up to this size are remembered with their bytes (to compare, not just hash)
const SMALL: usize = 2_048;
/// Entries per leaf at least, and a root of about this many entries is aimed for
const MIN_LEAF: usize = 4_096;
const ROOT_ENTRIES: u64 = 2_048;

/// What the archive is.
#[derive(Debug, Clone)]
pub struct Spec {
    pub tile_type: TileType,
    pub tile_compression: Compression,
    pub min_zoom: u8,
    pub max_zoom: u8,
    /// west, south, east, north
    pub bounds: [f64; 4],
    pub metadata: String,
    /// Tile entries expected, an upper bound: sets the leaf size
    pub expected: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Entry {
    tile_id: u64,
    offset: u64,
    length: u32,
    run_length: u32,
}

/// Recently written contents by hash: offset, length and, when small, the bytes.
#[derive(Default)]
struct Recent {
    by_hash: HashMap<u64, (u64, u32, Vec<u8>)>,
    order: VecDeque<u64>,
}

impl Recent {
    fn find(&self, hash: u64, data: &[u8]) -> Option<(u64, u32)> {
        let (offset, length, bytes) = self.by_hash.get(&hash)?;
        (bytes.as_slice() == data).then_some((*offset, *length))
    }

    fn keep(&mut self, hash: u64, offset: u64, data: &[u8]) {
        if data.len() > SMALL || self.by_hash.contains_key(&hash) {
            return;
        }
        if self.order.len() >= RECENT
            && let Some(old) = self.order.pop_front()
        {
            self.by_hash.remove(&old);
        }
        self.by_hash
            .insert(hash, (offset, data.len() as u32, data.to_vec()));
        self.order.push_back(hash);
    }
}

pub struct Writer {
    spec: Spec,
    out: BufWriter<File>,
    /// Bytes of tile data written
    data_len: u64,
    data_start: u64,
    metadata_len: u64,
    side: PathBuf,
    leaves: BufWriter<File>,
    leaves_len: u64,
    leaf: usize,
    root_max: usize,
    entries: Vec<Entry>,
    root: Vec<Entry>,
    recent: Recent,
    /// The last tile's content, to run equal tiles in a row into one entry
    last: Vec<u8>,
    addressed: u64,
    contents: u64,
    entry_count: u64,
}

impl Writer {
    /// A new archive at `path` (replaced if there), with a side file next to it for the leaves.
    pub fn create(path: &Path, spec: Spec) -> std::io::Result<Self> {
        let leaf = usize::try_from(spec.expected.div_ceil(ROOT_ENTRIES))
            .unwrap_or(usize::MAX)
            .clamp(MIN_LEAF, 1 << 20);
        Self::with_leaf(path, spec, leaf, ROOT_MAX)
    }

    /// With leaves of `leaf` entries and a root of at most `root_max` bytes (small in tests).
    fn with_leaf(path: &Path, spec: Spec, leaf: usize, root_max: usize) -> std::io::Result<Self> {
        let mut out = BufWriter::with_capacity(1 << 20, File::create(path)?);
        // Header and root come at the end; metadata right after their room
        out.write_all(&vec![0; HEADER as usize + ROOT_MAX])?;
        let metadata = gzip(spec.metadata.as_bytes())?;
        out.write_all(&metadata)?;
        let side = path.with_extension("leaves");
        let leaves = BufWriter::with_capacity(1 << 20, File::create(&side)?);
        Ok(Writer {
            data_start: HEADER + ROOT_MAX as u64 + metadata.len() as u64,
            metadata_len: metadata.len() as u64,
            spec,
            out,
            data_len: 0,
            side,
            leaves,
            leaves_len: 0,
            leaf,
            root_max,
            entries: Vec::with_capacity(leaf),
            root: Vec::new(),
            recent: Recent::default(),
            last: Vec::new(),
            addressed: 0,
            contents: 0,
            entry_count: 0,
        })
    }

    /// Adds a tile; ids must rise.
    pub fn add(&mut self, tile_id: u64, data: &[u8]) -> std::io::Result<()> {
        self.addressed += 1;
        if let Some(prev) = self.entries.last_mut()
            && prev.tile_id + u64::from(prev.run_length) == tile_id
            && self.last == data
        {
            prev.run_length += 1;
            return Ok(());
        }
        let hash = {
            let mut h = DefaultHasher::new();
            data.hash(&mut h);
            h.finish()
        };
        let (offset, length) = match self.recent.find(hash, data) {
            Some(found) => found,
            None => {
                let offset = self.data_len;
                self.out.write_all(data)?;
                self.data_len += data.len() as u64;
                self.contents += 1;
                self.recent.keep(hash, offset, data);
                let length = u32::try_from(data.len()).map_err(std::io::Error::other)?;
                (offset, length)
            }
        };
        if self.entries.len() == self.leaf {
            self.flush_leaf()?;
        }
        self.entries.push(Entry {
            tile_id,
            offset,
            length,
            run_length: 1,
        });
        self.entry_count += 1;
        self.last.clear();
        self.last.extend_from_slice(data);
        Ok(())
    }

    /// Writes the entries gathered as a leaf and points the root to it.
    fn flush_leaf(&mut self) -> std::io::Result<()> {
        let entries = std::mem::take(&mut self.entries);
        if let Some(first) = entries.first() {
            let bytes = gzip(&directory(&entries))?;
            self.leaves.write_all(&bytes)?;
            self.root.push(Entry {
                tile_id: first.tile_id,
                offset: self.leaves_len,
                length: u32::try_from(bytes.len()).map_err(std::io::Error::other)?,
                run_length: 0,
            });
            self.leaves_len += bytes.len() as u64;
        }
        Ok(())
    }

    /// Puts the leaves after the data, the root and the header in front; returns the tile
    /// entries written.
    pub fn finish(mut self) -> std::io::Result<u64> {
        // Few tiles: all entries in the root, if they fit
        let mut root = if self.root.is_empty() {
            let bytes = gzip(&directory(&self.entries))?;
            if bytes.len() <= self.root_max {
                std::mem::take(&mut self.entries)
            } else {
                self.flush_leaf()?;
                std::mem::take(&mut self.root)
            }
        } else {
            self.flush_leaf()?;
            std::mem::take(&mut self.root)
        };
        // A root too large for its room becomes leaves of leaves, a level at a time
        let mut root_bytes = gzip(&directory(&root))?;
        while root_bytes.len() > self.root_max {
            let mut upper = Vec::new();
            for chunk in root.chunks(self.leaf) {
                let bytes = gzip(&directory(chunk))?;
                self.leaves.write_all(&bytes)?;
                upper.push(Entry {
                    tile_id: chunk[0].tile_id,
                    offset: self.leaves_len,
                    length: u32::try_from(bytes.len()).map_err(std::io::Error::other)?,
                    run_length: 0,
                });
                self.leaves_len += bytes.len() as u64;
            }
            root = upper;
            root_bytes = gzip(&directory(&root))?;
        }
        // The leaves after the data
        self.leaves.flush()?;
        drop(self.leaves);
        let leaf_start = self.data_start + self.data_len;
        let mut side = File::open(&self.side)?;
        std::io::copy(&mut side, &mut self.out)?;
        drop(side);
        std::fs::remove_file(&self.side)?;
        let mut file = self.out.into_inner().map_err(|e| e.into_error())?;
        let header = header(&Header {
            spec: &self.spec,
            root_len: root_bytes.len() as u64,
            metadata_len: self.metadata_len,
            leaf_start,
            leaf_len: self.leaves_len,
            data_start: self.data_start,
            data_len: self.data_len,
            addressed: self.addressed,
            entries: self.entry_count,
            contents: self.contents,
        });
        file.seek(SeekFrom::Start(0))?;
        file.write_all(&header)?;
        file.write_all(&root_bytes)?;
        file.sync_all()?;
        Ok(self.entry_count)
    }
}

fn gzip(bytes: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut gz = GzEncoder::new(Vec::new(), Level::default());
    gz.write_all(bytes)?;
    gz.finish()
}

fn varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

/// A directory as the spec lays it out: count, then ids as differences, run lengths, lengths,
/// and offsets (0 for one that follows the entry before it, else offset + 1).
fn directory(entries: &[Entry]) -> Vec<u8> {
    let mut out = Vec::with_capacity(entries.len() * 6);
    varint(&mut out, entries.len() as u64);
    let mut last = 0;
    for e in entries {
        varint(&mut out, e.tile_id - last);
        last = e.tile_id;
    }
    for e in entries {
        varint(&mut out, u64::from(e.run_length));
    }
    for e in entries {
        varint(&mut out, u64::from(e.length));
    }
    for (i, e) in entries.iter().enumerate() {
        let follows = i > 0 && {
            let p = entries[i - 1];
            e.offset == p.offset + u64::from(p.length)
        };
        varint(&mut out, if follows { 0 } else { e.offset + 1 });
    }
    out
}

struct Header<'a> {
    spec: &'a Spec,
    root_len: u64,
    metadata_len: u64,
    leaf_start: u64,
    leaf_len: u64,
    data_start: u64,
    data_len: u64,
    addressed: u64,
    entries: u64,
    contents: u64,
}

fn compression(c: Compression) -> u8 {
    match c {
        Compression::Unknown => 0,
        Compression::None => 1,
        Compression::Gzip => 2,
        Compression::Brotli => 3,
        Compression::Zstd => 4,
    }
}

fn tile_type(t: TileType) -> u8 {
    match t {
        TileType::Unknown => 0,
        TileType::Mvt => 1,
        TileType::Png => 2,
        TileType::Jpeg => 3,
        TileType::Webp => 4,
        TileType::Avif => 5,
        TileType::Mlt => 6,
    }
}

/// The 127 bytes in front, little-endian, as the spec lays them out.
fn header(h: &Header) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER as usize);
    out.extend_from_slice(b"PMTiles");
    out.push(3);
    for v in [
        HEADER,
        h.root_len,
        HEADER + ROOT_MAX as u64,
        h.metadata_len,
        h.leaf_start,
        h.leaf_len,
        h.data_start,
        h.data_len,
        h.addressed,
        h.entries,
        h.contents,
    ] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    let s = h.spec;
    out.push(1); // clustered: data in the order of the ids
    out.push(compression(Compression::Gzip)); // directories and metadata
    out.push(compression(s.tile_compression));
    out.push(tile_type(s.tile_type));
    out.push(s.min_zoom);
    out.push(s.max_zoom);
    let e7 = |deg: f64| ((deg * 1e7).round() as i32).to_le_bytes();
    let [w, south, e, n] = s.bounds;
    for d in [w, south, e, n] {
        out.extend_from_slice(&e7(d));
    }
    out.push(s.min_zoom);
    out.extend_from_slice(&e7((w + e) / 2.0));
    out.extend_from_slice(&e7((south + n) / 2.0));
    debug_assert_eq!(out.len(), HEADER as usize);
    out
}

#[cfg(test)]
mod tests {
    use pmtiles::{
        AsyncBackend, AsyncPmTilesReader, BackendResponse, PmtResult, TileCoord, TileId,
    };

    use super::*;

    struct Memory(Vec<u8>);

    impl AsyncBackend for Memory {
        async fn read(&self, offset: usize, length: usize) -> PmtResult<BackendResponse> {
            let end = (offset + length).min(self.0.len());
            Ok(BackendResponse::new(self.0[offset..end].to_vec().into()))
        }
    }

    fn spec(expected: u64) -> Spec {
        Spec {
            tile_type: TileType::Mvt,
            tile_compression: Compression::Gzip,
            min_zoom: 0,
            max_zoom: 14,
            bounds: [-180.0, -85.0, 180.0, 85.0],
            metadata: r#"{"name":"test"}"#.into(),
            expected,
        }
    }

    /// The content of tile `id`: runs of equal tiles, a few contents again and again, the rest
    /// its own
    fn content(id: u64) -> Vec<u8> {
        match id % 10 {
            0..=3 => b"sea".to_vec(),
            4 => format!("land {}", id % 7).into_bytes(),
            _ => format!("tile {id}").into_bytes(),
        }
    }

    #[tokio::test]
    async fn reads_back_every_tile_through_leaves_of_leaves() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.pmtiles");
        // Leaves of 16 entries and a root of 512 bytes at most: the root needs levels of leaves
        let mut w = Writer::with_leaf(&path, spec(0), 16, 512).unwrap();
        // Ids with gaps, as a cut leaves out tiles the source does not have
        let ids: Vec<u64> = (0..30_000u64)
            .filter(|i| i % 13 != 5)
            .map(|i| i * 3 / 2)
            .collect();
        for &id in &ids {
            w.add(id, &content(id)).unwrap();
        }
        let entries = w.finish().unwrap();
        assert!(!dir.path().join("a.leaves").exists());
        let bytes = std::fs::read(&path).unwrap();
        // Runs and repeats make fewer entries and contents than tiles
        assert!(entries < ids.len() as u64);
        let info = crate::basemap::read_header(&bytes).unwrap();
        assert_eq!((info.min_zoom, info.max_zoom), (0, 14));
        // The root points to fewer directories than there are leaves of entries: a level of
        // leaves above them was needed to fit its 16 KiB.
        let root_len = u64::from_le_bytes(bytes[16..24].try_into().unwrap()) as usize;
        assert!(root_len <= 512);
        let mut root = Vec::new();
        std::io::Read::read_to_end(
            &mut flate2::read::GzDecoder::new(&bytes[127..127 + root_len]),
            &mut root,
        )
        .unwrap();
        let (mut count, mut shift) = (0u64, 0);
        for b in &root {
            count |= u64::from(b & 0x7f) << shift;
            shift += 7;
            if b & 0x80 == 0 {
                break;
            }
        }
        assert!(
            count < entries.div_ceil(16),
            "{count} root entries for {entries}"
        );
        let reader = AsyncPmTilesReader::try_from_source(Memory(bytes))
            .await
            .unwrap();
        assert!(reader.get_metadata().await.unwrap().contains("test"));
        let mut i = 0;
        for id in 0..*ids.last().unwrap() + 3 {
            let coord = TileCoord::from(TileId::new(id).unwrap());
            let got = reader.get_tile(coord).await.unwrap();
            if ids.get(i) == Some(&id) {
                assert_eq!(got.as_deref(), Some(&content(id)[..]), "tile {id}");
                i += 1;
            } else {
                assert_eq!(got, None, "tile {id} was never added");
            }
        }
    }

    #[tokio::test]
    async fn keeps_few_tiles_in_the_root() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("b.pmtiles");
        let mut w = Writer::create(&path, spec(3)).unwrap();
        for id in [0, 1, 5] {
            w.add(id, &content(id)).unwrap();
        }
        assert_eq!(w.finish().unwrap(), 2, "tiles 0 and 1 are one run");
        let bytes = std::fs::read(&path).unwrap();
        let reader = AsyncPmTilesReader::try_from_source(Memory(bytes))
            .await
            .unwrap();
        let tile = |id| TileCoord::from(TileId::new(id).unwrap());
        assert_eq!(
            reader.get_tile(tile(1)).await.unwrap().as_deref(),
            Some(&b"sea"[..])
        );
        assert_eq!(
            reader.get_tile(tile(5)).await.unwrap().as_deref(),
            Some(&b"tile 5"[..])
        );
        assert_eq!(reader.get_tile(tile(2)).await.unwrap(), None);
    }

    /// Peak memory of the whole process, kB
    fn peak_kb() -> u64 {
        std::fs::read_to_string("/proc/self/status")
            .unwrap()
            .lines()
            .find_map(|l| l.strip_prefix("VmHWM:"))
            .and_then(|v| v.trim().trim_end_matches(" kB").parse().ok())
            .unwrap()
    }

    /// 50 million tiles from a generator; run alone, in release:
    /// `cargo test --release -p wf-map --lib writer::tests::memory -- --ignored`
    #[test]
    #[ignore]
    fn memory_stays_flat_for_fifty_million_tiles() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c.pmtiles");
        let n = 50_000_000u64;
        let mut w = Writer::create(&path, spec(n)).unwrap();
        for id in 0..n {
            w.add(id, &content(id)).unwrap();
        }
        let entries = w.finish().unwrap();
        let mb = peak_kb() / 1024;
        eprintln!("{entries} entries, peak {mb} MB");
        assert!(mb < 200, "peak {mb} MB");
    }
}
