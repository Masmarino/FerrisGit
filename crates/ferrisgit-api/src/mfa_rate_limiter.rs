use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use uuid::Uuid;

const MAX_ATTEMPTS: u32 = 10;
const WINDOW: Duration = Duration::from_secs(5 * 60);
/// Cap on tracked users (see `LoginRateLimiter`). On overflow, expired windows are dropped first.
const MAX_TRACKED: usize = 50_000;

/// Keyed by user, not IP: a TOTP code has only a million values, so the budget follows the account, whoever holds
/// its `mfa-pending` token. Every call counts, a successful one included, and nothing is refunded.
pub struct MfaRateLimiter {
    window: Duration,
    attempts: Mutex<HashMap<Uuid, (u32, Instant)>>,
}

impl Default for MfaRateLimiter {
    fn default() -> Self {
        Self::with_window(WINDOW)
    }
}

impl MfaRateLimiter {
    pub fn with_window(window: Duration) -> Self {
        Self {
            window,
            attempts: Mutex::new(HashMap::new()),
        }
    }

    /// Only for tests, which need to check what a refused call consumed without racing a wall-clock window.
    pub fn reset(&self, user_id: Uuid) {
        self.attempts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&user_id);
    }

    pub fn check(&self, user_id: Uuid) -> bool {
        let mut attempts = self
            .attempts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = Instant::now();
        if attempts.len() >= MAX_TRACKED && !attempts.contains_key(&user_id) {
            attempts.retain(|_, (_, started)| now.duration_since(*started) < self.window);
            if attempts.len() >= MAX_TRACKED {
                attempts.clear();
            }
        }
        let entry = attempts.entry(user_id).or_insert((0, now));
        if now.duration_since(entry.1) >= self.window {
            *entry = (0, now);
        }
        entry.0 = entry.0.saturating_add(1);
        entry.0 <= MAX_ATTEMPTS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_attempts_up_to_the_limit_then_rejects() {
        let limiter = MfaRateLimiter::default();
        let user = Uuid::new_v4();

        for attempt in 1..=MAX_ATTEMPTS {
            assert!(
                limiter.check(user),
                "attempt {attempt} is within the budget"
            );
        }
        assert!(
            !limiter.check(user),
            "the attempt exceeding the budget must be rejected"
        );
        assert!(!limiter.check(user), "and it stays rejected");
    }

    #[test]
    fn tracks_each_user_independently() {
        let limiter = MfaRateLimiter::default();
        let (first, second) = (Uuid::new_v4(), Uuid::new_v4());
        for _ in 0..MAX_ATTEMPTS {
            limiter.check(first);
        }

        assert!(
            !limiter.check(first),
            "the exhausted user must stay blocked"
        );
        assert!(
            limiter.check(second),
            "another user has their own, untouched budget"
        );
    }

    #[test]
    fn reset_gives_one_user_a_fresh_budget_and_leaves_the_others_alone() {
        let limiter = MfaRateLimiter::default();
        let (first, second) = (Uuid::new_v4(), Uuid::new_v4());
        for _ in 0..=MAX_ATTEMPTS {
            limiter.check(first);
            limiter.check(second);
        }
        assert!(!limiter.check(first) && !limiter.check(second));

        limiter.reset(first);

        assert!(limiter.check(first), "the reset user starts over");
        assert!(!limiter.check(second), "the other one is still blocked");
    }

    #[test]
    fn the_budget_comes_back_once_the_window_has_elapsed() {
        let limiter = MfaRateLimiter::with_window(Duration::from_millis(60));
        let user = Uuid::new_v4();
        for _ in 0..MAX_ATTEMPTS {
            limiter.check(user);
        }
        assert!(!limiter.check(user));

        std::thread::sleep(Duration::from_millis(120));

        assert!(
            limiter.check(user),
            "a fresh window starts with a fresh budget"
        );
    }

    #[test]
    fn the_default_window_is_five_minutes_and_the_budget_ten() {
        assert_eq!(WINDOW, Duration::from_secs(300));
        assert_eq!(MAX_ATTEMPTS, 10);
    }

    #[test]
    fn the_tracked_users_stay_bounded() {
        let limiter = MfaRateLimiter::default();
        for _ in 0..MAX_TRACKED + 10 {
            limiter.check(Uuid::new_v4());
        }

        assert!(
            limiter.attempts.lock().unwrap().len() <= MAX_TRACKED,
            "the map must not grow without bound"
        );
    }

    #[test]
    fn expired_windows_are_dropped_before_live_ones_when_the_table_is_full() {
        let limiter = MfaRateLimiter::with_window(Duration::from_secs(10));
        let blocked = Uuid::new_v4();
        {
            let long_ago = Instant::now()
                .checked_sub(Duration::from_secs(20))
                .expect("the host has been up for more than 20 s");
            let mut attempts = limiter.attempts.lock().unwrap();
            for _ in 0..MAX_TRACKED - 1 {
                attempts.insert(Uuid::new_v4(), (1, long_ago));
            }
            attempts.insert(blocked, (MAX_ATTEMPTS + 1, Instant::now()));
        }

        assert!(limiter.check(Uuid::new_v4()), "a new user is admitted");
        assert!(
            !limiter.check(blocked),
            "the live, exhausted window survived the purge of expired ones"
        );
    }
}
