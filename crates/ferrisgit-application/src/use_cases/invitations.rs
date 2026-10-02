use std::sync::Arc;

use chrono::{Duration, Utc};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::GroupStorePort;
use ferrisgit_domain::invitation::UserInvitationPort;
use ferrisgit_domain::user::{NewUser, PasswordHasherPort, User, UserRepositoryPort};
use uuid::Uuid;

use crate::account_rules::{
    INVITATION_TTL_HOURS, ensure_account_available, generate_invitation_token, hash_blocking,
    is_invitation_token_shaped, normalize_email, normalize_username, validate_password,
};
use crate::token_hash::hash_token;

/// One error for unknown, expired, used or malformed tokens: telling them apart would only help someone probing
/// links.
const INVALID_INVITATION: &str = "invalid or expired invitation";

/// A freshly invited (or re-invited) user and the plaintext activation token. The token exists only to build the
/// activation URL: it is not stored (only its hash is) and is redacted from `Debug`.
#[derive(Clone)]
pub struct InvitedUser {
    pub user: User,
    pub token: String,
}

impl std::fmt::Debug for InvitedUser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InvitedUser")
            .field("user", &self.user)
            .field("token", &"[redacted]")
            .finish()
    }
}

/// Stores a new activation token for the user (replacing any previous one) and returns its plaintext.
async fn issue_invitation(
    invitations: &Arc<dyn UserInvitationPort>,
    user_id: Uuid,
) -> Result<String, DomainError> {
    let token = generate_invitation_token();
    invitations
        .replace(
            user_id,
            &hash_token(&token),
            Utc::now() + Duration::hours(INVITATION_TTL_HOURS),
        )
        .await?;
    Ok(token)
}

/// An admin creates an account for someone else. The account has an unusable password (a hash of a random secret
/// nobody knows) until the invitee activates it through the link.
pub struct InviteUserUseCase {
    users: Arc<dyn UserRepositoryPort>,
    hasher: Arc<dyn PasswordHasherPort>,
    groups: Arc<dyn GroupStorePort>,
    invitations: Arc<dyn UserInvitationPort>,
}

impl InviteUserUseCase {
    pub fn new(
        users: Arc<dyn UserRepositoryPort>,
        hasher: Arc<dyn PasswordHasherPort>,
        groups: Arc<dyn GroupStorePort>,
        invitations: Arc<dyn UserInvitationPort>,
    ) -> Self {
        Self {
            users,
            hasher,
            groups,
            invitations,
        }
    }

    pub async fn execute(
        &self,
        username: String,
        email: String,
        is_admin: bool,
    ) -> Result<InvitedUser, DomainError> {
        let username = normalize_username(&username)?;
        let email = normalize_email(&email)?;
        ensure_account_available(&self.users, &self.groups, &username, &email).await?;
        // A hash of a secret that is generated here and thrown away: nothing verifies against it until activation.
        let password_hash = hash_blocking(&self.hasher, generate_invitation_token()).await?;
        let user = self
            .users
            .create(NewUser {
                username,
                email,
                password_hash,
                is_admin,
            })
            .await?;
        let token = issue_invitation(&self.invitations, user.id).await?;
        Ok(InvitedUser { user, token })
    }
}

/// Issues a new activation link for a user who has not activated yet; the previous link stops working.
pub struct ResendInvitationUseCase {
    users: Arc<dyn UserRepositoryPort>,
    invitations: Arc<dyn UserInvitationPort>,
}

impl ResendInvitationUseCase {
    pub fn new(
        users: Arc<dyn UserRepositoryPort>,
        invitations: Arc<dyn UserInvitationPort>,
    ) -> Self {
        Self { users, invitations }
    }

