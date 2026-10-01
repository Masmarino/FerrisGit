use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(60);
const MAX_ATTEMPTS_PER_WINDOW: u32 = 10;
/// Cap on tracked IPs so a spray from many addresses cannot grow the map without bound. Clearing on overflow
/// never lets a blocked attempt back in. It only resets windows early under a large distributed attack.
const MAX_TRACKED_IPS: usize = 50_000;

/// A fixed-window rate limiter keyed by source IP: the login default is 10 attempts per 60 seconds, and
/// `with_limits` gives registration and activation their own budget.
pub struct LoginRateLimiter {
    attempts: Mutex<HashMap<IpAddr, (u32, Instant)>>,
    max_attempts: u32,
    window: Duration,
}

impl Default for LoginRateLimiter {
    fn default() -> Self {
        Self::with_limits(MAX_ATTEMPTS_PER_WINDOW, WINDOW)
    }
}

impl LoginRateLimiter {
    pub fn with_limits(max_attempts: u32, window: Duration) -> Self {
        Self {
            attempts: Mutex::new(HashMap::new()),
            max_attempts,
            window,
        }
    }

    pub fn check(&self, ip: IpAddr) -> bool {
        self.check_or_retry_after(ip).is_ok()
    }

    /// Same as `check`, but a refusal says how long until the client's window renews.
    pub fn check_or_retry_after(&self, ip: IpAddr) -> Result<(), Duration> {
        let mut attempts = self.attempts.lock().unwrap();
        if attempts.len() > MAX_TRACKED_IPS {
            attempts.clear();
        }
        let now = Instant::now();
        let entry = attempts.entry(ip).or_insert((0, now));
        if now.duration_since(entry.1) >= self.window {
            *entry = (0, now);
        }
        entry.0 = entry.0.saturating_add(1);
        if entry.0 <= self.max_attempts {
            Ok(())
        } else {
            Err(self.window.saturating_sub(now.duration_since(entry.1)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(last_octet: u8) -> IpAddr {
        IpAddr::from([127, 0, 0, last_octet])
    }

    #[test]
    fn allows_attempts_up_to_the_limit_then_rejects() {
        let limiter = LoginRateLimiter::default();
        let addr = ip(1);

        for _ in 0..MAX_ATTEMPTS_PER_WINDOW {
            assert!(
                limiter.check(addr),
                "attempts within the limit must be allowed"
            );
        }
        assert!(
            !limiter.check(addr),
            "the attempt exceeding the limit must be rejected"
        );
    }

    #[test]
    fn default_keeps_the_login_budget_of_ten_attempts_per_minute() {
        let limiter = LoginRateLimiter::default();

        assert_eq!(limiter.max_attempts, 10);
        assert_eq!(limiter.window, Duration::from_secs(60));
    }

    #[test]
    fn with_limits_applies_its_own_budget() {
        let limiter = LoginRateLimiter::with_limits(3, Duration::from_secs(300));
        let addr = ip(1);

        for _ in 0..3 {
            assert!(limiter.check(addr));
        }
        assert!(
            !limiter.check(addr),
            "the 4th attempt exceeds a budget of 3"
        );
        assert!(limiter.check(ip(2)), "another IP keeps its own budget");
    }

    #[test]
    fn with_limits_uses_its_own_window_not_the_default_one() {
        let long = LoginRateLimiter::with_limits(1, Duration::from_secs(300));
        assert!(long.check(ip(1)));
        long.attempts.lock().unwrap().get_mut(&ip(1)).unwrap().1 -= Duration::from_secs(61);
        assert!(!long.check(ip(1)), "still inside the 300 s window");

        let short = LoginRateLimiter::with_limits(1, Duration::from_secs(1));
        assert!(short.check(ip(1)));
        short.attempts.lock().unwrap().get_mut(&ip(1)).unwrap().1 -= Duration::from_secs(2);
        assert!(
            short.check(ip(1)),
            "the window elapsed, the budget is renewed"
        );
    }

    #[test]
    fn a_refusal_says_how_long_until_the_window_renews() {
        let limiter = LoginRateLimiter::with_limits(1, Duration::from_secs(60));
        assert_eq!(limiter.check_or_retry_after(ip(1)), Ok(()));
        limiter.attempts.lock().unwrap().get_mut(&ip(1)).unwrap().1 -= Duration::from_secs(20);

        let retry_after = limiter.check_or_retry_after(ip(1)).unwrap_err();

        assert!(retry_after <= Duration::from_secs(40));
        assert!(retry_after > Duration::from_secs(39));
    }

    #[test]
    fn tracks_each_ip_independently() {
        let limiter = LoginRateLimiter::default();
        for _ in 0..MAX_ATTEMPTS_PER_WINDOW {
            limiter.check(ip(1));
        }

        assert!(!limiter.check(ip(1)), "the exhausted IP must stay blocked");
        assert!(
            limiter.check(ip(2)),
            "a different IP must have its own, unaffected budget"
        );
    }
}
