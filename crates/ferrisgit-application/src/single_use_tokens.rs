//! Remembers which short-lived tokens have already been spent. In memory and per instance, like a
//! login throttle: a restart forgets, which only matters for the few minutes a token lives.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

const MAX_REMEMBERED: usize = 10_000;

#[derive(Clone)]
pub struct SingleUseTokens {
    lifetime: Duration,
    spent: Arc<Mutex<HashMap<[u8; 32], Instant>>>,
}

impl SingleUseTokens {
    /// `lifetime` must outlast the tokens being tracked, or a spent one could be replayed once it is forgotten.
    pub fn new(lifetime: Duration) -> Self {
        Self {
            lifetime,
            spent: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// `true` the first time a token is seen. `false` for a replay, or when the table is full of tokens
    /// still inside their lifetime (better to make someone log in again than to forget one).
    pub fn consume(&self, token: &str) -> bool {
        let digest = fingerprint(token);
        let mut spent = self
            .spent
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if spent.contains_key(&digest) {
            return false;
        }
        if spent.len() >= MAX_REMEMBERED {
            spent.retain(|_, at| at.elapsed() < self.lifetime);
            if spent.len() >= MAX_REMEMBERED {
                return false;
            }
        }
        spent.insert(digest, Instant::now());
        true
    }

    /// Read-only: whether `token` was already spent (and not yet forgotten). Lets a caller refuse a
    /// spent token up front, before doing work that a later `consume` could only reject too late.
    pub fn is_spent(&self, token: &str) -> bool {
        self.spent
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .contains_key(&fingerprint(token))
    }
}

/// Only the digest is kept, so a memory dump of the table does not hold live tokens.
fn fingerprint(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_can_be_consumed_once() {
        let tokens = SingleUseTokens::new(Duration::from_secs(60));
        assert!(tokens.consume("abc"));
        assert!(!tokens.consume("abc"));
    }

    #[test]
    fn is_spent_reports_a_consumed_token_without_consuming_anything() {
        let tokens = SingleUseTokens::new(Duration::from_secs(60));
        assert!(!tokens.is_spent("abc"));
        assert!(!tokens.is_spent("abc"), "asking must not spend the token");
        assert!(tokens.consume("abc"));
        assert!(tokens.is_spent("abc"));
        assert!(!tokens.is_spent("def"));
    }

    #[test]
    fn different_tokens_are_independent() {
        let tokens = SingleUseTokens::new(Duration::from_secs(60));
        assert!(tokens.consume("abc"));
        assert!(tokens.consume("def"));
        assert!(!tokens.consume("def"));
    }

    #[test]
    fn concurrent_consumers_of_one_token_yield_exactly_one_winner() {
        let tokens = SingleUseTokens::new(Duration::from_secs(60));
        let handles: Vec<_> = (0..32)
            .map(|_| {
                let tokens = tokens.clone();
                std::thread::spawn(move || tokens.consume("shared"))
            })
            .collect();

        assert_eq!(
            handles
                .into_iter()
                .map(|h| h.join().unwrap())
                .filter(|won| *won)
                .count(),
            1
        );
    }

    #[test]
    fn a_full_table_refuses_rather_than_forgetting_a_live_token() {
        let tokens = SingleUseTokens::new(Duration::from_secs(60));
        for i in 0..MAX_REMEMBERED {
            assert!(tokens.consume(&format!("token-{i}")));
        }

        assert!(!tokens.consume("one-more"));
        assert!(!tokens.consume("token-0"), "a remembered token stays spent");
    }

    #[test]
    fn expired_entries_make_room_when_the_table_is_full() {
        let tokens = SingleUseTokens::new(Duration::from_millis(20));
        for i in 0..MAX_REMEMBERED {
            tokens.consume(&format!("token-{i}"));
        }
        std::thread::sleep(Duration::from_millis(40));

        assert!(tokens.consume("fresh"));
    }
}
