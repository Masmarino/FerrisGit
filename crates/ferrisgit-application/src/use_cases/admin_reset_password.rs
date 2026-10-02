//! Password reset by an admin: they issue a one-hour link, the user follows it and picks a new password. Tokens work
//! like invitations: 64 random hex chars, only the SHA-256 stored, consumed once.

use std::sync::Arc;

use chrono::{Duration, Utc};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::invitation::UserInvitationPort;
use ferrisgit_domain::password_reset::PasswordResetPort;
use ferrisgit_domain::user::{PasswordHasherPort, User, UserRepositoryPort};
use uuid::Uuid;

use crate::account_rules::{
    PASSWORD_RESET_TTL_HOURS, generate_invitation_token, hash_blocking, is_invitation_token_shaped,
    validate_password,
};
use crate::token_hash::hash_token;

/// Same error for unknown, expired, used or malformed tokens, so nobody can probe links.
const INVALID_LINK: &str = "invalid or expired password reset link";

/// The token is only there to build the reset URL: it's never stored (just its hash) and `Debug` hides it.
#[derive(Clone)]
pub struct IssuedPasswordReset {
    pub user: User,
    pub token: String,
}

impl std::fmt::Debug for IssuedPasswordReset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IssuedPasswordReset")
            .field("user", &self.user)
            .field("token", &"[redacted]")
            .finish()
    }
}

/// Replaces any previous link. The current password is swapped for a random secret's hash right away (as for an
/// invitation): otherwise a lapsed link would leave the old password working, and after an MFA reset whoever knows it
/// could enrol their own factor.
pub struct AdminResetPasswordUseCase {
    users: Arc<dyn UserRepositoryPort>,
    hasher: Arc<dyn PasswordHasherPort>,
    invitations: Arc<dyn UserInvitationPort>,
    password_resets: Arc<dyn PasswordResetPort>,
}

impl AdminResetPasswordUseCase {
    pub fn new(
        users: Arc<dyn UserRepositoryPort>,
        hasher: Arc<dyn PasswordHasherPort>,
        invitations: Arc<dyn UserInvitationPort>,
        password_resets: Arc<dyn PasswordResetPort>,
    ) -> Self {
        Self {
            users,
            hasher,
            invitations,
            password_resets,
        }
    }

    pub async fn execute(
        &self,
        actor_id: Uuid,
        target_user_id: Uuid,
    ) -> Result<IssuedPasswordReset, DomainError> {
        // Losing the mail would lock an admin out of their own account, and a sole admin has nobody to issue another
        // link. They go through the normal password change instead.
        if target_user_id == actor_id {
            return Err(DomainError::Validation(
                "use your account settings to change your own password".to_string(),
            ));
        }
        let user = self
            .users
            .find_by_id(target_user_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("user".to_string()))?;
        // An invited account has its activation link already; a reset link on top would mean two live links.
        if self
            .invitations
            .expiries(&[user.id])
            .await?
            .iter()
            .any(|(id, _)| *id == user.id)
        {
            return Err(DomainError::Validation(
                "the user has not activated their account yet; resend the invitation instead"
                    .to_string(),
            ));
        }
        // Hash first (it's slow), so a hasher failure happens before anything is written.
        let unusable_password_hash =
            hash_blocking(&self.hasher, generate_invitation_token()).await?;
        // Order matters: revoke sessions, disable the password, then store the link. If the last step fails the
        // account is just locked with no link, and the admin retries.
        self.users.bump_token_epoch(user.id).await?;
        self.users
            .update_password_hash(user.id, unusable_password_hash)
            .await?;
        let token = generate_invitation_token();
        self.password_resets
            .replace(
                user.id,
                &hash_token(&token),
                Utc::now() + Duration::hours(PASSWORD_RESET_TTL_HOURS),
            )
            .await?;
        Ok(IssuedPasswordReset { user, token })
    }
}

