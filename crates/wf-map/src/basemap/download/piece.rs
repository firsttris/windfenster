//! One piece of a download: its range asked on a connection of its own, its bytes written at
//! their place, asked again from where it got after a stall or a moment's failure.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use reqwest::StatusCode;
use reqwest::header::{ETAG, IF_RANGE, LAST_MODIFIED};

use super::{
    Pace, Piece, State, WRITE, content_range, passing_status, request, same, text, validator,
};
use crate::basemap::fetch::{self, Error};

/// One piece, asked again on a new connection from where it got after a stall or a moment's
/// failure, without end (the round gives up when no piece gets done for long).
pub(super) async fn piece(
    http: &reqwest::Client,
    state: &State,
    n: u64,
    file: &Arc<std::fs::File>,
    got: &AtomicU64,
    pace: Pace,
) -> Result<Piece, Error> {
    let (from, to) = state.range(n);
    let mut at = from;
    let mut wait = pace.backoff;
    loop {
        let before = at;
        let failure = match fetch_range(http, state, at, to, file, got, pace).await? {
            Fetched::To(end) => {
                at = end;
                if at > to {
                    return Ok(Piece::Done);
                }
                Error::Incomplete(format!("the connection ended at {at} of {to}"))
            }
            Fetched::Changed => return Ok(Piece::Restart),
            Fetched::Failed(e, end) => {
                at = end;
                e
            }
        };
        if at > before {
            wait = pace.backoff;
        }
        let pause = fetch::spread(wait, at as usize);
        tracing::debug!("piece {n}: {failure}; again from {at} in {pause:?}");
        tokio::time::sleep(pause).await;
        wait = (wait * 2).min(pace.max_wait);
    }
}

enum Fetched {
    /// Read up to (not including) this byte, the connection over
    To(u64),
    /// The source is another file now
    Changed,
    /// Failed for a moment after reaching this byte
    Failed(Error, u64),
}

/// One request for bytes `at..=to`, written at their place.
async fn fetch_range(
    http: &reqwest::Client,
    state: &State,
    at: u64,
    to: u64,
    file: &Arc<std::fs::File>,
    got: &AtomicU64,
    pace: Pace,
) -> Result<Fetched, Error> {
    let mut req = request(http, &state.url, &format!("bytes={at}-{to}"));
    if let Some(v) = validator(state) {
        req = req.header(IF_RANGE, v);
    }
    let mut resp = match tokio::time::timeout(pace.window, req.send()).await {
        Err(_) => return Ok(Fetched::Failed(stalled(pace), at)),
        Ok(Err(e)) => return Ok(Fetched::Failed(e.into(), at)),
        Ok(Ok(r)) => r,
    };
    let status = resp.status();
    match status {
        StatusCode::PARTIAL_CONTENT => {
            let fresh = State {
                etag: text(resp.headers(), ETAG),
                last_modified: text(resp.headers(), LAST_MODIFIED),
                ..State::default()
            };
            let asked = Some((at, to, state.total));
            if content_range(resp.headers()) != asked || !same(state, &fresh) {
                return Ok(Fetched::Changed);
            }
        }
        // The whole file instead of the range: If-Range found another file
        StatusCode::OK => return Ok(Fetched::Changed),
        StatusCode::NOT_FOUND | StatusCode::GONE => return Err(Error::Gone(state.url.clone())),
        s if passing_status(s) => {
            let e = Error::Incomplete(format!("{} answered {s}", state.url));
            return Ok(Fetched::Failed(e, at));
        }
        s => return Err(Error::Incomplete(format!("{} answered {s}", state.url))),
    }
    let mut pos = at;
    let mut buf: Vec<u8> = Vec::with_capacity(WRITE);
    // Bytes in the current window: too few, and the request is taken as hung
    let (mut since, mut window) = (Instant::now(), 0u64);
    let floor = (pace.min_rate as f64 * pace.window.as_secs_f64()) as u64;
    let ended = loop {
        let left = pace.window.saturating_sub(since.elapsed());
        match tokio::time::timeout(left, resp.chunk()).await {
            Err(_) if window < floor => break Some(stalled(pace)),
            Err(_) => (since, window) = (Instant::now(), 0),
            Ok(Err(e)) => break Some(e.into()),
            Ok(Ok(None)) => break None,
            Ok(Ok(Some(bytes))) => {
                if pos + (buf.len() + bytes.len()) as u64 > to + 1 {
                    return Ok(Fetched::Changed);
                }
                window += bytes.len() as u64;
                if since.elapsed() >= pace.window {
                    if window < floor {
                        buf.extend_from_slice(&bytes);
                        break Some(stalled(pace));
                    }
                    (since, window) = (Instant::now(), 0);
                }
                buf.extend_from_slice(&bytes);
                if buf.len() >= WRITE {
                    pos = write_at(file, std::mem::take(&mut buf), pos, got).await?;
                }
            }
        }
    };
    pos = write_at(file, buf, pos, got).await?;
    Ok(match ended {
        None => Fetched::To(pos),
        Some(e) => Fetched::Failed(e, pos),
    })
}

fn stalled(pace: Pace) -> Error {
    Error::Incomplete(format!(
        "less than {} kB/s for {:?}",
        pace.min_rate / 1_000,
        pace.window
    ))
}

/// Writes `bytes` at `pos`, counts them in `got`, and returns the position after them.
async fn write_at(
    file: &Arc<std::fs::File>,
    bytes: Vec<u8>,
    pos: u64,
    got: &AtomicU64,
) -> Result<u64, Error> {
    if bytes.is_empty() {
        return Ok(pos);
    }
    let file = file.clone();
    let n = bytes.len() as u64;
    tokio::task::spawn_blocking(move || {
        use std::os::unix::fs::FileExt;
        file.write_all_at(&bytes, pos)
    })
    .await??;
    got.fetch_add(n, Ordering::Relaxed);
    Ok(pos + n)
}
