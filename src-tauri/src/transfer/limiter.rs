use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tokio::sync::Notify;

/// Smallest burst allowance, so one request-sized chunk never waits on an empty bucket.
const MIN_BURST_BYTES: f64 = 256.0 * 1024.0;
const BURST_SECONDS: f64 = 0.25;

/// A token bucket shared by every worker moving data in one direction. Reservations may
/// overdraw the bucket; the caller then sleeps off the debt, which keeps the combined rate
/// of all workers at the limit.
pub struct RateLimiter {
    bytes_per_second: AtomicU64,
    bucket: Mutex<Bucket>,
    /// Wakes the callers sleeping off a debt when the rate changes.
    rate_changed: Notify,
}

struct Bucket {
    available: f64,
    refilled_at: Instant,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            bytes_per_second: AtomicU64::new(0),
            bucket: Mutex::new(Bucket {
                available: 0.0,
                refilled_at: Instant::now(),
            }),
            rate_changed: Notify::new(),
        }
    }

    /// Zero removes the limit, and the debt with it. Callers already waiting work out their
    /// wait again at the new rate.
    pub fn set_rate(&self, bytes_per_second: u64) {
        let previous = self
            .bytes_per_second
            .swap(bytes_per_second, Ordering::Relaxed);
        if previous == bytes_per_second {
            return;
        }
        if bytes_per_second == 0 {
            let mut bucket = self.bucket.lock().unwrap();
            bucket.available = bucket.available.max(0.0);
        }
        self.rate_changed.notify_waiters();
    }

    pub async fn acquire(&self, bytes: u64) {
        let mut bytes = bytes;
        loop {
            // Taken before reserving, so a change of rate just after is not missed.
            let rate_changed = self.rate_changed.notified();
            let Some(wait) = self.reserve(bytes, Instant::now()) else {
                return;
            };
            tokio::select! {
                () = tokio::time::sleep(wait) => return,
                // The bytes are already in the debt; only the wait is worked out again.
                () = rate_changed => bytes = 0,
            }
        }
    }

    fn reserve(&self, bytes: u64, now: Instant) -> Option<Duration> {
        let rate = self.bytes_per_second.load(Ordering::Relaxed);
        if rate == 0 {
            return None;
        }
        let rate = rate as f64;
        let mut bucket = self.bucket.lock().unwrap();
        let elapsed = now
            .saturating_duration_since(bucket.refilled_at)
            .as_secs_f64();
        let capacity = (rate * BURST_SECONDS).max(MIN_BURST_BYTES);
        bucket.available = (bucket.available + elapsed * rate).min(capacity);
        bucket.refilled_at = now;
        bucket.available -= bytes as f64;
        (bucket.available < 0.0).then(|| Duration::from_secs_f64(-bucket.available / rate))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlimited_never_waits() {
        let limiter = RateLimiter::new();
        assert_eq!(limiter.reserve(u64::MAX / 2, Instant::now()), None);
    }

    #[test]
    fn concurrent_reservations_share_the_rate() {
        let limiter = RateLimiter::new();
        limiter.set_rate(1_000_000);
        let start = Instant::now() + Duration::from_secs(10);
        // A full bucket covers the first burst.
        assert_eq!(limiter.reserve(262_144, start), None);
        // Two workers asking at once queue behind each other.
        let first = limiter.reserve(500_000, start).unwrap();
        let second = limiter.reserve(500_000, start).unwrap();
        assert!((first.as_secs_f64() - 0.5).abs() < 0.01, "{first:?}");
        assert!((second.as_secs_f64() - 1.0).abs() < 0.01, "{second:?}");
        // Once the debt is slept off, the bucket refills at the configured rate.
        let later = start + Duration::from_secs(2);
        assert_eq!(limiter.reserve(0, later), None);
    }

    #[tokio::test(start_paused = true)]
    async fn raising_or_lifting_the_limit_ends_waits_worked_out_at_the_old_rate() {
        for new_rate in [0, 1_000_000] {
            let limiter = std::sync::Arc::new(RateLimiter::new());
            limiter.set_rate(1_000);
            // At the old rate these wait 100 and 200 seconds.
            let waiting: Vec<_> = (0..2)
                .map(|_| {
                    let limiter = limiter.clone();
                    tokio::spawn(async move { limiter.acquire(100_000).await })
                })
                .collect();
            tokio::time::sleep(Duration::from_millis(10)).await;
            assert!(waiting.iter().all(|task| !task.is_finished()));
            limiter.set_rate(new_rate);
            tokio::time::sleep(Duration::from_secs(1)).await;
            for task in waiting {
                assert!(task.is_finished(), "still waiting at {new_rate} B/s");
            }
        }
    }
}
