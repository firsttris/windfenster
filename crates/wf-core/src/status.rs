//! What the background work is doing, for the admin page: each task's last run (when, how long,
//! what came of it), each outside source's last success and its errors of the last day, the
//! last warnings and errors of the log, and requests to run a task now.
//!
//! One board per process, like the log: the loops report to it wherever they are, and the API
//! reads it. It lives in memory; after a restart it fills again within a round.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, LazyLock, Mutex};

use serde::Serialize;
use tokio::sync::Notify;
use ts_rs::TS;

/// Log lines kept
const LOG_LINES: usize = 500;
/// Errors of a source counted back this far
const ERRORS_S: i64 = 86_400;

/// How a task's last run went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Outcome {
    /// Not run since the start
    Waiting,
    Running,
    Ok,
    /// Done, but part of it failed (a few stations, one model)
    Partly,
    Failed,
}

/// A task as the admin page shows it. `error` is the technical detail as the log has it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TaskStatus {
    pub id: String,
    /// How often it runs, seconds
    #[ts(type = "number")]
    pub every_s: i64,
    pub outcome: Outcome,
    #[ts(type = "number | null")]
    pub started_at: Option<i64>,
    #[ts(type = "number | null")]
    pub took_ms: Option<i64>,
    /// What it handled: METARs stored, places refreshed, hours written ...
    #[ts(type = "number | null")]
    pub count: Option<i64>,
    pub error: Option<String>,
    /// Restarts after a panic since the process started
    pub panics: u32,
}

/// An outside source: when it last answered, and how often it failed in the last day.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SourceStatus {
    pub id: String,
    #[ts(type = "number | null")]
    pub ok_at: Option<i64>,
    #[ts(type = "number | null")]
    pub failed_at: Option<i64>,
    pub errors_day: u32,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LogLine {
    #[ts(type = "number")]
    pub at: i64,
    /// "WARN" or "ERROR"
    pub level: String,
    pub target: String,
    pub message: String,
}

#[derive(Default)]
struct Source {
    ok_at: Option<i64>,
    failures: VecDeque<i64>,
    last_error: Option<String>,
}

#[derive(Default)]
struct Inner {
    tasks: BTreeMap<&'static str, TaskStatus>,
    sources: BTreeMap<&'static str, Source>,
    log: VecDeque<LogLine>,
    /// Tasks asked to run now
    asked: Vec<String>,
    /// Requests that came without the client-address header the server was told to read
    missing_ip_header: u64,
    /// Restarts after a panic, per loop
    panics: BTreeMap<String, u32>,
    /// The newest release on GitHub, as last looked up
    release: Option<Release>,
}

/// A release of Windfenster.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Release {
    /// e.g. "0.1.17", without the "v"
    pub version: String,
    pub url: String,
}

pub struct Board {
    inner: Mutex<Inner>,
    /// Woken when a task is asked to run now
    wake: Arc<Notify>,
    started_at: i64,
}

static BOARD: LazyLock<Board> = LazyLock::new(|| Board {
    inner: Mutex::default(),
    wake: Arc::new(Notify::new()),
    started_at: crate::time::now(),
});

/// The process's board.
pub fn board() -> &'static Board {
    &BOARD
}

/// A run in progress; say how it ended with [`Run::ok`], [`Run::partly`] or [`Run::failed`].
pub struct Run {
    task: &'static str,
    started: std::time::Instant,
}

impl Board {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn started_at(&self) -> i64 {
        self.started_at
    }

    /// Names a task and how often it runs, so the page shows it before its first run.
    pub fn register(&self, task: &'static str, every_s: i64) {
        self.lock().tasks.entry(task).or_insert_with(|| TaskStatus {
            id: task.to_owned(),
            every_s,
            outcome: Outcome::Waiting,
            started_at: None,
            took_ms: None,
            count: None,
            error: None,
            panics: 0,
        });
    }

    pub fn start(&self, task: &'static str) -> Run {
        let now = crate::time::now();
        let mut inner = self.lock();
        let t = inner.tasks.entry(task).or_insert_with(|| TaskStatus {
            id: task.to_owned(),
            every_s: 0,
            outcome: Outcome::Waiting,
            started_at: None,
            took_ms: None,
            count: None,
            error: None,
            panics: 0,
        });
        t.outcome = Outcome::Running;
        t.started_at = Some(now);
        Run {
            task,
            started: std::time::Instant::now(),
        }
    }

    fn end(&self, run: &Run, outcome: Outcome, count: Option<i64>, error: Option<String>) {
        let took = i64::try_from(run.started.elapsed().as_millis()).unwrap_or(i64::MAX);
        if let Some(t) = self.lock().tasks.get_mut(run.task) {
            t.outcome = outcome;
            t.took_ms = Some(took);
            t.count = count;
            t.error = error;
        }
    }

    /// A loop panicked and is started again; a task of the same name shows it.
    pub fn panicked(&self, name: &str) {
        let mut inner = self.lock();
        *inner.panics.entry(name.to_owned()).or_default() += 1;
        if let Some(t) = inner.tasks.get_mut(name) {
            t.panics += 1;
            t.outcome = Outcome::Failed;
            t.error = Some("panicked".into());
        }
    }

    /// Restarts after a panic since the start, per loop
    pub fn panics(&self) -> Vec<(String, u32)> {
        self.lock()
            .panics
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect()
    }

    pub fn tasks(&self) -> Vec<TaskStatus> {
        self.lock().tasks.values().cloned().collect()
    }

    /// The source answered.
    pub fn source_ok(&self, source: &'static str) {
        self.lock().sources.entry(source).or_default().ok_at = Some(crate::time::now());
    }

