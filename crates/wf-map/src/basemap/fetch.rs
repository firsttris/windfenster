//! Puts a basemap in place without the operator doing anything: from the newest Protomaps daily
//! build, the map's region from afar, read by byte range (pmtiles crate) and written to one
//! archive. The spots people look at get archives of their own (`spots.rs`). A file the
//! operator put there is never touched; ours carries a note next to it (`basemap.json`) and is
//! renewed when it grows old.

use std::path::Path;
use std::sync::Arc;

use pmtiles::{
    AsyncBackend, AsyncPmTilesReader, BackendResponse, HashMapCache, PmtError, PmtResult,
};
use reqwest::header::RANGE;
use serde::{Deserialize, Serialize};

use super::tiles::{self, Area};

pub const BUILDS: &str = "https://build.protomaps.com";
/// Builds older than this are not looked for; Protomaps keeps the last days.
const LOOK_BACK_DAYS: i64 = 10;
/// Our basemap is renewed after this long: OpenStreetMap changes slowly.
const RENEW_S: i64 = 120 * 86_400;
/// Tiles fetched at the same time
const PARALLEL: usize = 16;

const NOTE: &str = "basemap.json";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("PMTiles: {0}")]
    Pmtiles(#[from] PmtError),
    #[error("request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("writing the basemap: {0}")]
    Io(#[from] std::io::Error),
    #[error("writing thread: {0}")]
    Join(#[from] tokio::task::JoinError),
    #[error("no Protomaps build of the last {LOOK_BACK_DAYS} days answers")]
    NoBuild,
}

/// What our basemap was made from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub build: String,
    /// lat, lon of spots held in the same file, to 0.01°: only in files made before the spots
    /// got archives of their own
    #[serde(default)]
    pub spots: Vec<[f64; 2]>,
    pub made_at: i64,
    /// The zoom the region was cut to (8 in files made before it was a setting)
    #[serde(default = "region_zoom")]
    pub zoom: u8,
}

fn region_zoom() -> u8 {
    tiles::REGION_ZOOM
}

/// The note next to our basemap; None for an operator's file or none at all.
pub async fn read_note(root: &Path) -> Option<Note> {
    let bytes = tokio::fs::read(root.join(NOTE)).await.ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Whether to make our basemap now: none there, ours grown old, cut to another zoom than asked
/// for, or from before the spots had their own files. A file without our note is the
/// operator's.
pub fn due(file_exists: bool, note: Option<&Note>, zoom: u8, now: i64) -> bool {
    match (file_exists, note) {
        (false, _) => true,
        (true, None) => false,
        (true, Some(n)) => !n.spots.is_empty() || n.zoom != zoom || now - n.made_at > RENEW_S,
    }
}

/// A PMTiles file on a web server, read by byte range with our HTTP client.
pub(crate) struct Http {
    client: reqwest::Client,
    url: String,
}

impl AsyncBackend for Http {
    async fn read(&self, offset: usize, length: usize) -> PmtResult<BackendResponse> {
        let fail = |e: reqwest::Error| PmtError::Reading(std::io::Error::other(e));
        let bytes = self
            .client
            .get(&self.url)
            .header(RANGE, format!("bytes={offset}-{}", offset + length - 1))
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(fail)?
            .bytes()
            .await
            .map_err(fail)?;
        Ok(BackendResponse::new(bytes))
    }
}

pub(crate) type Reader = AsyncPmTilesReader<Http, HashMapCache>;

pub(crate) async fn open(http: &reqwest::Client, url: String) -> PmtResult<Reader> {
    let backend = Http {
        client: http.clone(),
        url,
    };
    AsyncPmTilesReader::try_from_cached_source(backend, HashMapCache::default()).await
}

/// The newest build that answers: YYYYMMDD and its reader.
pub(crate) async fn newest_build(
    http: &reqwest::Client,
    base: &str,
    now: i64,
) -> Result<(String, Reader), Error> {
    for k in 0..LOOK_BACK_DAYS {
        let day = wf_core::time::yyyymmdd(now - k * 86_400).to_string();
        let url = format!("{}/{day}.pmtiles", base.trim_end_matches('/'));
        if let Ok(reader) = open(http, url).await {
            return Ok((day, reader));
        }
    }
    Err(Error::NoBuild)
}

/// Copies the tiles of `areas` from `reader` into a new archive at `target`; returns how many.
pub(crate) async fn copy(reader: Reader, areas: &[Area], target: &Path) -> Result<usize, Error> {
    let reader = Arc::new(reader);
    let (tile_type, compression) = (
        reader.get_header().tile_type,
        reader.get_header().tile_compression,
    );
    let metadata = reader.get_metadata().await?;
    let max_zoom = areas.iter().map(|a| a.max_zoom).max().unwrap_or(0);
    let [west, south, east, north] = crate::REGION;
    let target = target.to_path_buf();
    // Tiles go to a writing thread in id order, a batch fetched at a time. The writer is made
    // there: it may not cross threads.
    let (tx, mut rx) = tokio::sync::mpsc::channel::<(pmtiles::TileCoord, Vec<u8>)>(PARALLEL * 4);
    let write = tokio::task::spawn_blocking(move || -> Result<usize, Error> {
        let file = std::fs::File::create(&target)?;
        let mut writer = pmtiles::PmTilesWriter::new(tile_type)
            .tile_compression(compression)
            .min_zoom(0)
            .max_zoom(max_zoom)
            .bounds(west, south, east, north)
            .center((west + east) / 2.0, (south + north) / 2.0)
            .metadata(&metadata)
            .create(std::io::BufWriter::new(file))?;
        let mut n = 0;
        while let Some((coord, data)) = rx.blocking_recv() {
            writer.add_raw_tile(coord, &data)?;
            n += 1;
        }
        writer.finalize()?;
        Ok(n)
    });
    for batch in tiles::wanted(areas).chunks(PARALLEL) {
        let mut set = tokio::task::JoinSet::new();
        for (i, &coord) in batch.iter().enumerate() {
            let reader = reader.clone();
            set.spawn(async move { (i, reader.get_tile(coord).await.map(|t| (coord, t))) });
        }
        let mut got = Vec::with_capacity(batch.len());
        while let Some(res) = set.join_next().await {
            got.push(res?);
        }
        got.sort_by_key(|(i, _)| *i);
        for (_, res) in got {
            if let (coord, Some(data)) = res?
                && tx.send((coord, data.to_vec())).await.is_err()
            {
                break;
            }
        }
    }
    drop(tx);
    write.await?
}

/// Makes our basemap if it is due: the newest build, cut to the region up to `zoom`. Returns
/// the build used, or None when nothing was due.
pub async fn keep_current(
    root: &Path,
    http: &reqwest::Client,
    base: &str,
    zoom: u8,
    now: i64,
) -> Result<Option<String>, Error> {
    let file = super::path(root);
    let note = read_note(root).await;
    let exists = tokio::fs::try_exists(&file).await.unwrap_or(false);
    if !due(exists, note.as_ref(), zoom, now) {
        return Ok(None);
    }
    let (build, reader) = newest_build(http, base, now).await?;
    tokio::fs::create_dir_all(root).await?;
    let partial = root.join("basemap.pmtiles.part");
    let n = copy(reader, &[tiles::region(zoom)], &partial).await?;
    tokio::fs::rename(&partial, &file).await?;
    let note = Note {
        build: build.clone(),
        spots: Vec::new(),
        made_at: now,
        zoom,
    };
    tokio::fs::write(
        root.join(NOTE),
        serde_json::to_vec(&note).map_err(std::io::Error::other)?,
    )
    .await?;
    tracing::info!("basemap: {n} tiles from Protomaps build {build}");
    Ok(Some(build))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_the_operators_file_alone() {
        let ours = Note {
            build: "20261010".into(),
            spots: vec![],
            made_at: 0,
            zoom: 8,
        };
        assert!(due(false, None, 8, 0));
        // Someone else's file
        assert!(!due(true, None, 8, 0));
        assert!(!due(true, Some(&ours), 8, 86_400));
        // Old, another zoom asked for, or from before the spots had files of their own
        assert!(due(true, Some(&ours), 8, RENEW_S + 1));
        assert!(due(true, Some(&ours), 12, 86_400));
        let older = Note {
            spots: vec![[48.78, 8.08]],
            ..ours.clone()
        };
        assert!(due(true, Some(&older), 8, 86_400));
        // A note from before the zoom was a setting reads as zoom 8.
        let read: Note =
            serde_json::from_str(r#"{"build":"20261011","spots":[],"made_at":1}"#).unwrap();
        assert_eq!(read.zoom, 8);
    }
}
