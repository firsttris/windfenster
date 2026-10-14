//! The basemap's long job under way (a cut, a download), for the admin page: what, how far, since
//! when. One at a time per kind; the log still says it every five minutes.

use std::sync::Mutex;

use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Work {
    /// "basemap", "terrain" or "spot"
    pub what: String,
    /// "tiles" or "bytes"
    pub unit: String,
    #[ts(type = "number")]
    pub done: u64,
    #[ts(type = "number")]
    pub total: u64,
    #[ts(type = "number")]
    pub started_at: i64,
    /// Units a second so far
    pub rate: f64,
}

static NOW: Mutex<Vec<Work>> = Mutex::new(Vec::new());

fn lock() -> std::sync::MutexGuard<'static, Vec<Work>> {
    NOW.lock().unwrap_or_else(|e| e.into_inner())
}

/// A job begins; it stays shown until [`end`].
pub fn begin(what: &str, unit: &str, done: u64, total: u64) {
    let mut now = lock();
    now.retain(|w| w.what != what);
    now.push(Work {
        what: what.to_owned(),
        unit: unit.to_owned(),
        done,
        total,
        started_at: wf_core::time::now(),
        rate: 0.0,
    });
}

/// The job got to `done`.
pub fn advance(what: &str, done: u64) {
    if let Some(w) = lock().iter_mut().find(|w| w.what == what) {
        let secs = (wf_core::time::now() - w.started_at).max(1) as f64;
        w.rate = done as f64 / secs;
        w.done = done;
    }
}

pub fn end(what: &str) {
    lock().retain(|w| w.what != what);
}

/// The jobs under way.
pub fn current() -> Vec<Work> {
    lock().clone()
}

/// Ends the job when dropped, however the work ends (done, failed, cancelled).
pub struct Guard(pub String);

impl Drop for Guard {
    fn drop(&mut self) {
        end(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_a_job_while_it_runs() {
        {
            let _g = Guard("test-work".into());
            begin("test-work", "tiles", 0, 100);
            advance("test-work", 40);
            let w = current()
                .into_iter()
                .find(|w| w.what == "test-work")
                .unwrap();
            assert_eq!((w.done, w.total, w.unit.as_str()), (40, 100, "tiles"));
        }
        assert!(current().iter().all(|w| w.what != "test-work"));
    }
}
