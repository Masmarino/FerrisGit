//! In-progress WebAuthn ceremonies between `start` and `finish`.
//!
//! Kept in memory: there's a single replica, so a restart mid-ceremony just makes the client retry. A ceremony is single
//! use, bound to its user and lives five minutes. The store is capped per user and in total because the login `start`
//! routes are unauthenticated, and past a cap the oldest ceremony goes.
//!
//! Nothing in here may panic or spin: a dangling entry, say after a recovered poisoned lock, is dropped, not trusted.

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;
use webauthn_rs::prelude::{PasskeyAuthentication, PasskeyRegistration};

const CEREMONY_TTL_MINUTES: i64 = 5;
const MAX_CEREMONIES: usize = 10_000;
const MAX_CEREMONIES_PER_USER: usize = 5;

pub enum CeremonyState {
    Registration(PasskeyRegistration),
    Authentication(PasskeyAuthentication),
}

struct CeremonyEntry {
    user_id: Uuid,
    state: CeremonyState,
    expires_at: DateTime<Utc>,
    seq: u64,
}

#[derive(Default)]
struct Ceremonies {
    entries: HashMap<Uuid, CeremonyEntry>,
    by_age: BTreeMap<u64, Uuid>,
    per_user: HashMap<Uuid, Vec<u64>>,
    next_seq: u64,
}

impl Ceremonies {
    fn remove(&mut self, challenge_id: Uuid) -> Option<CeremonyEntry> {
        let entry = self.entries.remove(&challenge_id)?;
        self.by_age.remove(&entry.seq);
        if let Some(seqs) = self.per_user.get_mut(&entry.user_id) {
            seqs.retain(|seq| *seq != entry.seq);
            if seqs.is_empty() {
                self.per_user.remove(&entry.user_id);
            }
        }
        Some(entry)
    }

    /// Insertion order only lets the sweep stop at the first live entry. `take_at` checks the entry's own expiry, so
    /// correctness doesn't depend on it.
    fn drop_expired(&mut self, now: DateTime<Utc>) {
        while let Some((seq, challenge_id)) = self.by_age.iter().next().map(|(seq, id)| (*seq, *id))
        {
            match self.entries.get(&challenge_id) {
                Some(entry) if entry.expires_at > now => break,
                Some(_) => {
                    self.remove(challenge_id);
                }
                None => {
                    self.by_age.remove(&seq);
                }
            }
        }
    }

    fn drop_oldest(&mut self) -> bool {
        while let Some((seq, challenge_id)) = self.by_age.iter().next().map(|(seq, id)| (*seq, *id))
        {
            if self.remove(challenge_id).is_some() {
                return true;
            }
            self.by_age.remove(&seq);
        }
        // Entries with no age record can't be ordered, and an unbounded store is worse than an empty one.
        let had_orphans = !self.entries.is_empty();
        self.entries.clear();
        self.per_user.clear();
        had_orphans
    }

    fn drop_oldest_of(&mut self, user_id: Uuid) -> bool {
        loop {
            let Some(seqs) = self.per_user.get_mut(&user_id) else {
                return false;
            };
            if seqs.is_empty() {
                self.per_user.remove(&user_id);
                return false;
            }
            let seq = seqs.remove(0);
            if seqs.is_empty() {
                self.per_user.remove(&user_id);
            }
            if let Some(challenge_id) = self.by_age.get(&seq).copied() {
                if self.remove(challenge_id).is_some() {
                    return true;
                }
                self.by_age.remove(&seq);
            }
        }
    }
}

pub struct PasskeyCeremonies {
    ceremonies: Mutex<Ceremonies>,
    ttl: Duration,
    max_total: usize,
    max_per_user: usize,
}

impl Default for PasskeyCeremonies {
    fn default() -> Self {
        Self::new()
    }
}

impl PasskeyCeremonies {
    pub fn new() -> Self {
        Self::with_limits(
            Duration::minutes(CEREMONY_TTL_MINUTES),
            MAX_CEREMONIES,
            MAX_CEREMONIES_PER_USER,
        )
    }

