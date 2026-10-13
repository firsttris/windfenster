//! How far a long download is, and the room on the disk for it.

use std::path::Path;
use std::time::Instant;

use crate::basemap::fetch::Error;

/// Bytes free for this user on the file system holding `dir`.
pub fn free_bytes(dir: &Path) -> Result<u64, Error> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(dir.as_os_str().as_bytes()).map_err(std::io::Error::other)?;
    // SAFETY: statvfs only fills the struct it is given; the path is a valid C string.
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    // u64 already on 64-bit Linux, narrower elsewhere
    #[allow(clippy::useless_conversion)]
    Ok(u64::from(s.f_bavail) * u64::from(s.f_frsize))
}

pub fn gb(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1e9)
}

/// How far a long job is, said every few minutes.
pub struct Progress {
    what: String,
    start: u64,
    began: Instant,
    pub total: Option<u64>,
}

impl Progress {
    pub fn new(what: &str, start: u64) -> Self {
        Progress {
            what: what.to_owned(),
            start,
            began: Instant::now(),
            total: None,
        }
    }

    /// Says where `at` is.
    pub fn tick(&mut self, at: u64) {
        let secs = self.began.elapsed().as_secs_f64().max(1.0);
        let rate = (at.saturating_sub(self.start)) as f64 / secs;
        let left = match self.total {
            Some(t) if rate > 0.0 && t > at => {
                format!(
                    ", about {} min to go",
                    ((t - at) as f64 / rate / 60.0).ceil()
                )
            }
            _ => String::new(),
        };
        let of = self
            .total
            .map(|t| format!(" of {}", gb(t)))
            .unwrap_or_default();
        tracing::info!(
            "{}: {}{of}, {:.1} MB/s{left}",
            self.what,
            gb(at),
            rate / 1e6
        );
    }
}