    pub async fn execute(&self, user_id: Uuid) -> Result<InvitedUser, DomainError> {
        let user = self
            .users
            .find_by_id(user_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("user".to_string()))?;
        // An invitation row (expired or not) is what marks the account as not yet activated. The write only happens if
        // that row still exists (`renew`, never an upsert). If an activation consumed it between the lookup and this
        // write, the now active account must not get a new invitation.
        let token = generate_invitation_token();
        let expires_at = Utc::now() + Duration::hours(INVITATION_TTL_HOURS);
        if !self
            .invitations
            .renew(user_id, &hash_token(&token), expires_at)
            .await?
        {
            return Err(DomainError::Validation(
                "user is already active".to_string(),
            ));
        }
        Ok(InvitedUser { user, token })
    }
}

/// The invitee follows the link and chooses a password. No session is issued: they log in afterwards and are sent
/// through MFA enrolment like everybody else.
pub struct ActivateAccountUseCase {
    users: Arc<dyn UserRepositoryPort>,
    hasher: Arc<dyn PasswordHasherPort>,
    invitations: Arc<dyn UserInvitationPort>,
}

impl ActivateAccountUseCase {
    pub fn new(
        users: Arc<dyn UserRepositoryPort>,
        hasher: Arc<dyn PasswordHasherPort>,
        invitations: Arc<dyn UserInvitationPort>,
    ) -> Self {
        Self {
            users,
            hasher,
            invitations,
        }
    }