    /// Both caps are at least 1, since a zero cap could never be satisfied.
    pub fn with_limits(ttl: Duration, max_total: usize, max_per_user: usize) -> Self {
        Self {
            ceremonies: Mutex::new(Ceremonies::default()),
            ttl,
            max_total: max_total.max(1),
            max_per_user: max_per_user.max(1),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Ceremonies> {
        self.ceremonies
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn put(&self, user_id: Uuid, state: CeremonyState) -> Uuid {
        self.put_at(user_id, state, Utc::now())
    }

    /// Single use, like a nonce. Another user's attempt leaves the ceremony for its owner.
    pub fn take(&self, challenge_id: Uuid, user_id: Uuid) -> Option<CeremonyState> {
        self.take_at(challenge_id, user_id, Utc::now())
    }

    pub(crate) fn put_at(&self, user_id: Uuid, state: CeremonyState, now: DateTime<Utc>) -> Uuid {
        let mut ceremonies = self.lock();
        ceremonies.drop_expired(now);
        while ceremonies
            .per_user
            .get(&user_id)
            .is_some_and(|seqs| seqs.len() >= self.max_per_user)
        {
            if !ceremonies.drop_oldest_of(user_id) {
                break;
            }
        }
        while ceremonies.entries.len() >= self.max_total {
            if !ceremonies.drop_oldest() {
                break;
            }
        }
        let challenge_id = Uuid::new_v4();
        let seq = ceremonies.next_seq;
        ceremonies.next_seq += 1;
        ceremonies.entries.insert(
            challenge_id,
            CeremonyEntry {
                user_id,
                state,
                expires_at: now + self.ttl,
                seq,
            },
        );
        ceremonies.by_age.insert(seq, challenge_id);
        ceremonies.per_user.entry(user_id).or_default().push(seq);
        challenge_id
    }

    pub(crate) fn take_at(
        &self,
        challenge_id: Uuid,
        user_id: Uuid,
        now: DateTime<Utc>,
    ) -> Option<CeremonyState> {
        let mut ceremonies = self.lock();
        ceremonies.drop_expired(now);
        let entry = ceremonies.entries.get(&challenge_id)?;
        if entry.expires_at <= now {
            // Expired although an older ceremony held the sweep back, which happens when the clock steps back.
            ceremonies.remove(challenge_id);
            return None;
        }
        if entry.user_id != user_id {
            return None;
        }
        ceremonies.remove(challenge_id).map(|entry| entry.state)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.lock().entries.len()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::{Duration, Utc};
    use url::Url;
    use uuid::Uuid;
    use webauthn_rs::WebauthnBuilder;

    use super::*;

    fn state() -> CeremonyState {
        let origin = Url::parse("http://localhost:4200").unwrap();
        let webauthn = WebauthnBuilder::new("localhost", &origin)
            .unwrap()
            .rp_name("FerrisGit")
            .build()
            .unwrap();
        let (_, registration) = webauthn
            .start_passkey_registration(Uuid::new_v4(), "florian", "florian", None)
            .unwrap();
        CeremonyState::Registration(registration)
    }

    fn is_registration(state: &CeremonyState) -> bool {
        matches!(state, CeremonyState::Registration(_))
    }

    #[test]
    fn a_ceremony_can_be_taken_once_by_its_user() {
        let store = PasskeyCeremonies::new();
        let user = Uuid::new_v4();
        let challenge = store.put(user, state());

        assert!(is_registration(
            &store.take(challenge, user).expect("first take")
        ));
        assert!(store.take(challenge, user).is_none(), "single use");
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn an_unknown_challenge_is_none() {
        let store = PasskeyCeremonies::new();

        assert!(store.take(Uuid::new_v4(), Uuid::new_v4()).is_none());
    }

    #[test]
    fn another_user_cannot_take_a_ceremony_and_does_not_consume_it() {
        let store = PasskeyCeremonies::new();
        let owner = Uuid::new_v4();
        let challenge = store.put(owner, state());

        assert!(store.take(challenge, Uuid::new_v4()).is_none());

        assert_eq!(
            store.len(),
            1,
            "the wrong user did not burn the owner's ceremony"
        );
        assert!(store.take(challenge, owner).is_some());
    }

    #[test]
    fn a_ceremony_expires_after_its_ttl() {
        let store = PasskeyCeremonies::with_limits(Duration::minutes(5), 100, 5);
        let user = Uuid::new_v4();
        let t0 = Utc::now();
        let challenge = store.put_at(user, state(), t0);

        assert!(
            store
                .take_at(
                    challenge,
                    user,
                    t0 + Duration::minutes(5) - Duration::seconds(1)
                )
                .is_some(),
            "just before the ttl"
        );

        let challenge = store.put_at(user, state(), t0);
        assert!(
            store
                .take_at(challenge, user, t0 + Duration::minutes(5))
                .is_none(),
            "at the ttl"
        );
        assert_eq!(store.len(), 0, "an expired entry is dropped, not kept");
    }

    #[test]
    fn the_default_ttl_is_five_minutes() {
        let store = PasskeyCeremonies::new();
        let user = Uuid::new_v4();
        let t0 = Utc::now();
        let challenge = store.put_at(user, state(), t0);

        assert!(
            store
                .take_at(challenge, user, t0 + Duration::minutes(4))
                .is_some()
        );
        let challenge = store.put_at(user, state(), t0);
        assert!(
            store
                .take_at(challenge, user, t0 + Duration::minutes(6))
                .is_none()
        );
    }

    #[test]
    fn a_new_ceremony_sweeps_the_expired_ones() {
        let store = PasskeyCeremonies::with_limits(Duration::minutes(5), 100, 5);
        let t0 = Utc::now();
        for _ in 0..3 {
            store.put_at(Uuid::new_v4(), state(), t0);
        }
        assert_eq!(store.len(), 3);

        store.put_at(Uuid::new_v4(), state(), t0 + Duration::minutes(6));

        assert_eq!(store.len(), 1);
    }

    #[test]
    fn the_per_user_cap_drops_that_users_oldest_ceremony() {
        let store = PasskeyCeremonies::new();
        let user = Uuid::new_v4();
        let other = Uuid::new_v4();
        let other_challenge = store.put(other, state());
        let challenges: Vec<Uuid> = (0..5).map(|_| store.put(user, state())).collect();

        let sixth = store.put(user, state());

        assert!(
            store.take(challenges[0], user).is_none(),
            "the oldest of the user was dropped"
        );
        for challenge in &challenges[1..] {
            assert!(store.take(*challenge, user).is_some());
        }
        assert!(store.take(sixth, user).is_some());
        assert!(
            store.take(other_challenge, other).is_some(),
            "another user is not affected by the cap"
        );
    }

    #[test]
    fn the_total_cap_drops_the_oldest_ceremony_overall() {
        let store = PasskeyCeremonies::with_limits(Duration::minutes(5), 3, 5);
        let challenges: Vec<(Uuid, Uuid)> = (0..3)
            .map(|_| {
                let user = Uuid::new_v4();
                (store.put(user, state()), user)
            })
            .collect();

        let newest_user = Uuid::new_v4();
        let newest = store.put(newest_user, state());

        assert_eq!(store.len(), 3);
        assert!(
            store.take(challenges[0].0, challenges[0].1).is_none(),
            "the globally oldest one went"
        );
        assert!(store.take(challenges[1].0, challenges[1].1).is_some());
        assert!(store.take(newest, newest_user).is_some());
    }

    #[test]
    fn the_default_caps_are_ten_thousand_in_total_and_five_per_user() {
        let store = PasskeyCeremonies::new();

        assert_eq!(store.max_total, 10_000);
        assert_eq!(store.max_per_user, 5);
    }

    #[test]
    fn the_per_user_bookkeeping_is_cleaned_up_when_ceremonies_are_taken() {
        let store = PasskeyCeremonies::new();
        let user = Uuid::new_v4();
        let challenge = store.put(user, state());
        store.take(challenge, user).unwrap();

        let inner = store.lock();
        assert!(inner.per_user.is_empty());
        assert!(inner.by_age.is_empty());
    }

    #[test]
    fn concurrent_takes_of_one_challenge_yield_exactly_one_winner() {
        let store = Arc::new(PasskeyCeremonies::new());
        let user = Uuid::new_v4();
        let challenge = store.put(user, state());

        let threads: Vec<_> = (0..16)
            .map(|_| {
                let store = store.clone();
                std::thread::spawn(move || store.take(challenge, user).is_some())
            })
            .collect();
        let winners = threads
            .into_iter()
            .map(|t| t.join().unwrap())
            .filter(|won| *won)
            .count();

        assert_eq!(winners, 1);
    }

    #[test]
    fn a_ceremony_is_refused_after_its_own_expiry_even_when_an_older_one_is_still_alive() {
        let store = PasskeyCeremonies::with_limits(Duration::minutes(5), 100, 5);
        let t0 = Utc::now();
        let (first_user, second_user) = (Uuid::new_v4(), Uuid::new_v4());
        let first = store.put_at(first_user, state(), t0);
        // The clock stepped back 3 minutes, so the second ceremony expires (t0 + 2 min) before the first.
        let second = store.put_at(second_user, state(), t0 - Duration::minutes(3));

        assert!(
            store
                .take_at(second, second_user, t0 + Duration::minutes(3))
                .is_none(),
            "past its own expiry"
        );
        assert!(
            store
                .take_at(first, first_user, t0 + Duration::minutes(3))
                .is_some(),
            "the first one is still alive"
        );
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn a_wrong_users_take_of_an_expired_ceremony_does_not_resurrect_it() {
        let store = PasskeyCeremonies::with_limits(Duration::minutes(5), 100, 5);
        let t0 = Utc::now();
        let owner = Uuid::new_v4();
        store.put_at(Uuid::new_v4(), state(), t0);
        let stepped_back = store.put_at(owner, state(), t0 - Duration::minutes(3));

        assert!(
            store
                .take_at(stepped_back, Uuid::new_v4(), t0 + Duration::minutes(3))
                .is_none()
        );
        assert!(
            store
                .take_at(stepped_back, owner, t0 + Duration::minutes(3))
                .is_none()
        );
    }

    #[test]
    fn limits_below_one_are_clamped_so_the_store_still_works_and_never_spins() {
        let store = PasskeyCeremonies::with_limits(Duration::minutes(5), 0, 0);
        let user = Uuid::new_v4();

        let first = store.put(user, state());
        let second = store.put(user, state());

        assert_eq!(store.len(), 1);
        assert!(store.take(first, user).is_none(), "the oldest was dropped");
        assert!(store.take(second, user).is_some());
    }

    #[test]
    fn dangling_bookkeeping_is_healed_instead_of_panicking() {
        let store = PasskeyCeremonies::with_limits(Duration::minutes(5), 3, 2);
        let user = Uuid::new_v4();
        {
            // As if a panic elsewhere had left the maps inconsistent (the poisoned lock is recovered).
            let mut inner = store.lock();
            inner.by_age.insert(0, Uuid::new_v4());
            inner.per_user.entry(user).or_default().extend([0, 41]);
            inner.by_age.insert(41, Uuid::new_v4());
            inner.next_seq = 100;
        }

        let a = store.put(user, state());
        let b = store.put(user, state());
        store.put(Uuid::new_v4(), state());
        store.put(Uuid::new_v4(), state());
        let last = store.put(user, state());

        let _ = (a, b); // whatever got dropped, nothing may panic and the newest has to be takeable
        assert!(store.take(last, user).is_some());
        assert!(store.len() <= 3);
        let inner = store.lock();
        assert!(
            inner
                .by_age
                .values()
                .all(|id| inner.entries.contains_key(id)),
            "no dangling age entry is left"
        );
    }

    #[test]
    fn orphaned_entries_without_age_bookkeeping_are_cleared_when_the_total_cap_is_hit() {
        let store = PasskeyCeremonies::with_limits(Duration::minutes(5), 2, 5);
        {
            let mut inner = store.lock();
            inner.entries.insert(
                Uuid::new_v4(),
                CeremonyEntry {
                    user_id: Uuid::new_v4(),
                    state: state(),
                    expires_at: Utc::now() + Duration::minutes(5),
                    seq: 7,
                },
            );
            inner.entries.insert(
                Uuid::new_v4(),
                CeremonyEntry {
                    user_id: Uuid::new_v4(),
                    state: state(),
                    expires_at: Utc::now() + Duration::minutes(5),
                    seq: 8,
                },
            );
        }

        let user = Uuid::new_v4();
        let challenge = store.put(user, state());

        assert!(store.take(challenge, user).is_some());
    }
}
