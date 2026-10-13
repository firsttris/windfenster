//! Puts a basemap in place without the operator doing anything: from the newest Protomaps daily
//! build, the whole world from afar and a region deeper, read by byte range (pmtiles crate) and
//! written to one archive. The spots people look at get archives of their own (`spots.rs`). A file the
//! operator put there is never touched; ours carries a note next to it (`basemap.json`) and is
//! renewed when it grows old.

use std::path::Path;
use std::time::Duration;

use pmtiles::{AsyncBackend, AsyncPmTilesReader, BackendResponse, PmtError, PmtResult};

use super::dircache::Bounded;
use reqwest::header::RANGE;
use serde::{Deserialize, Serialize};

use super::download;
use super::tiles::{self, Area};

pub const BUILDS: &str = "https://build.protomaps.com";
/// Builds older than this are not looked for; Protomaps keeps the last days.
const LOOK_BACK_DAYS: i64 = 10;
/// Our basemap is renewed after this long: OpenStreetMap changes slowly.
const RENEW_S: i64 = 120 * 86_400;

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
    #[error("{0} is gone")]
    Gone(String),
    #[error(
        "not enough room on the disk: {} needed, {} free; free some, or ask for less zoom",
        super::download::gb(*need),
        super::download::gb(*free)
    )]
    NoSpace { need: u64, free: u64 },
    #[error("{0}")]
    Incomplete(String),
}

/// What our basemap holds: the world to `world_zoom` and a region (west, south, east, north)
/// deeper, to its zoom.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cut {
    pub world_zoom: u8,
    pub region: Option<([f64; 4], u8)>,
}

impl Cut {
    /// The world, and a region only where it goes deeper.
    pub fn new(world_zoom: u8, region: Option<[f64; 4]>, region_zoom: u8) -> Self {
        Cut {
            world_zoom,
            region: region
                .filter(|_| region_zoom > world_zoom)
                .map(|b| (b, region_zoom)),
        }
    }

    pub fn areas(&self) -> Vec<Area> {
        std::iter::once(tiles::world(self.world_zoom))
            .chain(self.region.map(|(b, z)| tiles::region(b, z)))
            .collect()
    }

    /// The deepest zoom held everywhere in the region (or the world)
    pub fn region_zoom(&self) -> u8 {
        self.region.map_or(self.world_zoom, |(_, z)| z)
    }
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
    /// The world's zoom; none in files of Europe alone, made before the map went worldwide
    #[serde(default)]
    pub world_zoom: Option<u8>,
    /// The region held deeper than the world
    #[serde(default)]
    pub region: Option<[f64; 4]>,
}

fn region_zoom() -> u8 {
    tiles::REGION_ZOOM
}

impl Note {
    /// What the file holds; None for a file of Europe alone.
    pub fn cut(&self) -> Option<Cut> {
        Some(Cut {
            world_zoom: self.world_zoom?,
            region: self.region.map(|b| (b, self.zoom)),
        })
    }
}

