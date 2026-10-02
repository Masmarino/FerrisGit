use std::sync::Arc;

use ferrisgit_domain::audit::{EventPublisherPort, SecurityEvent};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::user::{PasswordHasherPort, UserRepositoryPort};
use uuid::Uuid;

/// A real argon2 hash, so an unknown username costs the same as a wrong password. Otherwise timing reveals which
/// usernames exist.
const DUMMY_PASSWORD_HASH: &str =
    "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQ$RdescudvJCsgt3ub+b+dWRWJTmaaJObG";

pub struct LoginUseCase {
    users: Arc<dyn UserRepositoryPort>,
    hasher: Arc<dyn PasswordHasherPort>,
    events: Arc<dyn EventPublisherPort>,
}

impl LoginUseCase {
    pub fn new(
        users: Arc<dyn UserRepositoryPort>,
        hasher: Arc<dyn PasswordHasherPort>,
        events: Arc<dyn EventPublisherPort>,
    ) -> Self {
        Self {
            users,
            hasher,
            events,
        }
    }

    /// Only checks credentials and returns the user's id. Issuing the session or MFA token is the caller's job.
    pub async fn execute(&self, username: &str, password: &str) -> Result<Uuid, DomainError> {
        let mut user = self.users.find_by_username(username).await?;
        // Usernames are stored trimmed and lower-cased. Try the exact name first so legacy mixed-case accounts keep
        // working, then the normalised one. That's unambiguous, new names being unique case-insensitively.
        if user.is_none() {
            let normalised = username.trim().to_lowercase();
            if normalised != username {
                user = self.users.find_by_username(&normalised).await?;
            }
        }

        let (user, password_hash) = match user {
            Some(user) => {
                let hash = user.password_hash.clone();
                (Some(user), hash)
            }
            None => (None, DUMMY_PASSWORD_HASH.to_string()),
        };

        let hasher = self.hasher.clone();
        let password_owned = password.to_string();
        let is_valid =
            tokio::task::spawn_blocking(move || hasher.verify(&password_owned, &password_hash))
                .await
                .map_err(|e| DomainError::Infrastructure(e.to_string()))??;

        // Grab the id before `filter` consumes the user: a wrong password on a known username keeps its real id in the
        // audit event, which makes brute force filterable per account.
        let user_id_if_known = user.as_ref().map(|u| u.id);

        let Some(user) = user.filter(|_| is_valid) else {
            self.events
                .publish_security_event(
                    SecurityEvent::LoginFailed {
                        username: username.to_string(),
                    },
                    user_id_if_known,
                )
                .await
                .ok();
            return Err(DomainError::Unauthorized(
                "invalid username or password".to_string(),
            ));
        };

        Ok(user.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeEvents, FakeHasher, FakeUsers};
    use crate::use_cases::fixtures;
    use ferrisgit_domain::user::User;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Counts `verify` calls.
    struct CountingHasher(AtomicUsize);
    impl PasswordHasherPort for CountingHasher {
        fn hash(&self, plain: &str) -> Result<String, DomainError> {
            Ok(format!("hashed:{plain}"))
        }
        fn verify(&self, plain: &str, hash: &str) -> Result<bool, DomainError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(hash == format!("hashed:{plain}"))
        }
    }

    fn account(username: &str, password: &str) -> User {
        User {
            password_hash: format!("hashed:{password}"),
            ..fixtures::user(username)
        }
    }

    fn use_case_with_events(users: Vec<User>) -> (LoginUseCase, Arc<FakeEvents>) {
        let events = Arc::new(FakeEvents::new());
        let use_case = LoginUseCase::new(
            Arc::new(FakeUsers::new(users)),
            Arc::new(FakeHasher),
            events.clone(),
        );
        (use_case, events)
    }

    fn use_case_with(users: Vec<User>) -> LoginUseCase {
        use_case_with_events(users).0
    }

    fn use_case_with_counting_hasher(
        users: Vec<User>,
    ) -> (LoginUseCase, Arc<CountingHasher>, Arc<FakeEvents>) {
        let hasher = Arc::new(CountingHasher(AtomicUsize::new(0)));
        let events = Arc::new(FakeEvents::new());
        let use_case = LoginUseCase::new(
            Arc::new(FakeUsers::new(users)),
            hasher.clone(),
            events.clone(),
        );
        (use_case, hasher, events)
    }

    #[tokio::test]
    async fn logging_in_with_correct_credentials_returns_the_user_id() {
        let user = account("florian", "secret");
        let use_case = use_case_with(vec![user.clone()]);

        let user_id = use_case.execute("florian", "secret").await.unwrap();
        assert_eq!(user_id, user.id);
    }

    #[tokio::test]
    async fn a_correct_password_publishes_no_event() {
        let (use_case, events) = use_case_with_events(vec![account("florian", "secret")]);

        use_case.execute("florian", "secret").await.unwrap();

        assert!(events.security_events().is_empty());
    }