    pub async fn execute(&self, token: &str, password: &str) -> Result<Uuid, DomainError> {
        // Hash before consuming: a rejected password or hasher failure must not burn the invitation. Garbage is
        // refused before anything expensive with the generic error.
        if !is_invitation_token_shaped(token) {
            return Err(DomainError::Validation(INVALID_INVITATION.to_string()));
        }
        validate_password(password)?;
        let password_hash = hash_blocking(&self.hasher, password.to_string()).await?;
        let token_hash = hash_token(token);
        let invitation = self
            .invitations
            .consume(&token_hash)
            .await?
            .ok_or_else(|| DomainError::Validation(INVALID_INVITATION.to_string()))?;
        if let Err(error) = self
            .users
            .update_password_hash(invitation.user_id, password_hash)
            .await
        {
            // Best-effort compensation: do not strand the account (no link, and Resend would call it "already
            // active"). The same link stays usable with its original expiry so the user can simply retry.
            let _ = self
                .invitations
                .replace(invitation.user_id, &token_hash, invitation.expires_at)
                .await;
            return Err(error);
        }
        Ok(invitation.user_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeGroups, FakeHasher, FakeInvitations, FakeUsers};
    use crate::use_cases::fixtures::{group, is_hex64, user};
    use chrono::DateTime;
    use ferrisgit_domain::group::Group;

    struct Fixture {
        users: Arc<FakeUsers>,
        invitations: Arc<FakeInvitations>,
        invite: InviteUserUseCase,
        resend: ResendInvitationUseCase,
        activate: ActivateAccountUseCase,
    }

    fn fixture(existing: Vec<User>, groups: Vec<Group>) -> Fixture {
        let users = Arc::new(FakeUsers::new(existing));
        let invitations = Arc::new(FakeInvitations::new());
        Fixture {
            invite: InviteUserUseCase::new(
                users.clone(),
                Arc::new(FakeHasher),
                Arc::new(FakeGroups::new(groups)),
                invitations.clone(),
            ),
            resend: ResendInvitationUseCase::new(users.clone(), invitations.clone()),
            activate: ActivateAccountUseCase::new(
                users.clone(),
                Arc::new(FakeHasher),
                invitations.clone(),
            ),
            users,
            invitations,
        }
    }

    fn existing(username: &str, email: &str) -> User {
        User {
            email: email.to_string(),
            ..user(username)
        }
    }

    async fn invite(
        f: &Fixture,
        username: &str,
        email: &str,
        is_admin: bool,
    ) -> Result<InvitedUser, DomainError> {
        f.invite
            .execute(username.to_string(), email.to_string(), is_admin)
            .await
    }

    #[tokio::test]
    async fn inviting_creates_the_user_and_returns_a_64_hex_token() {
        let f = fixture(vec![], vec![]);

        let invited = invite(&f, "  Bob ", "Bob@Example.com", false)
            .await
            .unwrap();

        assert_eq!(invited.user.username, "bob");
        assert_eq!(invited.user.email, "Bob@Example.com");
        assert!(!invited.user.is_admin);
        assert!(is_hex64(&invited.token), "{}", invited.token);
        assert_eq!(f.users.snapshot().len(), 1);
    }

    #[tokio::test]
    async fn the_admin_flag_is_respected() {
        let f = fixture(vec![], vec![]);

        let invited = invite(&f, "root2", "root2@example.com", true)
            .await
            .unwrap();

        assert!(invited.user.is_admin);
        assert!(f.users.get(invited.user.id).unwrap().is_admin);
    }

    #[tokio::test]
    async fn the_invited_user_has_an_unusable_random_password() {
        let f = fixture(vec![], vec![]);

        let first = invite(&f, "bob", "bob@example.com", false).await.unwrap();
        let second = invite(&f, "carol", "carol@example.com", false)
            .await
            .unwrap();

        assert_ne!(first.user.password_hash, second.user.password_hash);
        let hasher = FakeHasher;
        for (user, other_token) in [(&first.user, &second.token), (&second.user, &first.token)] {
            for guess in [
                "",
                user.username.as_str(),
                "password",
                "password12345",
                other_token.as_str(),
            ] {
                assert!(
                    !hasher.verify(guess, &user.password_hash).unwrap(),
                    "{guess:?} must not verify"
                );
            }
        }
    }

    #[tokio::test]
    async fn the_password_secret_is_not_the_activation_token() {
        let f = fixture(vec![], vec![]);

        let invited = invite(&f, "bob", "bob@example.com", false).await.unwrap();

        assert!(
            !FakeHasher
                .verify(&invited.token, &invited.user.password_hash)
                .unwrap()
        );
    }

    #[tokio::test]
    async fn only_the_hash_of_the_token_is_stored_and_it_expires_in_24_hours() {
        let f = fixture(vec![], vec![]);

        let invited = invite(&f, "bob", "bob@example.com", false).await.unwrap();

        let (stored_hash, expires_at) = f.invitations.row_of(invited.user.id).unwrap();
        assert_eq!(stored_hash, hash_token(&invited.token));
        assert_ne!(stored_hash, invited.token);
        let expected = Utc::now() + Duration::hours(24);
        assert!(
            (expires_at - expected).num_seconds().abs() < 5,
            "expires_at {expires_at} vs {expected}"
        );
    }

    #[tokio::test]
    async fn two_invitations_get_different_tokens() {
        let f = fixture(vec![], vec![]);

        let first = invite(&f, "bob", "bob@example.com", false).await.unwrap();
        let second = invite(&f, "carol", "carol@example.com", false)
            .await
            .unwrap();

        assert_ne!(first.token, second.token);
    }

    #[tokio::test]
    async fn inviting_refuses_duplicates_in_any_casing_and_root_group_names() {
        let f = fixture(
            vec![existing("alice", "Alice@Example.com")],
            vec![group(None, "acme")],
        );

        for (username, email) in [
            ("ALICE", "new@example.com"),
            ("bob", "alice@example.com"),
            ("acme", "acme@example.com"),
        ] {
            let result = invite(&f, username, email, false).await;
            assert!(
                matches!(result, Err(DomainError::Conflict(_))),
                "{username}/{email}: {result:?}"
            );
        }
        assert_eq!(f.users.snapshot().len(), 1);
        assert!(f.invitations.snapshot().is_empty());
    }

    #[tokio::test]
    async fn inviting_applies_the_account_rules_and_creates_nothing_on_failure() {
        let f = fixture(vec![], vec![]);

        for (username, email) in [
            ("ab", "a@example.com"),
            ("Admin", "a@example.com"),
            ("x.git", "a@example.com"),
            ("bob", "nope"),
            ("bob", "bob@localhost"),
        ] {
            let result = invite(&f, username, email, false).await;
            assert!(
                matches!(result, Err(DomainError::Validation(_))),
                "{username}/{email}: {result:?}"
            );
        }
        assert!(f.users.snapshot().is_empty());
        assert!(f.invitations.snapshot().is_empty());
    }

    #[test]
    fn an_invited_user_debug_output_never_contains_the_token() {
        let invited = InvitedUser {
            user: existing("bob", "bob@example.com"),
            token: "SECRETTOKENVALUE".to_string(),
        };

        let printed = format!("{invited:?}");

        assert!(!printed.contains("SECRETTOKENVALUE"));
        assert!(printed.contains("[redacted]"));
    }

    #[tokio::test]
    async fn resending_replaces_the_previous_token() {
        let f = fixture(vec![], vec![]);
        let first = invite(&f, "bob", "bob@example.com", false).await.unwrap();

        let second = f.resend.execute(first.user.id).await.unwrap();

        assert_eq!(second.user.id, first.user.id);
        assert_ne!(second.token, first.token);
        assert!(is_hex64(&second.token));
        assert_eq!(f.invitations.snapshot().len(), 1);
        assert!(
            f.invitations
                .consume(&hash_token(&first.token))
                .await
                .unwrap()
                .is_none(),
            "the old token must be dead"
        );
        assert!(
            f.invitations
                .consume(&hash_token(&second.token))
                .await
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    async fn resending_renews_an_expired_invitation() {
        let f = fixture(vec![], vec![]);
        let first = invite(&f, "bob", "bob@example.com", false).await.unwrap();
        f.invitations.insert(
            first.user.id,
            &hash_token(&first.token),
            Utc::now() - Duration::hours(1),
        );

        let second = f.resend.execute(first.user.id).await.unwrap();

        let (_, expires_at) = f.invitations.row_of(first.user.id).unwrap();
        assert!(
            (expires_at - (Utc::now() + Duration::hours(24)))
                .num_seconds()
                .abs()
                < 5
        );
        assert!(
            f.invitations
                .consume(&hash_token(&second.token))
                .await
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    async fn resending_for_an_unknown_user_is_not_found() {
        let f = fixture(vec![], vec![]);

        let result = f.resend.execute(Uuid::new_v4()).await;

        assert!(
            matches!(result, Err(DomainError::NotFound(_))),
            "{result:?}"
        );
    }

    #[tokio::test]
    async fn resending_for_an_already_active_user_is_a_validation_error() {
        let active = existing("alice", "alice@example.com");
        let f = fixture(vec![active.clone()], vec![]);

        let result = f.resend.execute(active.id).await;

        assert!(
            matches!(&result, Err(DomainError::Validation(m)) if m == "user is already active"),
            "{result:?}"
        );
        assert!(f.invitations.snapshot().is_empty());
    }

    /// Wraps the fake and lets an activation win the race: right before the resend's write, the invitation is consumed
    /// (the user activated between the resend's lookup and its write).
    struct ActivationWinsTheRace(Arc<FakeInvitations>);

    #[async_trait::async_trait]
    impl UserInvitationPort for ActivationWinsTheRace {
        async fn replace(
            &self,
            user_id: Uuid,
            token_hash: &str,
            expires_at: DateTime<Utc>,
        ) -> Result<(), DomainError> {
            self.0.remove(user_id);
            self.0.replace(user_id, token_hash, expires_at).await
        }
        async fn renew(
            &self,
            user_id: Uuid,
            token_hash: &str,
            expires_at: DateTime<Utc>,
        ) -> Result<bool, DomainError> {
            self.0.remove(user_id);
            self.0.renew(user_id, token_hash, expires_at).await
        }
        async fn consume(
            &self,
            token_hash: &str,
        ) -> Result<Option<ferrisgit_domain::invitation::Invitation>, DomainError> {
            self.0.consume(token_hash).await
        }
        async fn expiries(
            &self,
            user_ids: &[Uuid],
        ) -> Result<Vec<(Uuid, DateTime<Utc>)>, DomainError> {
            self.0.expiries(user_ids).await
        }
    }

    #[tokio::test]
    async fn a_resend_racing_an_activation_does_not_resurrect_an_invitation() {
        let f = fixture(vec![], vec![]);
        let first = invite(&f, "bob", "bob@example.com", false).await.unwrap();
        let racing = ResendInvitationUseCase::new(
            f.users.clone(),
            Arc::new(ActivationWinsTheRace(f.invitations.clone())),
        );

        let result = racing.execute(first.user.id).await;

        assert!(
            matches!(&result, Err(DomainError::Validation(m)) if m == "user is already active"),
            "{result:?}"
        );
        assert!(
            f.invitations.snapshot().is_empty(),
            "no invitation may exist for an account that just activated"
        );
    }

    #[tokio::test]
    async fn activating_sets_the_chosen_password_and_returns_the_user_id() {
        let f = fixture(vec![], vec![]);
        let invited = invite(&f, "bob", "bob@example.com", false).await.unwrap();
        let random_hash = invited.user.password_hash.clone();

        let user_id = f
            .activate
            .execute(&invited.token, "my-new-password")
            .await
            .unwrap();

        assert_eq!(user_id, invited.user.id);
        let stored = f.users.get(user_id).unwrap();
        assert!(
            FakeHasher
                .verify("my-new-password", &stored.password_hash)
                .unwrap()
        );
        assert_ne!(stored.password_hash, random_hash);
    }

    #[tokio::test]
    async fn an_activation_token_works_only_once() {
        let f = fixture(vec![], vec![]);
        let invited = invite(&f, "bob", "bob@example.com", false).await.unwrap();
        f.activate
            .execute(&invited.token, "my-new-password")
            .await
            .unwrap();

        let second = f.activate.execute(&invited.token, "another-password").await;

        assert!(
            matches!(&second, Err(DomainError::Validation(m)) if m == INVALID_INVITATION),
            "{second:?}"
        );
        assert!(
            FakeHasher
                .verify(
                    "my-new-password",
                    &f.users.get(invited.user.id).unwrap().password_hash
                )
                .unwrap()
        );
        assert!(f.invitations.snapshot().is_empty());
    }

    #[tokio::test]
    async fn an_expired_token_is_refused_and_never_consumable() {
        let f = fixture(vec![], vec![]);
        let invited = invite(&f, "bob", "bob@example.com", false).await.unwrap();
        f.invitations.insert(
            invited.user.id,
            &hash_token(&invited.token),
            Utc::now() - Duration::seconds(1),
        );

        let result = f.activate.execute(&invited.token, "my-new-password").await;

        assert!(
            matches!(&result, Err(DomainError::Validation(m)) if m == INVALID_INVITATION),
            "{result:?}"
        );
        assert_eq!(
            f.users.get(invited.user.id).unwrap().password_hash,
            invited.user.password_hash
        );
        assert!(
            f.invitations
                .consume(&hash_token(&invited.token))
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn an_unknown_or_empty_token_is_refused() {
        let f = fixture(vec![], vec![]);
        invite(&f, "bob", "bob@example.com", false).await.unwrap();

        for token in ["", "deadbeef", &"0".repeat(64)] {
            let result = f.activate.execute(token, "my-new-password").await;
            assert!(
                matches!(&result, Err(DomainError::Validation(m)) if m == INVALID_INVITATION),
                "{token:?}: {result:?}"
            );
        }
        assert_eq!(f.invitations.snapshot().len(), 1);
    }

    #[tokio::test]
    async fn the_stored_hash_is_not_itself_a_valid_token() {
        let f = fixture(vec![], vec![]);
        let invited = invite(&f, "bob", "bob@example.com", false).await.unwrap();
        let (stored_hash, _) = f.invitations.row_of(invited.user.id).unwrap();

        let result = f.activate.execute(&stored_hash, "my-new-password").await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn a_failed_password_update_puts_the_invitation_back_so_the_user_can_retry() {
        let f = fixture(vec![], vec![]);
        let invited = invite(&f, "bob", "bob@example.com", false).await.unwrap();
        let (hash_before, expires_before) = f.invitations.row_of(invited.user.id).unwrap();
        f.users.fail_next_password_update();

        let failed = f.activate.execute(&invited.token, "my-new-password").await;

        assert!(
            matches!(&failed, Err(DomainError::Infrastructure(m)) if m == "password update failed"),
            "{failed:?}"
        );
        assert_eq!(
            f.users.get(invited.user.id).unwrap().password_hash,
            invited.user.password_hash,
            "the password must be untouched"
        );
        assert_eq!(
            f.invitations.row_of(invited.user.id),
            Some((hash_before, expires_before)),
            "same hash and original expiry"
        );

        let retried = f
            .activate
            .execute(&invited.token, "my-new-password")
            .await
            .unwrap();

        assert_eq!(retried, invited.user.id);
        assert!(
            FakeHasher
                .verify(
                    "my-new-password",
                    &f.users.get(invited.user.id).unwrap().password_hash
                )
                .unwrap()
        );
        assert!(f.invitations.snapshot().is_empty());
    }

    #[tokio::test]
    async fn a_weak_password_is_refused_and_the_invitation_stays_usable() {
        let f = fixture(vec![], vec![]);
        let invited = invite(&f, "bob", "bob@example.com", false).await.unwrap();

        let weak = f.activate.execute(&invited.token, "short").await;

        assert!(
            matches!(&weak, Err(DomainError::Validation(m)) if m == "password must be at least 8 characters"),
            "{weak:?}"
        );
        assert_eq!(
            f.invitations.snapshot().len(),
            1,
            "the invitation must not be consumed by a rejected password"
        );
        assert!(
            f.activate
                .execute(&invited.token, "long-enough-password")
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn invited_users_random_password_hash_and_the_activation_hash_run_off_the_async_thread() {
        let hasher = Arc::new(crate::test_support::ThreadRecordingHasher::default());
        let users = Arc::new(FakeUsers::new(vec![]));
        let invitations = Arc::new(FakeInvitations::new());
        let invite = InviteUserUseCase::new(
            users.clone(),
            hasher.clone(),
            Arc::new(FakeGroups::new(vec![])),
            invitations.clone(),
        );
        let activate = ActivateAccountUseCase::new(users, hasher.clone(), invitations);

        let invited = invite
            .execute("bob".to_string(), "bob@example.com".to_string(), false)
            .await
            .unwrap();
        activate
            .execute(&invited.token, "my-new-password")
            .await
            .unwrap();

        let threads = hasher.hash_threads();
        assert_eq!(
            threads.len(),
            2,
            "one hash for the unusable password, one for the chosen password"
        );
        assert!(threads.iter().all(|t| *t != std::thread::current().id()));
    }

    #[tokio::test]
    async fn a_malformed_token_is_refused_generically_without_hashing_anything() {
        let hasher = Arc::new(crate::test_support::ThreadRecordingHasher::default());
        let users = Arc::new(FakeUsers::new(vec![]));
        let activate =
            ActivateAccountUseCase::new(users, hasher.clone(), Arc::new(FakeInvitations::new()));

        for token in [
            "",
            "garbage",
            &"z".repeat(64),
            &"a".repeat(65),
            &"a".repeat(100_000),
        ] {
            let result = activate.execute(token, "my-new-password").await;
            assert!(
                matches!(&result, Err(DomainError::Validation(m)) if m == INVALID_INVITATION),
                "{result:?}"
            );
        }
        assert!(
            hasher.hash_threads().is_empty(),
            "garbage must cost no argon2 run"
        );
    }

    #[tokio::test]
    async fn a_well_formed_unknown_token_is_the_same_generic_error() {
        let f = fixture(vec![], vec![]);

        let result = f.activate.execute(&"a".repeat(64), "my-new-password").await;

        assert!(
            matches!(&result, Err(DomainError::Validation(m)) if m == INVALID_INVITATION),
            "{result:?}"
        );
    }
}
