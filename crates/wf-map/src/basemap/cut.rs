//! Cutting a basemap or relief out of a larger archive, tile by tile: the tiles of the areas
//! read by range in the order of their ids and written as they come (`writer`), with memory
//! that stays flat and the log saying how far it is.

use std::path::Path;
use std::sync::Arc;

use super::download;
use super::fetch::{Error, Reader};
use super::tiles::{self, Area};
use super::writer;

/// Tiles fetched at the same time
const PARALLEL: usize = 16;

/// Copies the tiles of `areas` from `reader` into a new archive at `target`; returns how many
/// entries it holds. Memory stays flat however many tiles (`writer`); the log says how far it
/// is every few minutes.
pub(crate) async fn copy(
    reader: Reader,
    areas: &[Area],
    target: &Path,
    what: &'static str,
) -> Result<u64, Error> {
    let reader = Arc::new(reader);
    let header = reader.get_header();
    let max_zoom = areas.iter().map(|a| a.max_zoom).max().unwrap_or(0);
    let min = |f: fn(&Area) -> f64| areas.iter().map(f).fold(f64::INFINITY, f64::min);
    let max = |f: fn(&Area) -> f64| areas.iter().map(f).fold(f64::NEG_INFINITY, f64::max);
    let owned = areas.to_vec();
    let expected =
        tokio::task::spawn_blocking(move || tiles::wanted(&owned).count() as u64).await?;
    let spec = writer::Spec {
        tile_type: header.tile_type,
        tile_compression: header.tile_compression,
        min_zoom: 0,
        max_zoom,
        bounds: [
            min(|a| a.west),
            min(|a| a.south),
            max(|a| a.east),
            max(|a| a.north),
        ],
        metadata: reader.get_metadata().await?,
        expected,
    };
    let target = target.to_path_buf();
    // Tiles go to a writing thread in id order, a batch fetched at a time.
    let (tx, mut rx) = tokio::sync::mpsc::channel::<(u64, Vec<u8>)>(PARALLEL * 4);
    let write = tokio::task::spawn_blocking(move || -> Result<u64, Error> {
        let mut w = writer::Writer::create(&target, spec)?;
        while let Some((id, data)) = rx.blocking_recv() {
            w.add(id, &data)?;
        }
        Ok(w.finish()?)
    });
    let began = std::time::Instant::now();
    let mut said = began;
    let mut walked = 0u64;
    let _shown = super::work::Guard(what.to_owned());
    super::work::begin(what, "tiles", 0, expected);
    // A batch at a time from the tiles as they are made: the world deep is too many to list.
    let mut tiles = tiles::wanted(areas);
    loop {
        let batch: Vec<_> = tiles.by_ref().take(PARALLEL).collect();
        if batch.is_empty() {
            break;
        }
        walked += batch.len() as u64;
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
                && tx
                    .send((pmtiles::TileId::from(coord).value(), data.to_vec()))
                    .await
                    .is_err()
            {
                break;
            }
        }
        super::work::advance(what, walked);
        if said.elapsed() >= REPORT {
            said = std::time::Instant::now();
            let rate = walked as f64 / began.elapsed().as_secs_f64().max(1.0);
            let left = (expected.saturating_sub(walked)) as f64 / rate.max(1.0) / 60.0;
            tracing::info!(
                "{what}: {walked} of {expected} tiles, {rate:.0} a second, about {left:.0} min to go"
            );
        }
    }
    drop(tx);
    write.await?
}

/// Whether the disk has room for a cut of `areas` from the archive at `url`: its size times
/// the share of its tiles the areas ask for (more than they get: empty sea is not stored).
pub(crate) async fn room_for_cut(
    http: &reqwest::Client,
    url: &str,
    reader: &Reader,
    areas: &[Area],
    dir: &Path,
) -> Result<(), Error> {
    let Ok(resp) = http.head(url).send().await else {
        return Ok(());
    };
    let Some(size) = resp.content_length() else {
        return Ok(());
    };
    let Some(addressed) = reader.get_header().n_addressed_tiles() else {
        return Ok(());
    };
    let owned = areas.to_vec();
    let wanted = tokio::task::spawn_blocking(move || tiles::wanted(&owned).count() as u64).await?;
    let share = (wanted as f64 / addressed.get() as f64).min(1.0);
    let need = (size as f64 * share) as u64;
    let free = download::free_bytes(dir)?;
    if free < need + (1 << 30) {
        return Err(Error::NoSpace { need, free });
    }
    Ok(())
}

/// How often a long cut says how far it is
const REPORT: std::time::Duration = std::time::Duration::from_secs(300);
