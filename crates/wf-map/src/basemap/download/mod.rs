//! Fetching a whole PMTiles archive as it is, when the cut asked for is all of it (the world to
//! Protomaps' zoom 15, 139 GB; the relief to Mapterhorn's 12, 356 GB): at 35 to 60 MB/s instead
//! of a range request per tile at a few hundred tiles a second.
//!
//! Both sources sit behind Cloudflare, where one GET over a whole file was seen to stall after
//! some 12 GB: open, but sending nothing, and asking again on that connection got no further,
//! while a new connection went on at full speed at once. So the file comes in pieces of 512
//! MiB, each its own range request on a connection of its own (no pooling), written at its
//! place in `{name}.part`. Less than 1 MB/s for 20 s ends a piece's request, and it is asked
//! again on a new connection from the byte it reached; a piece counts only when complete
//! (206, the range asked for, all its bytes). Source, ETag, length and the pieces done are
//! kept in `{name}.part.json`: after an error or a restart only the missing pieces are
//! fetched, once the last MiB before them is checked against the source. A source that changed
//! (another ETag, a 200 instead of 206, bytes that differ) starts the download over. A
//! moment's failure (5xx, 429, a cut or stalled connection) is retried without end, waiting 5
//! to 60 s; only 30 minutes without a single piece done end the round, and the next goes on
//! from the pieces there.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use reqwest::StatusCode;
use reqwest::header::{
    ACCEPT_ENCODING, CONTENT_RANGE, ETAG, HeaderMap, IF_RANGE, LAST_MODIFIED, RANGE,
};
use serde::{Deserialize, Serialize};

use super::fetch::Error;

mod piece;
pub(crate) mod progress;
#[cfg(test)]
mod tests;

use piece::piece;
pub(crate) use progress::{Progress, free_bytes, gb};

/// How the download goes; short in tests.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Pace {
    /// Bytes per piece
    pub piece: u64,
    /// Pieces fetched at once: one connection gave some 45 MB/s, more only if it helps
    pub parallel: usize,
    /// Over this long a request must bring `min_rate`, else it is taken as hung
    pub window: Duration,
    /// Bytes a second
    pub min_rate: u64,
    /// The first wait after a failure; it doubles up to `max_wait`
    pub backoff: Duration,
    pub max_wait: Duration,
    /// This long without a piece done ends the round
    pub give_up: Duration,
}

pub(crate) const PACE: Pace = Pace {
    piece: 512 << 20,
    parallel: 1,
    window: Duration::from_secs(20),
    min_rate: 1_000_000,
    backoff: Duration::from_secs(5),
    max_wait: Duration::from_secs(60),
    give_up: Duration::from_secs(30 * 60),
};

/// Bytes compared with the source before going on with a download from an earlier round
const CHECK: u64 = 1 << 20;
/// Room kept free on the disk beside the download
const MARGIN: u64 = 1 << 30;
/// How often the log says how far it is
const REPORT: Duration = Duration::from_secs(300);
/// Times a source may change under a download before giving up for this round
const RESTARTS: u32 = 3;
/// User agent of the download's own client
const AGENT: &str = concat!("windfenster/", env!("CARGO_PKG_VERSION"));
/// Bytes gathered before each write
pub(super) const WRITE: usize = 8 << 20;

/// Where an interrupted download stands.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(super) struct State {
    pub url: String,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub total: u64,
    pub piece: u64,
    /// Pieces complete, by number
    pub done: BTreeSet<u64>,
}

impl State {
    fn pieces(&self) -> u64 {
        self.total.div_ceil(self.piece.max(1))
    }

    pub fn range(&self, n: u64) -> (u64, u64) {
        let from = n * self.piece;
        (from, (from + self.piece).min(self.total) - 1)
    }

    fn bytes_done(&self) -> u64 {
        self.done
            .iter()
            .map(|&n| self.range(n).1 - self.range(n).0 + 1)
            .sum()
    }
}