/// The note next to our basemap; None for an operator's file or none at all.
pub async fn read_note(root: &Path) -> Option<Note> {
    let bytes = tokio::fs::read(root.join(NOTE)).await.ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Whether to make our basemap now: none there, ours grown old, cut otherwise than asked for
/// (zooms, region, or Europe alone from before the world), or from before the spots had their
/// own files. A file without our note is the operator's.
pub fn due(file_exists: bool, note: Option<&Note>, cut: &Cut, now: i64) -> bool {
    match (file_exists, note) {
        (false, _) => true,
        (true, None) => false,
        (true, Some(n)) => {
            !n.spots.is_empty() || n.cut().as_ref() != Some(cut) || now - n.made_at > RENEW_S
        }
    }
}

/// A PMTiles file on a web server, read by byte range with our HTTP client.
pub(crate) struct Http {
    client: reqwest::Client,
    url: String,
    /// The first wait before asking again; it doubles each time
    backoff: Duration,
}

/// Tries per range: a cut of a deep region is millions of requests over hours, and one 502 or
/// connection cut among them ended the whole cut (seen against build.protomaps.com and
/// download.mapterhorn.com, which answer 5xx now and then).
/// Outages behind Cloudflare (520, 502) last minutes at times, and all ranges in flight fail at
/// once: waiting longer costs nothing, giving up costs hours of cutting.
pub(crate) const TRIES: u32 = 10;
/// The first wait; it doubles up to `MAX_WAIT`: 1, 2, 4, 8, 16, 32, 60, 60, 60 s, about five
/// minutes before a range gives up
pub(crate) const BACKOFF: Duration = Duration::from_secs(1);
pub(crate) const MAX_WAIT: Duration = Duration::from_secs(60);

/// A wait stretched by up to half again, by the range's offset, so the ranges in flight do not
/// all knock again at the same moment.
pub(crate) fn spread(wait: Duration, offset: usize) -> Duration {
    let share = (offset as u64).wrapping_mul(2_654_435_761) % 1_000;
    wait + wait.mul_f64(share as f64 / 2_000.0)
}

/// Whether asking again may help: the server or the way to it failed for a moment (5xx, 429,
/// a timeout, a cut connection, a body cut short). A 404 or 416 stays what it is: a build not
/// out, or a range past the file.
fn passing(e: &reqwest::Error) -> bool {
    match e.status() {
        Some(s) => s.is_server_error() || s == reqwest::StatusCode::TOO_MANY_REQUESTS,
        None => true,
    }
}

impl Http {
    async fn range(&self, offset: usize, length: usize) -> reqwest::Result<BackendResponse> {
        let bytes = self
            .client
            .get(&self.url)
            .header(RANGE, format!("bytes={offset}-{}", offset + length - 1))
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        Ok(BackendResponse::new(bytes))
    }
}

impl AsyncBackend for Http {
    async fn read(&self, offset: usize, length: usize) -> PmtResult<BackendResponse> {
        let mut wait = self.backoff;
        for attempt in 1.. {
            match self.range(offset, length).await {
                Ok(response) => return Ok(response),
                Err(e) if attempt < TRIES && passing(&e) => {
                    let pause = spread(wait, offset);
                    tracing::debug!("{}: {e}; again in {pause:?}", self.url);
                    tokio::time::sleep(pause).await;
                    wait = (wait * 2).min(MAX_WAIT);
                }
                Err(e) => return Err(PmtError::Reading(std::io::Error::other(e))),
            }
        }
        unreachable!("the loop returns")
    }
}

pub(crate) type Reader = AsyncPmTilesReader<Http, Bounded>;

pub(crate) async fn open(http: &reqwest::Client, url: String) -> PmtResult<Reader> {
    let backend = Http {
        client: http.clone(),
        url,
        backoff: BACKOFF,
    };
    AsyncPmTilesReader::try_from_cached_source(backend, Bounded::new()).await
}

fn build_url(base: &str, build: &str) -> String {
    format!("{}/{build}.pmtiles", base.trim_end_matches('/'))
}

/// The build a download under way is from, if it still answers; else the newest.
async fn build_to_use(
    http: &reqwest::Client,
    base: &str,
    part: &download::Part,
    now: i64,
) -> Result<(String, Reader), Error> {
    if let Some(url) = part.source().await
        && let Some(build) = url
            .rsplit('/')
            .next()
            .and_then(|f| f.strip_suffix(".pmtiles"))
        && let Ok(reader) = open(http, url.clone()).await
    {
        return Ok((build.to_owned(), reader));
    }
    newest_build(http, base, now).await
}

/// The newest build that answers: YYYYMMDD and its reader.
pub(crate) async fn newest_build(
    http: &reqwest::Client,
    base: &str,
    now: i64,
) -> Result<(String, Reader), Error> {
    for k in 0..LOOK_BACK_DAYS {
        let day = wf_core::time::yyyymmdd(now - k * 86_400).to_string();
        if let Ok(reader) = open(http, build_url(base, &day)).await {
            return Ok((day, reader));
        }
    }
    Err(Error::NoBuild)
}

/// Makes our basemap if it is due: the newest build, cut to the world and the region, or
/// downloaded whole when the cut is all of it. Returns the build used, or None when nothing was
/// due.
pub async fn keep_current(
    root: &Path,
    http: &reqwest::Client,
    base: &str,
    cut: &Cut,
    now: i64,
) -> Result<Option<String>, Error> {
    let file = super::path(root);
    let note = read_note(root).await;
    let exists = tokio::fs::try_exists(&file).await.unwrap_or(false);
    if !due(exists, note.as_ref(), cut, now) {
        return Ok(None);
    }
    tokio::fs::create_dir_all(root).await?;
    let part = download::Part::of(root, super::FILE);
    let areas = cut.areas();
    let (mut build, reader) = build_to_use(http, base, &part, now).await?;
    let made = if tiles::whole_source(&areas, reader.get_header().max_zoom) {
        drop(reader);
        let n = match download::whole(&build_url(base, &build), &part, "basemap").await {
            // Protomaps keeps a build some days: one gone meanwhile gives way to the newest.
            Err(Error::Gone(_)) => {
                part.clear().await;
                build = newest_build(http, base, now).await?.0;
                download::whole(&build_url(base, &build), &part, "basemap").await?
            }
            other => other?,
        };
        download::check_archive(&part.file, n).await?;
        part.done().await;
        format!("the whole build ({})", download::gb(n))
    } else {
        part.clear().await;
        super::cut::room_for_cut(http, &build_url(base, &build), &reader, &areas, root).await?;
        let n = super::cut::copy(reader, &areas, &part.file).await?;
        format!("{n} tile entries")
    };
    tokio::fs::rename(&part.file, &file).await?;
    let note = Note {
        build: build.clone(),
        spots: Vec::new(),
        made_at: now,
        zoom: cut.region_zoom(),
        world_zoom: Some(cut.world_zoom),
        region: cut.region.map(|(b, _)| b),
    };
    tokio::fs::write(
        root.join(NOTE),
        serde_json::to_vec(&note).map_err(std::io::Error::other)?,
    )
    .await?;
    tracing::info!("basemap: {made} from Protomaps build {build}");
    Ok(Some(build))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::http::StatusCode;

    use super::*;

    /// A server that answers `fails` times with `status`, then with the range's bytes; returns
    /// its address and how often it was asked.
    async fn flaky(fails: usize, status: StatusCode) -> (String, Arc<AtomicUsize>) {
        let asked = Arc::new(AtomicUsize::new(0));
        let count = asked.clone();
        let app = axum::Router::new().route(
            "/f.pmtiles",
            axum::routing::get(move || {
                let n = count.fetch_add(1, Ordering::SeqCst);
                async move {
                    if n < fails {
                        (status, Vec::new())
                    } else {
                        (StatusCode::PARTIAL_CONTENT, b"tile".to_vec())
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/f.pmtiles", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (url, asked)
    }

    fn http(url: String) -> Http {
        Http {
            client: reqwest::Client::new(),
            url,
            backoff: Duration::from_millis(1),
        }
    }

    #[test]
    fn waits_spread_and_capped() {
        let w = Duration::from_secs(8);
        for offset in [0, 1, 127, 1 << 20, usize::MAX] {
            let p = spread(w, offset);
            assert!(p >= w && p < w + w / 2, "{offset}: {p:?}");
        }
        assert_ne!(spread(w, 1), spread(w, 2));
        // The waits of all tries: doubling, then a minute each, about five minutes in all
        let mut wait = BACKOFF;
        let mut all = Duration::ZERO;
        for _ in 1..TRIES {
            all += wait;
            wait = (wait * 2).min(MAX_WAIT);
        }
        assert_eq!(
            all,
            Duration::from_secs(1 + 2 + 4 + 8 + 16 + 32 + 60 + 60 + 60)
        );
    }

    #[tokio::test]
    async fn asks_again_when_the_server_fails_for_a_moment() {
        // Two 502s, then the bytes: one read, three requests
        let (url, asked) = flaky(2, StatusCode::BAD_GATEWAY).await;
        let got = http(url).read(0, 4).await.unwrap();
        assert_eq!(&got.bytes[..], b"tile");
        assert_eq!(asked.load(Ordering::SeqCst), 3);
        // Busy (429) counts as a moment too
        let (url, _) = flaky(1, StatusCode::TOO_MANY_REQUESTS).await;
        assert!(http(url).read(0, 4).await.is_ok());
        // A server that stays down: given up after the tries
        let (url, asked) = flaky(usize::MAX, StatusCode::SERVICE_UNAVAILABLE).await;
        assert!(http(url).read(0, 4).await.is_err());
        assert_eq!(asked.load(Ordering::SeqCst), TRIES as usize);
        // A build that is not out: no second ask
        let (url, asked) = flaky(usize::MAX, StatusCode::NOT_FOUND).await;
        assert!(http(url).read(0, 4).await.is_err());
        assert_eq!(asked.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn leaves_the_operators_file_alone() {
        let europe = [-25.0, 27.0, 45.0, 72.0];
        let cut = Cut::new(6, Some(europe), 8);
        let ours = Note {
            build: "20261010".into(),
            spots: vec![],
            made_at: 0,
            zoom: 8,
            world_zoom: Some(6),
            region: Some(europe),
        };
        assert_eq!(ours.cut(), Some(cut));
        assert!(due(false, None, &cut, 0));
        // Someone else's file
        assert!(!due(true, None, &cut, 0));
        assert!(!due(true, Some(&ours), &cut, 86_400));
        // Old, another zoom or region asked for, or from before the spots had files of their own
        assert!(due(true, Some(&ours), &cut, RENEW_S + 1));
        assert!(due(
            true,
            Some(&ours),
            &Cut::new(6, Some(europe), 12),
            86_400
        ));
        assert!(due(
            true,
            Some(&ours),
            &Cut::new(7, Some(europe), 8),
            86_400
        ));
        assert!(due(true, Some(&ours), &Cut::new(6, None, 8), 86_400));
        let older = Note {
            spots: vec![[48.78, 8.08]],
            ..ours.clone()
        };
        assert!(due(true, Some(&older), &cut, 86_400));
        // A note from before the world (and the zoom setting): Europe alone at zoom 8, cut anew
        let read: Note =
            serde_json::from_str(r#"{"build":"20261011","spots":[],"made_at":1}"#).unwrap();
        assert_eq!((read.zoom, read.cut()), (8, None));
        assert!(due(true, Some(&read), &cut, 86_400));
    }

    #[test]
    fn holds_a_region_only_deeper_than_the_world() {
        let europe = [-25.0, 27.0, 45.0, 72.0];
        assert_eq!(Cut::new(6, Some(europe), 8).areas().len(), 2);
        assert_eq!(Cut::new(8, Some(europe), 8), Cut::new(8, None, 0));
        assert_eq!(Cut::new(8, Some(europe), 6).region_zoom(), 8);
        assert_eq!(Cut::new(6, Some(europe), 12).region_zoom(), 12);
    }
}
