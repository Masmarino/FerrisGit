use std::sync::Arc;

use ferrisgit_domain::email::SmtpSettingsPort;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::GroupStorePort;
use ferrisgit_domain::invitation::UserInvitationPort;
use ferrisgit_domain::registration::RegistrationSettingsPort;
use ferrisgit_domain::user::{NewUser, PasswordHasherPort, UserRepositoryPort};

use super::invitations::{InvitedUser, issue_invitation, renew_invitation};
use crate::account_rules::{
    ensure_account_available, generate_invitation_token, hash_blocking, normalize_email,
    normalize_username,
};

/// Self-registration, only while an admin has it switched on. The account starts out like an invited one: its password
/// is unusable until the owner follows the link mailed to the address they gave and picks one, so nobody can register
/// with an address they don't read. Always a plain non-admin account.
pub struct RegisterUserUseCase {
    users: Arc<dyn UserRepositoryPort>,
    hasher: Arc<dyn PasswordHasherPort>,
    groups: Arc<dyn GroupStorePort>,
    registration: Arc<dyn RegistrationSettingsPort>,
    invitations: Arc<dyn UserInvitationPort>,
    smtp: Arc<dyn SmtpSettingsPort>,
}

impl RegisterUserUseCase {
    pub fn new(
        users: Arc<dyn UserRepositoryPort>,
        hasher: Arc<dyn PasswordHasherPort>,
        groups: Arc<dyn GroupStorePort>,
        registration: Arc<dyn RegistrationSettingsPort>,
        invitations: Arc<dyn UserInvitationPort>,
        smtp: Arc<dyn SmtpSettingsPort>,
    ) -> Self {
        Self {
            users,
            hasher,
            groups,
            registration,
            invitations,
            smtp,
        }
    }

    pub async fn execute(
        &self,
        username: String,
        email: String,
    ) -> Result<InvitedUser, DomainError> {
        // First, so a disabled instance answers the same whatever is submitted and nobody can probe usernames.
        if !self.registration.is_enabled().await? {
            return Err(DomainError::Validation(
                "registration is disabled".to_string(),
            ));
        }
        // Without mail nobody would ever receive their link, so there's no point creating an account.
        if self.smtp.get().await?.is_none() {
            return Err(DomainError::ServiceUnavailable(
                "registration needs e-mail to be configured".to_string(),
            ));
        }
        let username = normalize_username(&username)?;
        let email = normalize_email(&email)?;
        if let Some(pending) = self.resend_to_pending(&username, &email).await? {
            return Ok(pending);
        }
        ensure_account_available(&self.users, &self.groups, &username, &email).await?;
        // Hash of a secret that's thrown away, so nothing can log in before activation.
        let password_hash = hash_blocking(&self.hasher, generate_invitation_token()).await?;
        let user = self
            .users
            .create(NewUser {
                username,
                email,
                password_hash,
                is_admin: false,
            })
            .await?;
        let token = issue_invitation(&self.invitations, user.id).await?;
        Ok(InvitedUser { user, token })
    }