/// The part file and the note on it for a target `name` in `root`.
pub(crate) struct Part {
    pub file: PathBuf,
    note: PathBuf,
}

impl Part {
    pub fn of(root: &Path, name: &str) -> Self {
        Part {
            file: root.join(format!("{name}.part")),
            note: root.join(format!("{name}.part.json")),
        }
    }

    async fn state(&self) -> Option<State> {
        let bytes = tokio::fs::read(&self.note).await.ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// The source of a download under way, to go on with it rather than start another.
    pub async fn source(&self) -> Option<String> {
        tokio::fs::metadata(&self.file).await.ok()?;
        self.state().await.map(|s| s.url)
    }

    async fn save(&self, state: &State) -> Result<(), Error> {
        let bytes = serde_json::to_vec(state).map_err(std::io::Error::other)?;
        let tmp = self.note.with_extension("json.tmp");
        tokio::fs::write(&tmp, bytes).await?;
        tokio::fs::rename(&tmp, &self.note).await?;
        Ok(())
    }

    pub async fn clear(&self) {
        tokio::fs::remove_file(&self.file).await.ok();
        tokio::fs::remove_file(&self.note).await.ok();
    }

    /// Done: the note goes, the file is the caller's to rename.
    pub async fn done(&self) {
        tokio::fs::remove_file(&self.note).await.ok();
    }
}

/// How fetching a piece ended
pub(super) enum Piece {
    Done,
    /// The source changed: begin the whole download again
    Restart,
}

/// Downloads `url` whole into `part.file`, going on with the pieces there; returns its length.
/// `what` names it in the log.
pub(crate) async fn whole(url: &str, part: &Part, what: &str) -> Result<u64, Error> {
    // A connection of its own per request: a pooled one may be the one that hangs.
    let http = reqwest::Client::builder()
        .user_agent(AGENT)
        .pool_max_idle_per_host(0)
        .connect_timeout(Duration::from_secs(20))
        .build()?;
    whole_with(&http, url, part, what, PACE).await
}

async fn whole_with(
    http: &reqwest::Client,
    url: &str,
    part: &Part,
    what: &str,
    pace: Pace,
) -> Result<u64, Error> {
    for _ in 0..=RESTARTS {
        let state = match part.state().await {
            Some(s)
                if s.url == url && part.file.exists() && still(http, &s, part, pace).await? =>
            {
                s
            }
            _ => {
                part.clear().await;
                begin(http, url, part, pace).await?
            }
        };
        match pieces(http, part, state, what, pace).await? {
            Some(total) => return Ok(total),
            None => {
                tracing::info!("{what}: the source changed, downloading it from the start");
                part.clear().await;
            }
        }
    }
    Err(Error::Incomplete(format!("{url} keeps changing")))
}

/// Asks the source for its length and ETag (the first byte by range) and lays out the part.
async fn begin(http: &reqwest::Client, url: &str, part: &Part, pace: Pace) -> Result<State, Error> {
    let (mut wait, began) = (pace.backoff, Instant::now());
    let resp = loop {
        let sent = request(http, url, "bytes=0-0").send().await;
        match sent {
            Ok(r) if !passing_status(r.status()) => break r,
            _ if began.elapsed() > pace.give_up => {
                return Err(Error::Incomplete(format!("{url} does not answer")));
            }
            _ => {}
        }
        tokio::time::sleep(wait).await;
        wait = (wait * 2).min(pace.max_wait);
    };
    match resp.status() {
        StatusCode::NOT_FOUND | StatusCode::GONE => return Err(Error::Gone(url.to_owned())),
        StatusCode::PARTIAL_CONTENT => {}
        s => {
            return Err(Error::Incomplete(format!(
                "{url} answered {s} to a range: it cannot be fetched in pieces"
            )));
        }
    }
    let headers = resp.headers();
    let Some((_, _, total)) = content_range(headers) else {
        return Err(Error::Incomplete(format!("{url} gives no length")));
    };
    let state = State {
        url: url.to_owned(),
        etag: text(headers, ETAG),
        last_modified: text(headers, LAST_MODIFIED),
        total,
        piece: pace.piece,
        done: BTreeSet::new(),
    };
    let dir = part.file.parent().unwrap_or(Path::new("."));
    tokio::fs::create_dir_all(dir).await?;
    let free = free_bytes(dir)?;
    if free < total + MARGIN {
        return Err(Error::NoSpace { need: total, free });
    }
    // The file at its full length, the pieces written into it where they belong
    let file = tokio::fs::File::create(&part.file).await?;
    file.set_len(total).await?;
    part.save(&state).await?;
    Ok(state)
}

/// Fetches the missing pieces, `pace.parallel` at once. Some(length) when all are in, None
/// when the source changed.
async fn pieces(
    http: &reqwest::Client,
    part: &Part,
    state: State,
    what: &str,
    pace: Pace,
) -> Result<Option<u64>, Error> {
    let total = state.total;
    let missing: Vec<u64> = (0..state.pieces())
        .filter(|n| !state.done.contains(n))
        .collect();
    let done_before = state.bytes_done();
    if done_before > 0 {
        tracing::info!(
            "{what}: going on with the download, {} of {} there",
            gb(done_before),
            gb(total)
        );
    } else {
        tracing::info!("{what}: downloading {} whole, {}", state.url, gb(total));
    }
    let file = Arc::new(std::fs::OpenOptions::new().write(true).open(&part.file)?);
    let state = Arc::new(Mutex::new(state));
    let got = Arc::new(AtomicU64::new(0));
    let mut progress = Progress::new(what, done_before);
    progress.total = Some(total);
    let _shown = super::work::Guard(what.to_owned());
    super::work::begin(what, "bytes", done_before, total);
    let mut queue = missing.into_iter();
    let mut set = tokio::task::JoinSet::new();
    let mut tick = tokio::time::interval(REPORT.min(pace.give_up));
    tick.tick().await;
    let mut last_done = Instant::now();
    loop {
        while set.len() < pace.parallel {
            let Some(n) = queue.next() else { break };
            let (http, file, got) = (http.clone(), file.clone(), got.clone());
            let s = state.lock().expect("state").clone();
            set.spawn(async move { (n, piece(&http, &s, n, &file, &got, pace).await) });
        }
        if set.is_empty() {
            break;
        }
        tokio::select! {
            joined = set.join_next() => {
                let Some(joined) = joined else { break };
                match joined? {
                    (n, Ok(Piece::Done)) => {
                        let s = {
                            let mut s = state.lock().expect("state");
                            s.done.insert(n);
                            s.clone()
                        };
                        part.save(&s).await?;
                        super::work::advance(what, (s.done.len() as u64 * s.piece).min(total));
                        last_done = Instant::now();
                    }
                    (_, Ok(Piece::Restart)) => {
                        set.abort_all();
                        return Ok(None);
                    }
                    (_, Err(e)) => {
                        set.abort_all();
                        return Err(e);
                    }
                }
            }
            _ = tick.tick() => {
                if last_done.elapsed() > pace.give_up {
                    set.abort_all();
                    return Err(Error::Incomplete(format!(
                        "no piece done in {:?}; going on next round",
                        pace.give_up
                    )));
                }
                progress.tick(done_before + got.load(Ordering::Relaxed));
            }
        }
    }
    file.sync_data()?;
    Ok(Some(total))
}

/// A GET for `range` on a connection that is closed after it
pub(super) fn request(http: &reqwest::Client, url: &str, range: &str) -> reqwest::RequestBuilder {
    http.get(url)
        .header(RANGE, range)
        // The bytes as stored: a range of a compressed body would be no range of the file
        .header(ACCEPT_ENCODING, "identity")
        .header(reqwest::header::CONNECTION, "close")
}

/// Whether a part from an earlier round still is the source: the last MiB before the end of
/// its pieces done, asked again and compared. A part with nothing done is taken as is.
async fn still(
    http: &reqwest::Client,
    state: &State,
    part: &Part,
    pace: Pace,
) -> Result<bool, Error> {
    let Some(&last) = state.done.iter().next_back() else {
        return Ok(true);
    };
    let end = state.range(last).1;
    let from = end.saturating_sub(CHECK - 1);
    let mut wait = pace.backoff;
    let began = Instant::now();
    let resp = loop {
        let mut req = request(http, &state.url, &format!("bytes={from}-{end}"));
        if let Some(v) = validator(state) {
            req = req.header(IF_RANGE, v);
        }
        match req.send().await {
            Ok(r) if !passing_status(r.status()) => break r,
            _ if began.elapsed() > pace.give_up => {
                return Err(Error::Incomplete(format!("{} does not answer", state.url)));
            }
            _ => {}
        }
        tokio::time::sleep(wait).await;
        wait = (wait * 2).min(pace.max_wait);
    };
    match resp.status() {
        StatusCode::PARTIAL_CONTENT
            if content_range(resp.headers()) == Some((from, end, state.total)) => {}
        StatusCode::NOT_FOUND | StatusCode::GONE => return Err(Error::Gone(state.url.clone())),
        _ => return Ok(false),
    }
    let theirs = resp.bytes().await?;
    let file = std::fs::File::open(&part.file)?;
    let ours = tokio::task::spawn_blocking(move || {
        use std::os::unix::fs::FileExt;
        let mut buf = vec![0; (end - from + 1) as usize];
        file.read_exact_at(&mut buf, from).map(|()| buf)
    })
    .await??;
    Ok(theirs[..] == ours[..])
}

pub(super) fn passing_status(s: StatusCode) -> bool {
    s.is_server_error() || s == StatusCode::TOO_MANY_REQUESTS
}

/// What If-Range takes: a strong ETag, else the date (a weak ETag may not be used there)
pub(super) fn validator(state: &State) -> Option<String> {
    state
        .etag
        .clone()
        .filter(|e| !e.starts_with("W/"))
        .or_else(|| state.last_modified.clone())
}

/// Whether the source is the one the part came from, by what both name
pub(super) fn same(old: &State, new: &State) -> bool {
    match (&old.etag, &new.etag) {
        (Some(a), Some(b)) => a == b,
        _ => match (&old.last_modified, &new.last_modified) {
            (Some(a), Some(b)) => a == b,
            _ => true,
        },
    }
}

pub(super) fn text(h: &HeaderMap, name: reqwest::header::HeaderName) -> Option<String> {
    h.get(name)?.to_str().ok().map(str::to_owned)
}

/// `Content-Range: bytes 100-199/1000` as (100, 199, 1000)
pub(super) fn content_range(h: &HeaderMap) -> Option<(u64, u64, u64)> {
    let v = text(h, CONTENT_RANGE)?;
    let (range, total) = v.strip_prefix("bytes ")?.split_once('/')?;
    let (from, to) = range.split_once('-')?;
    Some((from.parse().ok()?, to.parse().ok()?, total.parse().ok()?))
}

async fn len(path: &Path) -> u64 {
    tokio::fs::metadata(path).await.map_or(0, |m| m.len())
}

/// Whether a downloaded file is a whole PMTiles archive of the length the source gave: its
/// header and root directory read.
pub(crate) async fn check_archive(path: &Path, length: u64) -> Result<(), Error> {
    let have = len(path).await;
    if have != length {
        return Err(Error::Incomplete(format!(
            "{} holds {have} of {length} bytes",
            path.display()
        )));
    }
    let file = std::fs::File::open(path)?;
    pmtiles::AsyncPmTilesReader::try_from_source(super::serve::File::of(file)).await?;
    Ok(())
}
