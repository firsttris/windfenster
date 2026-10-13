//! Relief under the wind: elevation tiles from Mapterhorn (terrain RGB in Terrarium encoding as
//! 512 px WebP, open data from many national surveys; credited as "© Mapterhorn" with its list
//! of sources), the whole world up to `ZOOM` by byte range from its planet archive into
//! `{root}/terrain.pmtiles`, renewed yearly. The map draws it as hillshade.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::download;
use super::fetch::{self, Error};
use super::tiles::{self, Area};

pub const DOWNLOADS: &str = "https://download.mapterhorn.com";
pub const FILE: &str = "terrain.pmtiles";
/// Deepest zoom kept by default (`WF_TERRAIN_ZOOM`): about 400 m a pixel at 48° N, enough for
/// hillshade from afar; nearer the map enlarges it, softly. Each zoom more is about four times
/// the size; Mapterhorn's planet archive goes to 12.
pub const ZOOM: u8 = 7;
pub const MAX_ZOOM: u8 = 12;
/// Elevation hardly changes: renewed once a year.
const RENEW_S: i64 = 365 * 86_400;
const NOTE: &str = "terrain.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Note {
    made_at: i64,
    #[serde(default = "default_zoom")]
    zoom: u8,
    /// The whole world; false in files of Europe alone, from before the map went worldwide
    #[serde(default)]
    world: bool,
}

fn default_zoom() -> u8 {
    ZOOM
}

pub fn path(root: &Path) -> PathBuf {
    root.join(FILE)
}

/// Makes the relief up to `zoom` if there is none, it is a year old, cut to another zoom or
/// Europe alone;
/// returns what it made, or None when nothing was due. Zoom 0 removes ours (no
/// relief). A file without our note is the operator's and stays.
pub async fn keep_current(
    root: &Path,
    http: &reqwest::Client,
    base: &str,
    zoom: u8,
    now: i64,
) -> Result<Option<String>, Error> {
    let exists = tokio::fs::try_exists(path(root)).await.unwrap_or(false);
    let note: Option<Note> = tokio::fs::read(root.join(NOTE))
        .await
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    if zoom == 0 {
        if note.is_some() {
            tokio::fs::remove_file(path(root)).await.ok();
            tokio::fs::remove_file(root.join(NOTE)).await?;
        }
        return Ok(None);
    }
    let due = match (exists, &note) {
        (false, _) => true,
        (true, None) => false,
        (true, Some(n)) => !n.world || n.zoom != zoom || now - n.made_at > RENEW_S,
    };
    if !due {
        return Ok(None);
    }
    let url = format!("{}/planet.pmtiles", base.trim_end_matches('/'));
    let reader = fetch::open(http, url.clone()).await?;
    let world: Area = tiles::world(zoom.min(MAX_ZOOM));
    tokio::fs::create_dir_all(root).await?;
    let part = download::Part::of(root, FILE);
    // To Mapterhorn's own deepest zoom the relief is the planet file itself: fetched whole.
    let made = if tiles::whole_source(&[world], reader.get_header().max_zoom) {
        drop(reader);
        let n = download::whole(&url, &part, "terrain").await?;
        download::check_archive(&part.file, n).await?;
        part.done().await;
        format!("the whole planet file ({})", download::gb(n))
    } else {
        part.clear().await;
        let n = super::cut::copy(reader, &[world], &part.file).await?;
        format!("{n} relief tiles")
    };
    tokio::fs::rename(&part.file, path(root)).await?;
    let note = serde_json::to_vec(&Note {
        made_at: now,
        zoom,
        world: true,
    })
    .map_err(std::io::Error::other)?;
    tokio::fs::write(root.join(NOTE), note).await?;
    tracing::info!("terrain: {made} from Mapterhorn");
    Ok(Some(made))
}

/// The relief's zooms and bounds, if there is one.
pub async fn info(root: &Path) -> Option<super::BasemapInfo> {
    use tokio::io::AsyncReadExt;
    let mut file = tokio::fs::File::open(path(root)).await.ok()?;
    let mut bytes = [0u8; 127];
    file.read_exact(&mut bytes).await.ok()?;
    super::read_header(&bytes)
}
