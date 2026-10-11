//! Relief under the wind: elevation tiles from Mapterhorn (terrain RGB in Terrarium encoding as
//! 512 px WebP, open data from many national surveys; credited as "© Mapterhorn" with its list
//! of sources), cut to the map's region up to `ZOOM` by byte range from its planet archive into
//! `{root}/terrain.pmtiles` and renewed yearly. The map draws it as hillshade.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::fetch::{self, Error};
use super::tiles::{self, Area};

pub const DOWNLOADS: &str = "https://download.mapterhorn.com";
pub const FILE: &str = "terrain.pmtiles";
/// Deepest zoom kept: about 400 m a pixel at 48° N, enough for hillshade from afar; nearer the
/// map enlarges it, softly.
pub const ZOOM: u8 = 7;
/// Elevation hardly changes: renewed once a year.
const RENEW_S: i64 = 365 * 86_400;
const NOTE: &str = "terrain.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Note {
    made_at: i64,
}

pub fn path(root: &Path) -> PathBuf {
    root.join(FILE)
}

/// Makes the relief if there is none or it is a year old; returns how many tiles it holds, or
/// None when nothing was due. A file without our note is the operator's and stays.
pub async fn keep_current(
    root: &Path,
    http: &reqwest::Client,
    base: &str,
    now: i64,
) -> Result<Option<usize>, Error> {
    let exists = tokio::fs::try_exists(path(root)).await.unwrap_or(false);
    let note: Option<Note> = tokio::fs::read(root.join(NOTE))
        .await
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    let due = match (exists, &note) {
        (false, _) => true,
        (true, None) => false,
        (true, Some(n)) => now - n.made_at > RENEW_S,
    };
    if !due {
        return Ok(None);
    }
    let url = format!("{}/planet.pmtiles", base.trim_end_matches('/'));
    let reader = fetch::open(http, url).await?;
    let region = Area {
        max_zoom: ZOOM,
        ..tiles::region()
    };
    tokio::fs::create_dir_all(root).await?;
    let partial = root.join("terrain.pmtiles.part");
    let n = fetch::copy(reader, &[region], &partial).await?;
    tokio::fs::rename(&partial, path(root)).await?;
    let note = serde_json::to_vec(&Note { made_at: now }).map_err(std::io::Error::other)?;
    tokio::fs::write(root.join(NOTE), note).await?;
    tracing::info!("terrain: {n} relief tiles from Mapterhorn");
    Ok(Some(n))
}

/// The relief's zooms and bounds, if there is one.
pub async fn info(root: &Path) -> Option<super::BasemapInfo> {
    use tokio::io::AsyncReadExt;
    let mut file = tokio::fs::File::open(path(root)).await.ok()?;
    let mut bytes = [0u8; 127];
    file.read_exact(&mut bytes).await.ok()?;
    super::read_header(&bytes)
}