    #[tokio::test]
    async fn logging_in_with_the_wrong_password_is_unauthorized() {
        let use_case = use_case_with(vec![account("florian", "secret")]);

        let result = use_case.execute("florian", "wrong").await;
        assert!(matches!(result, Err(DomainError::Unauthorized(_))));
    }

    #[tokio::test]
    async fn logging_in_with_an_unknown_username_is_unauthorized() {
        let use_case = use_case_with(vec![]);
        let result = use_case.execute("nobody", "secret").await;
        assert!(matches!(result, Err(DomainError::Unauthorized(_))));
    }

    #[tokio::test]
    async fn the_hasher_is_invoked_exactly_once_for_both_known_and_unknown_usernames() {
        // The fake hasher is too cheap to time, so this only checks the dummy-hash branch still calls the hasher.
        let (use_case, hasher, _) =
            use_case_with_counting_hasher(vec![account("florian", "secret")]);

        let _ = use_case.execute("florian", "wrong").await;
        assert_eq!(
            hasher.0.load(Ordering::SeqCst),
            1,
            "known username, wrong password should call verify exactly once"
        );

        let _ = use_case.execute("nobody", "wrong").await;
        assert_eq!(
            hasher.0.load(Ordering::SeqCst),
            2,
            "unknown username should still call verify exactly once (dummy hash)"
        );
    }

    #[tokio::test]
    async fn a_wrong_password_for_a_known_username_publishes_login_failed_with_the_real_user_id_as_actor()
     {
        let user = account("florian", "secret");
        let user_id = user.id;
        let (use_case, events) = use_case_with_events(vec![user]);

        let result = use_case.execute("florian", "wrong").await;
        assert!(matches!(result, Err(DomainError::Unauthorized(_))));

        let published = events.security_events();
        assert_eq!(published.len(), 1);
        assert!(
            matches!(&published[0].0, SecurityEvent::LoginFailed { username } if username == "florian")
        );
        assert_eq!(
            published[0].1,
            Some(user_id),
            "actor_id must be the real user id for a known username"
        );
    }

    #[tokio::test]
    async fn an_unknown_username_publishes_login_failed_with_no_actor_id() {
        let (use_case, events) = use_case_with_events(vec![]);

        let result = use_case.execute("nobody", "secret").await;
        assert!(matches!(result, Err(DomainError::Unauthorized(_))));

        let published = events.security_events();
        assert_eq!(published.len(), 1);
        assert!(
            matches!(&published[0].0, SecurityEvent::LoginFailed { username } if username == "nobody")
        );
        assert_eq!(
            published[0].1, None,
            "actor_id must be None for an unknown username"
        );
    }

    #[tokio::test]
    async fn a_mixed_case_username_finds_the_lower_cased_account() {
        let alice = account("alice_1", "secret");
        let use_case = use_case_with(vec![alice.clone()]);

        assert_eq!(
            use_case.execute("Alice_1", "secret").await.unwrap(),
            alice.id
        );
        assert_eq!(
            use_case.execute("ALICE_1", "secret").await.unwrap(),
            alice.id
        );
        assert_eq!(
            use_case.execute("  Alice_1 ", "secret").await.unwrap(),
            alice.id,
            "surrounding whitespace is trimmed like at sign-up"
        );
        assert!(
            matches!(
                use_case.execute("Alice_1", "wrong").await,
                Err(DomainError::Unauthorized(_))
            ),
            "the password is still checked"
        );
    }

    #[tokio::test]
    async fn the_exact_username_wins_over_its_lower_cased_form() {
        let legacy = account("Florian", "legacy-secret");
        let modern = account("florian", "modern-secret");
        let use_case = use_case_with(vec![legacy.clone(), modern.clone()]);

        assert_eq!(
            use_case.execute("Florian", "legacy-secret").await.unwrap(),
            legacy.id
        );
        assert_eq!(
            use_case.execute("florian", "modern-secret").await.unwrap(),
            modern.id
        );
        assert!(
            matches!(
                use_case.execute("Florian", "modern-secret").await,
                Err(DomainError::Unauthorized(_))
            ),
            "no fallback once the exact account exists"
        );
    }

    #[tokio::test]
    async fn an_unknown_mixed_case_username_still_verifies_exactly_once_and_logs_what_was_typed() {
        let (use_case, hasher, events) =
            use_case_with_counting_hasher(vec![account("alice", "secret")]);

        let result = use_case.execute("Nobody", "secret").await;

        assert!(matches!(result, Err(DomainError::Unauthorized(_))));
        assert_eq!(hasher.0.load(Ordering::SeqCst), 1);
        let published = events.security_events();
        assert!(
            matches!(&published[0].0, SecurityEvent::LoginFailed { username } if username == "Nobody")
        );
        assert_eq!(published[0].1, None);
    }

    #[tokio::test]
    async fn a_wrong_password_on_a_mixed_case_username_is_attributed_to_the_real_account() {
        let alice = account("alice", "secret");
        let (use_case, events) = use_case_with_events(vec![alice.clone()]);

        let _ = use_case.execute("ALICE", "wrong").await;

        assert_eq!(events.security_events()[0].1, Some(alice.id));
    }
}
