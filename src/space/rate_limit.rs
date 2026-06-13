use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;

/// Fixed-window rate limit keyed by client IP, guarding `/auth/*` against brute
/// force: cap attempts per window and let the lock-out expire on its own.
pub const UNLOCK_MAX_ATTEMPTS: u32 = 8;
pub const UNLOCK_WINDOW: Duration = Duration::from_secs(60);

struct Bucket {
    window_start: Instant,
    attempts: u32,
}

#[derive(Clone, Default)]
pub struct RateLimiter {
    inner: Arc<DashMap<IpAddr, Bucket>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an attempt and return `Some(retry_after)` if the caller is
    /// over the budget, or `None` if they're allowed to proceed.
    pub fn check(&self, ip: IpAddr) -> Option<Duration> {
        let now = Instant::now();
        let mut bucket = self.inner.entry(ip).or_insert(Bucket {
            window_start: now,
            attempts: 0,
        });
        if now.duration_since(bucket.window_start) >= UNLOCK_WINDOW {
            bucket.window_start = now;
            bucket.attempts = 0;
        }
        bucket.attempts += 1;
        if bucket.attempts > UNLOCK_MAX_ATTEMPTS {
            let elapsed = now.duration_since(bucket.window_start);
            Some(UNLOCK_WINDOW.saturating_sub(elapsed))
        } else {
            None
        }
    }

    /// Forget the bucket for a client after a successful unlock so an earlier
    /// typo doesn't burn a slot on the next legitimate session.
    pub fn clear(&self, ip: IpAddr) {
        self.inner.remove(&ip);
    }

    /// Drop windows that have rolled over. Cheap enough for a periodic task.
    pub fn sweep(&self) {
        let now = Instant::now();
        self.inner
            .retain(|_, bucket| now.duration_since(bucket.window_start) < UNLOCK_WINDOW);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn ip(n: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, n))
    }

    #[test]
    fn first_attempts_are_allowed() {
        let limiter = RateLimiter::new();
        for _ in 0..UNLOCK_MAX_ATTEMPTS {
            assert!(limiter.check(ip(1)).is_none());
        }
    }

    #[test]
    fn extra_attempt_in_window_is_rejected() {
        let limiter = RateLimiter::new();
        for _ in 0..UNLOCK_MAX_ATTEMPTS {
            assert!(limiter.check(ip(1)).is_none());
        }
        assert!(limiter.check(ip(1)).is_some());
    }

    #[test]
    fn different_clients_dont_share_a_bucket() {
        let limiter = RateLimiter::new();
        for _ in 0..UNLOCK_MAX_ATTEMPTS {
            assert!(limiter.check(ip(1)).is_none());
        }
        assert!(limiter.check(ip(2)).is_none());
    }

    #[test]
    fn clear_resets_a_client() {
        let limiter = RateLimiter::new();
        for _ in 0..UNLOCK_MAX_ATTEMPTS {
            let _ = limiter.check(ip(1));
        }
        limiter.clear(ip(1));
        assert!(limiter.check(ip(1)).is_none());
    }
}
