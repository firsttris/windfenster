//! The background loops (map, basemap, alarms, scheduler) run for the life of the process. A
//! panic in one would end it with one log line while the server goes on answering with ever
//! older data; so each runs under a supervisor that starts it again after a pause.

use std::future::Future;
use std::time::Duration;

/// How long a loop that panicked waits before it starts again: long enough not to spin on a
/// fault that repeats at once, short enough to catch up within a round.
const RESTART_AFTER: Duration = Duration::from_secs(60);

/// Runs the loop `make` gives, and gives a new one when it panics; returns when a loop ends
/// on its own (they do not).
pub fn supervise<F, Fut>(name: &'static str, make: F) -> tokio::task::JoinHandle<()>
where
    F: Fn() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    supervise_with(name, RESTART_AFTER, make)
}

fn supervise_with<F, Fut>(
    name: &'static str,
    pause: Duration,
    make: F,
) -> tokio::task::JoinHandle<()>
where
    F: Fn() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    tokio::spawn(async move {
        loop {
            match tokio::spawn(make()).await {
                Ok(()) => break,
                Err(e) if e.is_panic() => {
                    tracing::error!("{name}: the loop panicked; starting it again in {pause:?}");
                    wf_core::status::board().panicked(name);
                    tokio::time::sleep(pause).await;
                }
                // Cancelled: the runtime is shutting down
                Err(_) => break,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[tokio::test]
    async fn starts_a_loop_again_after_a_panic() {
        let runs = Arc::new(AtomicUsize::new(0));
        let counted = runs.clone();
        // Panics twice, then ends on its own
        let handle = supervise_with("test", Duration::from_millis(1), move || {
            let runs = counted.clone();
            async move {
                if runs.fetch_add(1, Ordering::SeqCst) < 2 {
                    panic!("a fault");
                }
            }
        });
        handle.await.unwrap();
        assert_eq!(runs.load(Ordering::SeqCst), 3);
    }
}