    /// The source failed; the detail is kept as the log has it.
    pub fn source_failed(&self, source: &'static str, error: impl std::fmt::Display) {
        let now = crate::time::now();
        let mut inner = self.lock();
        let s = inner.sources.entry(source).or_default();
        s.failures.push_back(now);
        while s.failures.front().is_some_and(|t| now - t > ERRORS_S) {
            s.failures.pop_front();
        }
        s.last_error = Some(error.to_string());
    }

    pub fn sources(&self) -> Vec<SourceStatus> {
        let now = crate::time::now();
        self.lock()
            .sources
            .iter()
            .map(|(id, s)| SourceStatus {
                id: (*id).to_owned(),
                ok_at: s.ok_at,
                failed_at: s.failures.back().copied(),
                errors_day: s.failures.iter().filter(|t| now - **t <= ERRORS_S).count() as u32,
                last_error: s.last_error.clone(),
            })
            .collect()
    }

    /// A warning or error from the log.
    pub fn log(&self, level: &str, target: &str, message: String) {
        let mut inner = self.lock();
        if inner.log.len() == LOG_LINES {
            inner.log.pop_front();
        }
        inner.log.push_back(LogLine {
            at: crate::time::now(),
            level: level.to_owned(),
            target: target.to_owned(),
            message,
        });
    }

    /// The log's last lines, newest first.
    pub fn log_lines(&self) -> Vec<LogLine> {
        self.lock().log.iter().rev().cloned().collect()
    }

    /// Asks a task to run now; its loop wakes and takes the request.
    pub fn ask(&self, task: &str) -> bool {
        let known = {
            let mut inner = self.lock();
            let known = inner.tasks.contains_key(task);
            if known && !inner.asked.iter().any(|t| t == task) {
                inner.asked.push(task.to_owned());
            }
            known
        };
        if known {
            self.wake.notify_waiters();
        }
        known
    }

    /// Whether a task was asked to run now; takes the request.
    pub fn take(&self, task: &str) -> bool {
        let mut inner = self.lock();
        let before = inner.asked.len();
        inner.asked.retain(|t| t != task);
        inner.asked.len() != before
    }

    /// Resolves when any task is asked to run now.
    pub async fn asked(&self) {
        self.wake.notified().await;
    }

    pub fn set_release(&self, release: Release) {
        self.lock().release = Some(release);
    }

    pub fn release(&self) -> Option<Release> {
        self.lock().release.clone()
    }

    pub fn missing_ip_header(&self) {
        self.lock().missing_ip_header += 1;
    }

    /// Requests without the client-address header since the start
    pub fn missing_ip_headers(&self) -> u64 {
        self.lock().missing_ip_header
    }
}

impl Run {
    pub fn ok(self, count: Option<i64>) {
        board().end(&self, Outcome::Ok, count, None);
    }

    pub fn partly(self, count: Option<i64>, error: impl std::fmt::Display) {
        board().end(&self, Outcome::Partly, count, Some(error.to_string()));
    }

    pub fn failed(self, error: impl std::fmt::Display) {
        board().end(&self, Outcome::Failed, None, Some(error.to_string()));
    }
}

/// Whether version `latest` is newer than `running` (x.y.z, the numbers compared).
pub fn newer(latest: &str, running: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> {
        v.split('.')
            .map(|p| p.trim().parse().unwrap_or(0))
            .collect()
    };
    parts(latest) > parts(running)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions_as_numbers() {
        assert!(newer("0.1.17", "0.1.16"));
        assert!(newer("0.10.0", "0.9.9"));
        assert!(!newer("0.1.16", "0.1.16"));
        assert!(!newer("0.1.9", "0.1.16"));
    }

    // One board per process: the tests use task and source names of their own.
    #[test]
    fn keeps_what_the_tasks_report() {
        let b = board();
        b.register("test-a", 600);
        assert_eq!(
            b.tasks().iter().find(|t| t.id == "test-a").unwrap().outcome,
            Outcome::Waiting
        );
        b.start("test-a").ok(Some(42));
        let t = b.tasks().into_iter().find(|t| t.id == "test-a").unwrap();
        assert_eq!(
            (t.outcome, t.count, t.every_s),
            (Outcome::Ok, Some(42), 600)
        );
        b.start("test-a").failed("HTTP 429");
        b.panicked("test-a");
        let t = b.tasks().into_iter().find(|t| t.id == "test-a").unwrap();
        assert_eq!((t.outcome, t.panics), (Outcome::Failed, 1));
        assert_eq!(t.error.as_deref(), Some("panicked"));

        b.source_ok("test-src");
        b.source_failed("test-src", "timeout");
        b.source_failed("test-src", "HTTP 503");
        let s = b
            .sources()
            .into_iter()
            .find(|s| s.id == "test-src")
            .unwrap();
        assert_eq!(
            (s.errors_day, s.last_error.as_deref()),
            (2, Some("HTTP 503"))
        );
        assert!(s.ok_at.is_some());
    }

    #[test]
    fn asks_known_tasks_once() {
        let b = board();
        assert!(!b.ask("test-unknown"));
        b.register("test-b", 60);
        assert!(b.ask("test-b"));
        assert!(b.ask("test-b"));
        assert!(b.take("test-b"));
        assert!(!b.take("test-b"));
    }

    #[test]
    fn keeps_the_last_log_lines() {
        let b = board();
        for i in 0..(LOG_LINES + 3) {
            b.log("WARN", "test", format!("line {i}"));
        }
        let lines = b.log_lines();
        assert_eq!(lines.len(), LOG_LINES);
        assert_eq!(lines[0].message, format!("line {}", LOG_LINES + 2));
    }
}
