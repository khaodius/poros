//! Telling a slow connection from a stalled one. A request may wait a long time behind others
//! on a slow link, so it fails only once its connection has moved no bytes, in either
//! direction, for the stall limit, however long the request itself has been waiting.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;

/// When a connection last moved any bytes. Clones share the record.
#[derive(Clone)]
pub struct Activity {
    origin: Instant,
    /// Milliseconds from `origin` to the latest progress.
    last_progress: Arc<AtomicU64>,
    limit: Duration,
}

impl Activity {
    pub fn new(limit: Duration) -> Self {
        Self {
            origin: Instant::now(),
            last_progress: Arc::new(AtomicU64::new(0)),
            limit,
        }
    }

    pub fn mark_progress(&self) {
        let elapsed = self.origin.elapsed().as_millis() as u64;
        self.last_progress.fetch_max(elapsed, Ordering::Relaxed);
    }

    fn last_progress(&self) -> Instant {
        self.origin + Duration::from_millis(self.last_progress.load(Ordering::Relaxed))
    }

    /// Completes once nothing has moved for the limit, counting from `since` when that is
    /// later than the last progress.
    pub async fn stalled(&self, since: Instant) {
        loop {
            let deadline = self.last_progress().max(since) + self.limit;
            if deadline <= Instant::now() {
                return;
            }
            tokio::time::sleep_until(deadline).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMIT: Duration = Duration::from_secs(30);

    #[tokio::test(start_paused = true)]
    async fn a_connection_that_keeps_moving_never_stalls() {
        let activity = Activity::new(LIMIT);
        let started = Instant::now();
        let moving = activity.clone();
        let ticker = tokio::spawn(async move {
            for _ in 0..10 {
                tokio::time::sleep(Duration::from_secs(20)).await;
                moving.mark_progress();
            }
        });
        tokio::select! {
            () = activity.stalled(started) => panic!("stalled while bytes were moving"),
            finished = ticker => finished.unwrap(),
        }
        assert_eq!(started.elapsed(), Duration::from_secs(200));
    }

    #[tokio::test(start_paused = true)]
    async fn a_connection_stalls_one_limit_after_it_last_moved() {
        let activity = Activity::new(LIMIT);
        let started = Instant::now();
        tokio::time::sleep(Duration::from_secs(10)).await;
        activity.mark_progress();
        activity.stalled(started).await;
        assert_eq!(started.elapsed(), Duration::from_secs(10) + LIMIT);
    }

    #[tokio::test(start_paused = true)]
    async fn a_request_started_after_a_quiet_spell_gets_the_whole_limit() {
        let activity = Activity::new(LIMIT);
        tokio::time::sleep(Duration::from_secs(300)).await;
        let started = Instant::now();
        activity.stalled(started).await;
        assert_eq!(started.elapsed(), LIMIT);
    }
}
