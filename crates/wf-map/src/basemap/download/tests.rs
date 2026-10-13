//! The download against a file server that cuts, hangs, fails, changes and goes away.

use std::collections::VecDeque;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::*;

/// What the source does with one request
#[derive(Debug, Clone, Copy)]
enum Act {
    /// As asked
    Plain,
    /// Sends this many bytes of the body, then ends the connection
    Cut(usize),
    /// Sends this many bytes of the body, then nothing, the connection left open
    Hang(usize),
    Fail,
    /// 200 with the whole file, range or not
    Whole,
    Gone,
}

/// A file server as plain as a CDN: ranges `a-b`, If-Range by ETag; each request takes the
/// next act of its plan, a plain answer once the plan is through.
#[derive(Default)]
struct Source {
    data: Mutex<Vec<u8>>,
    etag: Mutex<String>,
    plan: Mutex<VecDeque<Act>>,
    /// Once the plan is through, every answer is 503 (down for longer than a round waits)
    down_after: std::sync::atomic::AtomicBool,
    /// The ranges asked for, in order
    asked: Mutex<Vec<(u64, u64)>>,
}

impl Source {
    fn new(data: Vec<u8>, etag: &str, plan: &[Act]) -> Arc<Self> {
        let s = Source::default();
        s.change(data, etag);
        s.then(plan);
        Arc::new(s)
    }

    fn change(&self, data: Vec<u8>, etag: &str) {
        *self.data.lock().unwrap() = data;
        *self.etag.lock().unwrap() = etag.into();
    }

    fn then(&self, plan: &[Act]) {
        self.plan.lock().unwrap().extend(plan);
    }

    fn asked(&self) -> Vec<(u64, u64)> {
        self.asked.lock().unwrap().clone()
    }

    async fn answer(&self, mut conn: tokio::net::TcpStream) {
        let mut req = Vec::new();
        let mut buf = [0u8; 1024];
        while !req.windows(4).any(|w| w == b"\r\n\r\n") {
            match conn.read(&mut buf).await {
                Ok(0) | Err(_) => return,
                Ok(n) => req.extend_from_slice(&buf[..n]),
            }
        }
        let req = String::from_utf8_lossy(&req).to_lowercase();
        let header = |name: &str| {
            req.lines()
                .find_map(|l| l.strip_prefix(&format!("{name}: ")))
                .map(|v| v.trim().to_owned())
        };
        let data = self.data.lock().unwrap().clone();
        let etag = self.etag.lock().unwrap().clone();
        let last = data.len() as u64 - 1;
        let (from, to) = header("range")
            .and_then(|r| {
                let (a, b) = r.strip_prefix("bytes=")?.split_once('-')?;
                Some((a.parse().ok()?, b.parse::<u64>().unwrap_or(last).min(last)))
            })
            .unwrap_or((0, last));
        self.asked.lock().unwrap().push((from, to));
        let act = self.plan.lock().unwrap().pop_front().or(self
            .down_after
            .load(std::sync::atomic::Ordering::SeqCst)
            .then_some(Act::Fail));
        let empty = |status: &str| format!("HTTP/1.1 {status}\r\ncontent-length: 0\r\n\r\n");
        let stale = header("if-range").is_some_and(|v| v != etag.to_lowercase());
        let (head, body) = match act {
            Some(Act::Gone) => (empty("404 Not Found"), vec![]),
            Some(Act::Fail) => (empty("503 Service Unavailable"), vec![]),
            _ if stale || matches!(act, Some(Act::Whole)) => (
                format!(
                    "HTTP/1.1 200 OK\r\ncontent-length: {}\r\netag: {etag}\r\n\r\n",
                    data.len()
                ),
                data.clone(),
            ),
            _ => (
                format!(
                    "HTTP/1.1 206 Partial Content\r\ncontent-length: {}\r\n\
                     content-range: bytes {from}-{to}/{}\r\netag: {etag}\r\n\r\n",
                    to - from + 1,
                    data.len()
                ),
                data[from as usize..=to as usize].to_vec(),
            ),
        };
        conn.write_all(head.as_bytes()).await.ok();
        match act {
            Some(Act::Cut(k)) => {
                conn.write_all(&body[..k.min(body.len())]).await.ok();
            }
            Some(Act::Hang(k)) => {
                conn.write_all(&body[..k.min(body.len())]).await.ok();
                conn.flush().await.ok();
                // Open, silent, as the stuck Cloudflare connections were
                tokio::time::sleep(Duration::from_secs(3_600)).await;
            }
            _ => {
                conn.write_all(&body).await.ok();
            }
        }
        conn.shutdown().await.ok();
    }
}