/// No session is issued: the user logs in afterwards, MFA included (a reset leaves factors alone). Returns whose
/// password was set, for the notification.
pub struct ConsumePasswordResetUseCase {
    users: Arc<dyn UserRepositoryPort>,
    hasher: Arc<dyn PasswordHasherPort>,
    password_resets: Arc<dyn PasswordResetPort>,
}

impl ConsumePasswordResetUseCase {
    pub fn new(
        users: Arc<dyn UserRepositoryPort>,
        hasher: Arc<dyn PasswordHasherPort>,
        password_resets: Arc<dyn PasswordResetPort>,
    ) -> Self {
        Self {
            users,
            hasher,
            password_resets,
        }
    }

    pub async fn execute(&self, token: &str, new_password: &str) -> Result<Uuid, DomainError> {
        // Hash before consuming, as in account activation, so a bad password or a hasher failure doesn't burn the link.
        // Malformed tokens are refused up front, before any hashing.
        if !is_invitation_token_shaped(token) {
            return Err(DomainError::Validation(INVALID_LINK.to_string()));
        }
        validate_password(new_password)?;
        let password_hash = hash_blocking(&self.hasher, new_password.to_string()).await?;
        let token_hash = hash_token(token);
        let reset = self
            .password_resets
            .consume(&token_hash)
            .await?
            .ok_or_else(|| DomainError::Validation(INVALID_LINK.to_string()))?;
        if let Err(error) = self
            .users
            .update_password_hash(reset.user_id, password_hash)
            .await
        {
            // Put the link back so the user can retry. `restore` rather than `replace`, so a newer link issued by an
            // admin in the meantime wins.
            let _ = self
                .password_resets
                .restore(reset.user_id, &token_hash, reset.expires_at)
                .await;
            return Err(error);
        }
        // The admin already bumped it, but it's cheap and keeps "a password write revokes sessions" true by itself.
        self.users.bump_token_epoch(reset.user_id).await?;
        Ok(reset.user_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        FakeHasher, FakeInvitations, FakePasswordResets, FakeUsers, ThreadRecordingHasher,
    };
    use crate::use_cases::fixtures::{is_hex64, user};
    use chrono::DateTime;
    use ferrisgit_domain::password_reset::PasswordReset;

    struct Fixture {
        users: Arc<FakeUsers>,
        invitations: Arc<FakeInvitations>,
        resets: Arc<FakePasswordResets>,
        admin_id: Uuid,
        issue: AdminResetPasswordUseCase,
        consume: ConsumePasswordResetUseCase,
    }

    impl Fixture {
        async fn reset(&self, target: Uuid) -> Result<IssuedPasswordReset, DomainError> {
            self.issue.execute(self.admin_id, target).await
        }
    }

    fn fixture(existing: Vec<User>) -> Fixture {
        let users = Arc::new(FakeUsers::new(existing));
        let invitations = Arc::new(FakeInvitations::new());
        let resets = Arc::new(FakePasswordResets::new());
        Fixture {
            issue: AdminResetPasswordUseCase::new(
                users.clone(),
                Arc::new(FakeHasher),
                invitations.clone(),
                resets.clone(),
            ),
            consume: ConsumePasswordResetUseCase::new(
                users.clone(),
                Arc::new(FakeHasher),
                resets.clone(),
            ),
            admin_id: Uuid::new_v4(),
            users,
            invitations,
            resets,
        }
    }

    const OLD_PASSWORD_HASH: &str = "hashed:old-password";

    fn alice() -> User {
        User {
            password_hash: OLD_PASSWORD_HASH.to_string(),
            ..user("alice")
        }
    }

    fn is_invalid_link<T: std::fmt::Debug>(result: &Result<T, DomainError>) -> bool {
        matches!(result, Err(DomainError::Validation(m)) if m == INVALID_LINK)
    }

    #[tokio::test]
    async fn an_admin_reset_returns_the_user_and_a_64_hex_token() {
        let user = alice();
        let f = fixture(vec![user.clone()]);

        let issued = f.reset(user.id).await.unwrap();

        assert_eq!(issued.user.id, user.id);
        assert!(is_hex64(&issued.token), "{}", issued.token);
    }

    #[tokio::test]
    async fn only_the_hash_of_the_token_is_stored_and_it_expires_in_one_hour() {
        let user = alice();
        let f = fixture(vec![user.clone()]);

        let issued = f.reset(user.id).await.unwrap();

        let (stored_hash, expires_at) = f.resets.row_of(user.id).unwrap();
        assert_eq!(stored_hash, hash_token(&issued.token));
        assert_ne!(stored_hash, issued.token);
        let expected = Utc::now() + Duration::hours(1);
        assert!(
            (expires_at - expected).num_seconds().abs() < 5,
            "expires_at {expires_at} vs {expected}"
        );
    }

    #[tokio::test]
    async fn an_admin_reset_revokes_every_session_of_the_target_right_away() {
        let user = alice();
        let f = fixture(vec![user.clone()]);

        f.reset(user.id).await.unwrap();

        assert_eq!(
            f.users.token_epoch_of(user.id),
            1,
            "the target's sessions must die before the link is even used"
        );
    }

    #[tokio::test]
    async fn an_admin_reset_kills_the_current_password_at_once_and_touches_nobody_else() {
        let user = alice();
        let other = User {
            id: Uuid::new_v4(),
            username: "bob".to_string(),
            ..alice()
        };
        let f = fixture(vec![user.clone(), other.clone()]);

        let issued = f.reset(user.id).await.unwrap();

        let stored = f.users.get(user.id).unwrap().password_hash;
        assert_ne!(stored, OLD_PASSWORD_HASH);
        for guess in ["old-password", "", "alice", issued.token.as_str()] {
            assert!(
                !FakeHasher.verify(guess, &stored).unwrap(),
                "{guess:?} must not verify: nobody can sign in until the link is used"
            );
        }
        assert_eq!(
            f.users.get(other.id).unwrap().password_hash,
            OLD_PASSWORD_HASH
        );
        assert_eq!(f.users.token_epoch_of(other.id), 0);
        assert!(f.resets.row_of(other.id).is_none());
    }

    #[tokio::test]
    async fn two_resets_leave_two_different_unusable_passwords() {
        let user = alice();
        let f = fixture(vec![user.clone()]);

        f.reset(user.id).await.unwrap();
        let first = f.users.get(user.id).unwrap().password_hash;
        f.reset(user.id).await.unwrap();

        assert_ne!(
            f.users.get(user.id).unwrap().password_hash,
            first,
            "a fresh random secret every time"
        );
    }

    #[tokio::test]
    async fn an_admin_reset_of_an_unknown_user_is_not_found_and_changes_nothing() {
        let f = fixture(vec![alice()]);
        let unknown = Uuid::new_v4();

        let result = f.reset(unknown).await;

        assert!(
            matches!(result, Err(DomainError::NotFound(_))),
            "{result:?}"
        );
        assert!(f.resets.snapshot().is_empty());
        assert_eq!(f.users.token_epoch_of(unknown), 0);
    }

    #[tokio::test]
    async fn an_admin_cannot_reset_their_own_password_this_way() {
        let admin = User {
            is_admin: true,
            ..alice()
        };
        let f = fixture(vec![admin.clone()]);

        let result = f.issue.execute(admin.id, admin.id).await;

        assert!(
            matches!(&result, Err(DomainError::Validation(m)) if m == "use your account settings to change your own password"),
            "{result:?}"
        );
        assert_eq!(
            f.users.get(admin.id).unwrap().password_hash,
            OLD_PASSWORD_HASH
        );
        assert_eq!(f.users.token_epoch_of(admin.id), 0);
        assert!(f.resets.snapshot().is_empty());
    }

    #[tokio::test]
    async fn an_account_still_pending_activation_is_refused_and_changes_nothing() {
        let user = alice();
        let f = fixture(vec![user.clone()]);
        f.invitations
            .insert(user.id, "invitation-hash", Utc::now() + Duration::hours(24));

        let result = f.reset(user.id).await;

        assert!(
            matches!(&result, Err(DomainError::Validation(m)) if m == "the user has not activated their account yet; resend the invitation instead"),
            "{result:?}"
        );
        assert_eq!(
            f.users.get(user.id).unwrap().password_hash,
            OLD_PASSWORD_HASH
        );
        assert_eq!(f.users.token_epoch_of(user.id), 0);
        assert!(f.resets.snapshot().is_empty());
        assert_eq!(
            f.invitations.snapshot().len(),
            1,
            "the invitation is untouched"
        );
    }

    #[tokio::test]
    async fn an_expired_pending_invitation_is_refused_too() {
        let user = alice();
        let f = fixture(vec![user.clone()]);
        f.invitations
            .insert(user.id, "invitation-hash", Utc::now() - Duration::hours(1));

        assert!(
            matches!(f.reset(user.id).await, Err(DomainError::Validation(_))),
            "an expired invitation still marks the account as never activated"
        );
    }

    #[tokio::test]
    async fn a_second_admin_reset_kills_the_first_link() {
        let user = alice();
        let f = fixture(vec![user.clone()]);
        let first = f.reset(user.id).await.unwrap();

        let second = f.reset(user.id).await.unwrap();

        assert_ne!(first.token, second.token);
        assert_eq!(f.resets.snapshot().len(), 1, "one live link per user");
        assert!(is_invalid_link(
            &f.consume.execute(&first.token, "new-password-1").await
        ));
        f.consume
            .execute(&second.token, "new-password-1")
            .await
            .unwrap();
    }

    struct BrokenResets;

    #[async_trait::async_trait]
    impl PasswordResetPort for BrokenResets {
        async fn replace(
            &self,
            _user_id: Uuid,
            _token_hash: &str,
            _expires_at: DateTime<Utc>,
        ) -> Result<(), DomainError> {
            Err(DomainError::Infrastructure("database down".to_string()))
        }
        async fn consume(&self, _token_hash: &str) -> Result<Option<PasswordReset>, DomainError> {
            Err(DomainError::Infrastructure("database down".to_string()))
        }
        async fn restore(
            &self,
            _user_id: Uuid,
            _token_hash: &str,
            _expires_at: DateTime<Utc>,
        ) -> Result<bool, DomainError> {
            Err(DomainError::Infrastructure("database down".to_string()))
        }
        async fn is_pending(&self, _user_id: Uuid) -> Result<bool, DomainError> {
            Err(DomainError::Infrastructure("database down".to_string()))
        }
    }

    #[tokio::test]
    async fn the_sessions_and_the_password_are_killed_before_the_link_is_stored() {
        let user = alice();
        let users = Arc::new(FakeUsers::new(vec![user.clone()]));
        let issue = AdminResetPasswordUseCase::new(
            users.clone(),
            Arc::new(FakeHasher),
            Arc::new(FakeInvitations::new()),
            Arc::new(BrokenResets),
        );

        let result = issue.execute(Uuid::new_v4(), user.id).await;

        assert!(
            matches!(result, Err(DomainError::Infrastructure(_))),
            "{result:?}"
        );
        assert_eq!(
            users.token_epoch_of(user.id),
            1,
            "a failed link store must still leave the sessions revoked"
        );
        assert!(
            !FakeHasher
                .verify("old-password", &users.get(user.id).unwrap().password_hash)
                .unwrap(),
            "and the old password dead: the admin retries"
        );
    }

    struct FailingHasher;

    impl PasswordHasherPort for FailingHasher {
        fn hash(&self, _plain: &str) -> Result<String, DomainError> {
            Err(DomainError::Infrastructure("hasher down".to_string()))
        }
        fn verify(&self, _plain: &str, _hash: &str) -> Result<bool, DomainError> {
            Ok(false)
        }
    }

    #[tokio::test]
    async fn a_hasher_failure_happens_before_any_write() {
        let user = alice();
        let users = Arc::new(FakeUsers::new(vec![user.clone()]));
        let resets = Arc::new(FakePasswordResets::new());
        let issue = AdminResetPasswordUseCase::new(
            users.clone(),
            Arc::new(FailingHasher),
            Arc::new(FakeInvitations::new()),
            resets.clone(),
        );

        let result = issue.execute(Uuid::new_v4(), user.id).await;

        assert!(
            matches!(result, Err(DomainError::Infrastructure(_))),
            "{result:?}"
        );
        assert_eq!(users.token_epoch_of(user.id), 0);
        assert_eq!(users.get(user.id).unwrap().password_hash, OLD_PASSWORD_HASH);
        assert!(resets.snapshot().is_empty());
    }

    #[tokio::test]
    async fn the_unusable_password_is_hashed_off_the_async_thread() {
        let user = alice();
        let hasher = Arc::new(ThreadRecordingHasher::default());
        let issue = AdminResetPasswordUseCase::new(
            Arc::new(FakeUsers::new(vec![user.clone()])),
            hasher.clone(),
            Arc::new(FakeInvitations::new()),
            Arc::new(FakePasswordResets::new()),
        );

        issue.execute(Uuid::new_v4(), user.id).await.unwrap();

        let threads = hasher.hash_threads();
        assert_eq!(threads.len(), 1);
        assert_ne!(threads[0], std::thread::current().id());
    }

    #[test]
    fn an_issued_reset_debug_output_never_contains_the_token() {
        let issued = IssuedPasswordReset {
            user: alice(),
            token: "SECRETTOKENVALUE".to_string(),
        };

        let printed = format!("{issued:?}");

        assert!(!printed.contains("SECRETTOKENVALUE"));
        assert!(printed.contains("[redacted]"));
    }

    #[tokio::test]
    async fn following_the_link_sets_the_new_password_and_returns_whose_it_is() {
        let user = alice();
        let f = fixture(vec![user.clone()]);
        let issued = f.reset(user.id).await.unwrap();

        let user_id = f
            .consume
            .execute(&issued.token, "my-new-password")
            .await
            .unwrap();

        assert_eq!(user_id, user.id);
        let stored = f.users.get(user.id).unwrap();
        assert!(
            FakeHasher
                .verify("my-new-password", &stored.password_hash)
                .unwrap()
        );
        assert!(
            !FakeHasher
                .verify("old-password", &stored.password_hash)
                .unwrap(),
            "the old password must no longer work"
        );
        assert!(f.resets.snapshot().is_empty(), "the link is consumed");
    }

    #[tokio::test]
    async fn following_the_link_revokes_the_sessions_again() {
        let user = alice();
        let f = fixture(vec![user.clone()]);
        let issued = f.reset(user.id).await.unwrap();
        assert_eq!(f.users.token_epoch_of(user.id), 1);

        f.consume
            .execute(&issued.token, "my-new-password")
            .await
            .unwrap();

        assert_eq!(
            f.users.token_epoch_of(user.id),
            2,
            "belt and braces: every password write revokes every session"
        );
    }

    #[tokio::test]
    async fn a_reset_link_works_only_once() {
        let user = alice();
        let f = fixture(vec![user.clone()]);
        let issued = f.reset(user.id).await.unwrap();
        f.consume
            .execute(&issued.token, "my-new-password")
            .await
            .unwrap();

        let second = f.consume.execute(&issued.token, "another-password").await;

        assert!(is_invalid_link(&second), "{second:?}");
        assert!(
            FakeHasher
                .verify(
                    "my-new-password",
                    &f.users.get(user.id).unwrap().password_hash
                )
                .unwrap()
        );
        assert_eq!(
            f.users.token_epoch_of(user.id),
            2,
            "the refused attempt bumped nothing"
        );
    }

    #[tokio::test]
    async fn an_expired_link_is_refused_and_changes_nothing() {
        let user = alice();
        let f = fixture(vec![user.clone()]);
        let issued = f.reset(user.id).await.unwrap();
        let locked = f.users.get(user.id).unwrap().password_hash;
        f.resets.insert(
            user.id,
            &hash_token(&issued.token),
            Utc::now() - Duration::seconds(1),
        );

        let result = f.consume.execute(&issued.token, "my-new-password").await;

        assert!(is_invalid_link(&result), "{result:?}");
        assert_eq!(
            f.users.get(user.id).unwrap().password_hash,
            locked,
            "still locked: the admin issues a new reset"
        );
        assert!(
            f.resets
                .consume(&hash_token(&issued.token))
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn an_unknown_or_malformed_token_is_the_same_generic_error() {
        let user = alice();
        let f = fixture(vec![user.clone()]);
        f.reset(user.id).await.unwrap();
        let locked = f.users.get(user.id).unwrap().password_hash;

        for token in [
            "",
            "deadbeef",
            &"0".repeat(64),
            &"a".repeat(64),
            &"z".repeat(64),
        ] {
            let result = f.consume.execute(token, "my-new-password").await;
            assert!(is_invalid_link(&result), "{token:?}: {result:?}");
        }
        assert_eq!(f.resets.snapshot().len(), 1, "the real link is untouched");
        assert_eq!(f.users.get(user.id).unwrap().password_hash, locked);
    }

    #[tokio::test]
    async fn the_stored_hash_is_not_itself_a_valid_token() {
        let user = alice();
        let f = fixture(vec![user.clone()]);
        f.reset(user.id).await.unwrap();
        let (stored_hash, _) = f.resets.row_of(user.id).unwrap();

        let result = f.consume.execute(&stored_hash, "my-new-password").await;

        assert!(is_invalid_link(&result), "{result:?}");
    }

    #[tokio::test]
    async fn a_weak_password_is_refused_and_the_link_stays_usable() {
        let user = alice();
        let f = fixture(vec![user.clone()]);
        let issued = f.reset(user.id).await.unwrap();
        let locked = f.users.get(user.id).unwrap().password_hash;

        let weak = f.consume.execute(&issued.token, "short").await;

        assert!(
            matches!(&weak, Err(DomainError::Validation(m)) if m == "password must be at least 8 characters"),
            "{weak:?}"
        );
        assert_eq!(
            f.resets.snapshot().len(),
            1,
            "the link must not be consumed by a rejected password"
        );
        assert_eq!(f.users.get(user.id).unwrap().password_hash, locked);
        assert_eq!(
            f.users.token_epoch_of(user.id),
            1,
            "only the admin's own bump so far"
        );
        f.consume
            .execute(&issued.token, "long-enough-password")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn a_failed_password_update_puts_the_link_back_so_the_user_can_retry() {
        let user = alice();
        let f = fixture(vec![user.clone()]);
        let issued = f.reset(user.id).await.unwrap();
        let locked = f.users.get(user.id).unwrap().password_hash;
        let (hash_before, expires_before) = f.resets.row_of(user.id).unwrap();
        f.users.fail_next_password_update();

        let failed = f.consume.execute(&issued.token, "my-new-password").await;

        assert!(
            matches!(&failed, Err(DomainError::Infrastructure(m)) if m == "password update failed"),
            "{failed:?}"
        );
        assert_eq!(
            f.users.get(user.id).unwrap().password_hash,
            locked,
            "the password must be untouched"
        );
        assert_eq!(
            f.resets.row_of(user.id),
            Some((hash_before, expires_before)),
            "same hash and original expiry"
        );

        f.consume
            .execute(&issued.token, "my-new-password")
            .await
            .unwrap();

        assert!(
            FakeHasher
                .verify(
                    "my-new-password",
                    &f.users.get(user.id).unwrap().password_hash
                )
                .unwrap()
        );
        assert!(f.resets.snapshot().is_empty());
    }

    /// An admin issues a new reset right after the user's link is consumed, before the failed password write is
    /// compensated.
    struct AdminResetsMeanwhile {
        inner: Arc<FakePasswordResets>,
        newer_hash: String,
    }

    #[async_trait::async_trait]
    impl PasswordResetPort for AdminResetsMeanwhile {
        async fn replace(
            &self,
            user_id: Uuid,
            token_hash: &str,
            expires_at: DateTime<Utc>,
        ) -> Result<(), DomainError> {
            self.inner.replace(user_id, token_hash, expires_at).await
        }
        async fn consume(&self, token_hash: &str) -> Result<Option<PasswordReset>, DomainError> {
            let consumed = self.inner.consume(token_hash).await?;
            if let Some(reset) = &consumed {
                self.inner.insert(
                    reset.user_id,
                    &self.newer_hash,
                    Utc::now() + Duration::hours(1),
                );
            }
            Ok(consumed)
        }
        async fn restore(
            &self,
            user_id: Uuid,
            token_hash: &str,
            expires_at: DateTime<Utc>,
        ) -> Result<bool, DomainError> {
            self.inner.restore(user_id, token_hash, expires_at).await
        }
        async fn is_pending(&self, user_id: Uuid) -> Result<bool, DomainError> {
            self.inner.is_pending(user_id).await
        }
    }

    #[tokio::test]
    async fn a_failed_password_update_never_revives_a_link_superseded_meanwhile() {
        let user = alice();
        let f = fixture(vec![user.clone()]);
        let issued = f.reset(user.id).await.unwrap();
        let racing = ConsumePasswordResetUseCase::new(
            f.users.clone(),
            Arc::new(FakeHasher),
            Arc::new(AdminResetsMeanwhile {
                inner: f.resets.clone(),
                newer_hash: "newer-hash".to_string(),
            }),
        );
        f.users.fail_next_password_update();

        let failed = racing.execute(&issued.token, "my-new-password").await;

        assert!(
            matches!(failed, Err(DomainError::Infrastructure(_))),
            "{failed:?}"
        );
        assert_eq!(
            f.resets.row_of(user.id).map(|(hash, _)| hash),
            Some("newer-hash".to_string()),
            "the admin's newer link wins"
        );
        assert!(
            is_invalid_link(&f.consume.execute(&issued.token, "my-new-password").await),
            "the older link stays dead"
        );
    }

    #[tokio::test]
    async fn a_malformed_token_is_refused_without_hashing_anything() {
        let hasher = Arc::new(ThreadRecordingHasher::default());
        let consume = ConsumePasswordResetUseCase::new(
            Arc::new(FakeUsers::empty()),
            hasher.clone(),
            Arc::new(FakePasswordResets::new()),
        );

        for token in [
            "",
            "garbage",
            &"z".repeat(64),
            &"a".repeat(65),
            &"a".repeat(100_000),
        ] {
            let result = consume.execute(token, "my-new-password").await;
            assert!(is_invalid_link(&result), "{result:?}");
        }
        assert!(
            hasher.hash_threads().is_empty(),
            "garbage must cost no argon2 run"
        );
    }

    #[tokio::test]
    async fn the_new_password_is_hashed_off_the_async_thread() {
        let user = alice();
        let users = Arc::new(FakeUsers::new(vec![user.clone()]));
        let resets = Arc::new(FakePasswordResets::new());
        let issued = AdminResetPasswordUseCase::new(
            users.clone(),
            Arc::new(FakeHasher),
            Arc::new(FakeInvitations::new()),
            resets.clone(),
        )
        .execute(Uuid::new_v4(), user.id)
        .await
        .unwrap();
        let hasher = Arc::new(ThreadRecordingHasher::default());

        ConsumePasswordResetUseCase::new(users, hasher.clone(), resets)
            .execute(&issued.token, "my-new-password")
            .await
            .unwrap();

        let threads = hasher.hash_threads();
        assert_eq!(threads.len(), 1);
        assert_ne!(threads[0], std::thread::current().id());
    }
}