    /// The same name and address as a registration nobody has activated yet gets a fresh link instead of a conflict, so
    /// a lost mail doesn't lock people out of their own name. An active account has no pending invitation and falls
    /// through to the usual conflict.
    async fn resend_to_pending(
        &self,
        username: &str,
        email: &str,
    ) -> Result<Option<InvitedUser>, DomainError> {
        let Some(user) = self.users.find_by_username_ignore_case(username).await? else {
            return Ok(None);
        };
        if !user.email.eq_ignore_ascii_case(email) {
            return Ok(None);
        }
        let token = renew_invitation(&self.invitations, user.id).await?;
        Ok(token.map(|token| InvitedUser { user, token }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account_rules::INVITATION_TTL_HOURS;
    use crate::test_support::{
        FakeGroups, FakeHasher, FakeInvitations, FakeRegistration, FakeSmtpSettings, FakeUsers,
    };
    use crate::token_hash::hash_token;
    use crate::use_cases::fixtures::{is_hex64, user};
    use chrono::{Duration, Utc};
    use ferrisgit_domain::group::Group;
    use ferrisgit_domain::user::User;
    use uuid::Uuid;

    struct Fixture {
        users: Arc<FakeUsers>,
        invitations: Arc<FakeInvitations>,
        use_case: RegisterUserUseCase,
    }

    fn fixture(enabled: bool, existing: Vec<User>, groups: Vec<Group>) -> Fixture {
        let users = Arc::new(FakeUsers::new(existing));
        let invitations = Arc::new(FakeInvitations::new());
        let use_case = RegisterUserUseCase::new(
            users.clone(),
            Arc::new(FakeHasher),
            Arc::new(FakeGroups::new(groups)),
            Arc::new(FakeRegistration::new(enabled)),
            invitations.clone(),
            Arc::new(FakeSmtpSettings::configured()),
        );
        Fixture {
            users,
            invitations,
            use_case,
        }
    }

    fn existing(username: &str, email: &str) -> User {
        User {
            email: email.to_string(),
            ..user(username)
        }
    }

    async fn register(
        f: &Fixture,
        username: &str,
        email: &str,
    ) -> Result<InvitedUser, DomainError> {
        f.use_case
            .execute(username.to_string(), email.to_string())
            .await
    }

    /// An account whose registration nobody has activated: it has a pending invitation.
    fn pending(f: &Fixture, user: &User) {
        f.invitations.insert(
            user.id,
            "old-token-hash",
            Utc::now() + Duration::hours(INVITATION_TTL_HOURS),
        );
    }

    #[tokio::test]
    async fn refuses_while_registration_is_disabled_and_creates_nothing() {
        let f = fixture(false, vec![], vec![]);

        let result = register(&f, "alice", "alice@example.com").await;

        assert!(
            matches!(&result, Err(DomainError::Validation(m)) if m == "registration is disabled"),
            "{result:?}"
        );
        assert!(f.users.snapshot().is_empty());
        assert!(f.invitations.snapshot().is_empty());
    }

    #[tokio::test]
    async fn refuses_with_a_503_style_error_when_mail_is_not_configured_and_creates_nothing() {
        let users = Arc::new(FakeUsers::new(vec![]));
        let invitations = Arc::new(FakeInvitations::new());
        let use_case = RegisterUserUseCase::new(
            users.clone(),
            Arc::new(FakeHasher),
            Arc::new(FakeGroups::new(vec![])),
            Arc::new(FakeRegistration::new(true)),
            invitations.clone(),
            Arc::new(FakeSmtpSettings::unconfigured()),
        );

        let result = use_case
            .execute("alice".to_string(), "alice@example.com".to_string())
            .await;

        assert!(
            matches!(&result, Err(DomainError::ServiceUnavailable(m)) if m == "registration needs e-mail to be configured"),
            "{result:?}"
        );
        assert!(users.snapshot().is_empty());
        assert!(invitations.snapshot().is_empty());
    }

    #[tokio::test]
    async fn a_disabled_instance_says_so_even_without_mail() {
        let use_case = RegisterUserUseCase::new(
            Arc::new(FakeUsers::new(vec![])),
            Arc::new(FakeHasher),
            Arc::new(FakeGroups::new(vec![])),
            Arc::new(FakeRegistration::new(false)),
            Arc::new(FakeInvitations::new()),
            Arc::new(FakeSmtpSettings::unconfigured()),
        );

        let result = use_case
            .execute("alice".to_string(), "alice@example.com".to_string())
            .await;

        assert!(
            matches!(&result, Err(DomainError::Validation(m)) if m == "registration is disabled"),
            "{result:?}"
        );
    }

    #[tokio::test]
    async fn creates_an_inactive_non_admin_user_with_a_24_hour_activation_link() {
        let f = fixture(true, vec![], vec![]);

        let registered = register(&f, "  Alice ", " Alice@Example.com ")
            .await
            .unwrap();

        assert_eq!(registered.user.username, "alice");
        assert_eq!(registered.user.email, "Alice@Example.com");
        assert!(!registered.user.is_admin);
        assert_eq!(f.users.snapshot().len(), 1);
        // A hash of a discarded secret: nobody knows a password for the account until it is activated.
        assert!(registered.user.password_hash.starts_with("hashed:"));
        assert_ne!(registered.user.password_hash, "hashed:");
        let (stored_hash, expires_at) = f.invitations.row_of(registered.user.id).unwrap();
        assert_eq!(stored_hash, hash_token(&registered.token));
        assert!(is_hex64(&stored_hash));
        let remaining = expires_at - Utc::now();
        assert!(remaining > Duration::hours(INVITATION_TTL_HOURS - 1));
        assert!(remaining <= Duration::hours(INVITATION_TTL_HOURS));
    }

    #[tokio::test]
    async fn refuses_a_username_taken_in_another_casing() {
        let f = fixture(true, vec![existing("alice", "a@example.com")], vec![]);

        let result = register(&f, "ALICE", "other@example.com").await;

        assert!(
            matches!(&result, Err(DomainError::Conflict(m)) if m == "username already taken"),
            "{result:?}"
        );
        assert_eq!(f.users.snapshot().len(), 1);
    }

    #[tokio::test]
    async fn refuses_an_e_mail_in_use_in_another_casing() {
        let f = fixture(true, vec![existing("alice", "Alice@Example.com")], vec![]);

        let result = register(&f, "bob", "alice@example.COM").await;

        assert!(
            matches!(&result, Err(DomainError::Conflict(m)) if m == "email already in use"),
            "{result:?}"
        );
        assert_eq!(f.users.snapshot().len(), 1);
    }

    #[tokio::test]
    async fn refuses_a_username_colliding_with_a_root_group() {
        let group = Group {
            id: Uuid::new_v4(),
            parent_group_id: None,
            name: "acme".to_string(),
            description: String::new(),
            created_by: Some(Uuid::new_v4()),
            created_at: Utc::now(),
        };
        let f = fixture(true, vec![], vec![group]);

        let result = register(&f, "Acme", "acme@example.com").await;

        assert!(matches!(result, Err(DomainError::Conflict(_))));
        assert!(f.users.snapshot().is_empty());
    }

    #[tokio::test]
    async fn invalid_input_is_a_validation_error_and_creates_nothing() {
        let f = fixture(true, vec![], vec![]);

        for (username, email) in [
            ("ab", "a@example.com"),
            ("admin", "a@example.com"),
            ("x.git", "a@example.com"),
            ("alice", "not-an-email"),
            ("alice", "alice@localhost"),
        ] {
            let result = register(&f, username, email).await;
            assert!(
                matches!(result, Err(DomainError::Validation(_))),
                "{username}/{email}: {result:?}"
            );
        }
        assert!(f.users.snapshot().is_empty());
        assert!(f.invitations.snapshot().is_empty());
    }

    #[tokio::test]
    async fn the_disabled_check_comes_before_any_other_validation() {
        let f = fixture(false, vec![existing("alice", "a@example.com")], vec![]);

        // With the other rules first this would be a conflict, and a disabled instance would leak usernames.
        let result = register(&f, "alice", "a@example.com").await;

        assert!(
            matches!(&result, Err(DomainError::Validation(m)) if m == "registration is disabled"),
            "{result:?}"
        );
    }

    #[tokio::test]
    async fn registering_again_with_the_same_name_and_address_sends_a_fresh_link() {
        let alice = existing("alice", "Alice@Example.com");
        let f = fixture(true, vec![alice.clone()], vec![]);
        pending(&f, &alice);

        let again = register(&f, "ALICE", "alice@example.com").await.unwrap();

        assert_eq!(again.user.id, alice.id);
        assert_eq!(f.users.snapshot().len(), 1, "no second account");
        let (stored_hash, _) = f.invitations.row_of(alice.id).unwrap();
        assert_eq!(stored_hash, hash_token(&again.token));
        assert_ne!(
            stored_hash, "old-token-hash",
            "the previous link stops working"
        );
    }

    #[tokio::test]
    async fn a_pending_name_with_another_address_is_still_a_conflict() {
        let alice = existing("alice", "alice@example.com");
        let f = fixture(true, vec![alice.clone()], vec![]);
        pending(&f, &alice);

        let result = register(&f, "alice", "someone-else@example.com").await;

        assert!(
            matches!(&result, Err(DomainError::Conflict(m)) if m == "username already taken"),
            "{result:?}"
        );
        let (stored_hash, _) = f.invitations.row_of(alice.id).unwrap();
        assert_eq!(
            stored_hash, "old-token-hash",
            "the owner's link is untouched"
        );
    }

    #[tokio::test]
    async fn a_pending_address_under_another_name_is_still_a_conflict() {
        let alice = existing("alice", "alice@example.com");
        let f = fixture(true, vec![alice.clone()], vec![]);
        pending(&f, &alice);

        let result = register(&f, "mallory", "alice@example.com").await;

        assert!(
            matches!(&result, Err(DomainError::Conflict(m)) if m == "email already in use"),
            "{result:?}"
        );
        assert_eq!(f.users.snapshot().len(), 1);
    }

    #[tokio::test]
    async fn an_active_account_with_the_same_name_and_address_is_a_conflict_not_a_resend() {
        // No pending invitation: the account was activated.
        let f = fixture(true, vec![existing("alice", "alice@example.com")], vec![]);

        let result = register(&f, "alice", "alice@example.com").await;

        assert!(
            matches!(&result, Err(DomainError::Conflict(m)) if m == "username already taken"),
            "{result:?}"
        );
        assert!(
            f.invitations.snapshot().is_empty(),
            "no link for an active account"
        );
    }

    #[tokio::test]
    async fn the_placeholder_password_is_hashed_off_the_async_thread() {
        let hasher = Arc::new(crate::test_support::ThreadRecordingHasher::default());
        let use_case = RegisterUserUseCase::new(
            Arc::new(FakeUsers::new(vec![])),
            hasher.clone(),
            Arc::new(FakeGroups::new(vec![])),
            Arc::new(FakeRegistration::new(true)),
            Arc::new(FakeInvitations::new()),
            Arc::new(FakeSmtpSettings::configured()),
        );

        use_case
            .execute("alice".to_string(), "alice@example.com".to_string())
            .await
            .unwrap();

        let threads = hasher.hash_threads();
        assert_eq!(threads.len(), 1);
        assert_ne!(threads[0], std::thread::current().id());
    }
}