async fn serve(src: Arc<Source>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/20261012.pmtiles", listener.local_addr().unwrap());
    tokio::spawn(async move {
        while let Ok((conn, _)) = listener.accept().await {
            let src = src.clone();
            tokio::spawn(async move { src.answer(conn).await });
        }
    });
    url
}

fn bytes(n: usize, seed: u8) -> Vec<u8> {
    (0..n)
        .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}

/// Pieces of 64 kB, one at a time (the plans go in order), short waits
const ONE: Pace = Pace {
    piece: 65_536,
    parallel: 1,
    window: Duration::from_millis(300),
    min_rate: 1_000,
    backoff: Duration::from_millis(1),
    max_wait: Duration::from_millis(5),
    give_up: Duration::from_millis(400),
};

async fn get(url: &str, part: &Part, pace: Pace) -> Result<u64, Error> {
    whole_with(&reqwest::Client::new(), url, part, "test", pace).await
}

#[tokio::test]
async fn fetches_in_pieces_and_goes_on_where_a_cut_stopped() {
    let data = bytes(200_000, 1);
    let src = Source::new(
        data.clone(),
        "\"a\"",
        &[
            Act::Plain,
            Act::Fail,
            Act::Cut(30_000),
            Act::Fail,
            Act::Fail,
        ],
    );
    let url = serve(src.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let part = Part::of(dir.path(), "basemap.pmtiles");
    assert_eq!(get(&url, &part, ONE).await.unwrap(), 200_000);
    assert_eq!(std::fs::read(&part.file).unwrap(), data);
    // The first byte to learn length and ETag, then piece 0 (503, cut at 30 kB, two more
    // 503s), asked again from where it got, then the other pieces whole
    assert_eq!(
        src.asked(),
        vec![
            (0, 0),
            (0, 65_535),
            (0, 65_535),
            (30_000, 65_535),
            (30_000, 65_535),
            (30_000, 65_535),
            (65_536, 131_071),
            (131_072, 196_607),
            (196_608, 199_999),
        ]
    );
}

#[tokio::test]
async fn asks_again_when_a_connection_hangs() {
    let data = bytes(200_000, 2);
    let src = Source::new(data.clone(), "\"a\"", &[Act::Hang(0), Act::Hang(10_000)]);
    let url = serve(src.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let part = Part::of(dir.path(), "basemap.pmtiles");
    // Several pieces at once: the hung one is asked again, the file comes out whole
    let pace = Pace { parallel: 3, ..ONE };
    assert_eq!(get(&url, &part, pace).await.unwrap(), 200_000);
    assert_eq!(std::fs::read(&part.file).unwrap(), data);
    // The second request hung after 10 kB: its piece was asked again from there
    let (from, to) = src.asked()[1];
    assert!(
        src.asked().contains(&(from + 10_000, to)),
        "{:?}",
        src.asked()
    );
}

/// The length and piece 0 of a download, then the server down until the round gives up
async fn half(src: &Arc<Source>, url: &str, part: &Part) {
    src.then(&[Act::Plain, Act::Plain]);
    src.down_after
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(get(url, part, ONE).await.is_err());
    src.down_after
        .store(false, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(src.asked()[..2], [(0, 0), (0, 65_535)]);
}

#[tokio::test]
async fn fetches_only_the_missing_pieces_after_a_restart() {
    let data = bytes(200_000, 3);
    let src = Source::new(data.clone(), "\"a\"", &[]);
    let url = serve(src.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let part = Part::of(dir.path(), "terrain.pmtiles");
    half(&src, &url, &part).await;
    // A restart later: the end of piece 0 is checked against the source, then only the
    // pieces after it are fetched.
    let before = src.asked().len();
    assert_eq!(get(&url, &part, ONE).await.unwrap(), 200_000);
    assert_eq!(std::fs::read(&part.file).unwrap(), data);
    let later = &src.asked()[before..];
    assert_eq!(
        later,
        [
            (0, 65_535),
            (65_536, 131_071),
            (131_072, 196_607),
            (196_608, 199_999)
        ]
    );
}

#[tokio::test]
async fn starts_over_when_the_check_after_a_restart_differs() {
    let data = bytes(200_000, 9);
    let src = Source::new(data.clone(), "\"a\"", &[]);
    let url = serve(src.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let part = Part::of(dir.path(), "basemap.pmtiles");
    half(&src, &url, &part).await;
    // The part was spoilt meanwhile (a disk fault, a crash mid-write)
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(&part.file)
        .unwrap();
    std::os::unix::fs::FileExt::write_all_at(&file, &[0xff; 100], 65_000).unwrap();
    let before = src.asked().len();
    assert_eq!(get(&url, &part, ONE).await.unwrap(), 200_000);
    assert_eq!(std::fs::read(&part.file).unwrap(), data);
    // The check, then all from the start
    let later = &src.asked()[before..];
    assert_eq!(later[..3], [(0, 65_535), (0, 0), (0, 65_535)]);
}

#[tokio::test]
async fn starts_over_when_the_source_changed() {
    let src = Source::new(bytes(150_000, 4), "\"a\"", &[]);
    let url = serve(src.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let part = Part::of(dir.path(), "basemap.pmtiles");
    half(&src, &url, &part).await;
    // Another file under the same name: If-Range does not match, the server sends it all
    let new = bytes(180_000, 5);
    src.change(new.clone(), "\"b\"");
    assert_eq!(get(&url, &part, ONE).await.unwrap(), 180_000);
    assert_eq!(std::fs::read(&part.file).unwrap(), new);
}

#[tokio::test]
async fn starts_over_when_the_source_answers_with_the_whole_file() {
    let data = bytes(150_000, 6);
    let src = Source::new(data.clone(), "\"a\"", &[]);
    let url = serve(src.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let part = Part::of(dir.path(), "basemap.pmtiles");
    half(&src, &url, &part).await;
    // 200 instead of 206 for a piece: the download begins again
    src.then(&[Act::Whole]);
    assert_eq!(get(&url, &part, ONE).await.unwrap(), 150_000);
    assert_eq!(std::fs::read(&part.file).unwrap(), data);
}

#[tokio::test]
async fn says_when_the_source_is_gone() {
    let src = Source::new(bytes(1_000, 7), "\"a\"", &[Act::Gone]);
    let url = serve(src).await;
    let dir = tempfile::tempdir().unwrap();
    let part = Part::of(dir.path(), "basemap.pmtiles");
    assert!(matches!(get(&url, &part, ONE).await, Err(Error::Gone(_))));
}

#[tokio::test]
async fn a_file_cut_short_is_no_archive() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.pmtiles");
    std::fs::write(&path, bytes(500, 8)).unwrap();
    // Shorter than the source said
    assert!(matches!(
        check_archive(&path, 1_000).await,
        Err(Error::Incomplete(_))
    ));
    // As long, but not a PMTiles archive
    assert!(check_archive(&path, 500).await.is_err());
}

#[test]
fn reads_the_content_range() {
    let mut h = HeaderMap::new();
    h.insert(CONTENT_RANGE, "bytes 100-199/1000".parse().unwrap());
    assert_eq!(content_range(&h), Some((100, 199, 1_000)));
    h.insert(CONTENT_RANGE, "bytes */1000".parse().unwrap());
    assert_eq!(content_range(&h), None);
}

#[test]
fn knows_the_room_on_the_disk() {
    let dir = tempfile::tempdir().unwrap();
    assert!(free_bytes(dir.path()).unwrap() > 0);
}
