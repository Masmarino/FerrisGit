use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::invitation::UserInvitationPort;
use ferrisgit_domain::mfa::TotpCredentialPort;
use ferrisgit_domain::user::{User, UserRepositoryPort};
use ferrisgit_domain::webauthn::WebauthnCredentialPort;
use uuid::Uuid;

/// Instance-scale data: everyone in one page, hard-capped so a runaway table can never turn into a huge response.
const LIST_LIMIT: i64 = 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserState {
    Active,
    /// Invited and not activated yet. An expired invitation stays here until it is resent or activated.
    Invited {
        expires_at: DateTime<Utc>,
    },
}

#[derive(Debug, Clone)]
pub struct UserListing {
    pub user: User,
    pub state: UserState,
    /// A confirmed TOTP credential exists (an enrolment that was started but never confirmed does not count), or the
    /// user has at least one passkey.
    pub mfa_enabled: bool,
}

/// The admin Users page: every account with its activation state and MFA status, in creation order.
pub struct ListUsersUseCase {
    users: Arc<dyn UserRepositoryPort>,
    invitations: Arc<dyn UserInvitationPort>,
    totp: Arc<dyn TotpCredentialPort>,
    passkeys: Arc<dyn WebauthnCredentialPort>,
}

impl ListUsersUseCase {
    pub fn new(
        users: Arc<dyn UserRepositoryPort>,
        invitations: Arc<dyn UserInvitationPort>,
        totp: Arc<dyn TotpCredentialPort>,
        passkeys: Arc<dyn WebauthnCredentialPort>,
    ) -> Self {
        Self {
            users,
            invitations,
            totp,
            passkeys,
        }
    }

    pub async fn execute(&self) -> Result<Vec<UserListing>, DomainError> {
        let users = self.users.list(LIST_LIMIT).await?;
        let ids: Vec<Uuid> = users.iter().map(|u| u.id).collect();
        let expiries: HashMap<Uuid, DateTime<Utc>> =
            self.invitations.expiries(&ids).await?.into_iter().collect();
        // Two batched queries for the whole page (no per-row work): a confirmed TOTP, or at least one passkey.
        let mut mfa_enabled: HashSet<Uuid> = self
            .totp
            .confirmed_user_ids(&ids)
            .await?
            .into_iter()
            .collect();
        mfa_enabled.extend(self.passkeys.user_ids_with_passkeys(&ids).await?);
        Ok(users
            .into_iter()
            .map(|user| {
                let state = match expiries.get(&user.id) {
                    Some(expires_at) => UserState::Invited {
                        expires_at: *expires_at,
                    },
                    None => UserState::Active,
                };
                let mfa_enabled = mfa_enabled.contains(&user.id);
                UserListing {
                    user,
                    state,
                    mfa_enabled,
                }
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeInvitations, FakePasskeys, FakeTotp, FakeUsers};
    use chrono::Duration;
    use ferrisgit_domain::mfa::TotpCredential;

    struct Fixture {
        invitations: Arc<FakeInvitations>,
        totp: Arc<FakeTotp>,
        passkeys: Arc<FakePasskeys>,
        use_case: ListUsersUseCase,
    }

    fn user(name: &str, created_at: DateTime<Utc>) -> User {
        User {
            id: Uuid::new_v4(),
            username: name.to_string(),
            email: format!("{name}@example.com"),
            password_hash: "h".to_string(),
            is_admin: false,
            created_at,
        }
    }

    fn fixture(users: Vec<User>) -> Fixture {
        let invitations = Arc::new(FakeInvitations::new());
        let totp = Arc::new(FakeTotp::new());
        let passkeys = Arc::new(FakePasskeys::new());
        let use_case = ListUsersUseCase::new(
            Arc::new(FakeUsers::new(users)),
            invitations.clone(),
            totp.clone(),
            passkeys.clone(),
        );
        Fixture {
            invitations,
            totp,
            passkeys,
            use_case,
        }
    }

    async fn enrol(f: &Fixture, user_id: Uuid, confirmed: bool) {
        f.totp
            .upsert(&TotpCredential {
                user_id,
                secret: "SECRET".to_string(),
                confirmed,
                last_used_step: None,
                created_at: Utc::now(),
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn active_and_invited_users_are_told_apart_with_the_invitation_expiry() {
        let now = Utc::now();
        let active = user("alice", now - Duration::days(3));
        let invited = user("bob", now - Duration::days(2));
        let expired = user("carol", now - Duration::days(1));
        let f = fixture(vec![active.clone(), invited.clone(), expired.clone()]);
        let expires_at = now + Duration::hours(5);
        f.invitations.insert(invited.id, "h1", expires_at);
        let long_ago = now - Duration::hours(2);
        f.invitations.insert(expired.id, "h2", long_ago);

        let listing = f.use_case.execute().await.unwrap();

        assert_eq!(
            listing.iter().map(|l| l.state.clone()).collect::<Vec<_>>(),
            vec![
                UserState::Active,
                UserState::Invited { expires_at },
                UserState::Invited {
                    expires_at: long_ago
                }
            ]
        );
    }

    #[tokio::test]
    async fn mfa_is_enabled_only_for_a_confirmed_credential() {
        let now = Utc::now();
        let confirmed = user("alice", now - Duration::days(3));
        let pending = user("bob", now - Duration::days(2));
        let none = user("carol", now - Duration::days(1));
        let f = fixture(vec![confirmed.clone(), pending.clone(), none.clone()]);
        enrol(&f, confirmed.id, true).await;
        enrol(&f, pending.id, false).await;

        let listing = f.use_case.execute().await.unwrap();

        assert_eq!(
            listing
                .iter()
                .map(|l| (l.user.username.as_str(), l.mfa_enabled))
                .collect::<Vec<_>>(),
            vec![("alice", true), ("bob", false), ("carol", false)]
        );
    }

    #[tokio::test]
    async fn a_passkey_alone_enables_mfa_and_the_two_factors_are_united() {
        let now = Utc::now();
        let totp_only = user("alice", now - Duration::days(4));
        let passkey_only = user("bob", now - Duration::days(3));
        let both = user("carol", now - Duration::days(2));
        let unconfirmed_totp = user("dave", now - Duration::days(1));
        let none = user("erin", now);
        let f = fixture(vec![
            totp_only.clone(),
            passkey_only.clone(),
            both.clone(),
            unconfirmed_totp.clone(),
            none.clone(),
        ]);
        enrol(&f, totp_only.id, true).await;
        enrol(&f, both.id, true).await;
        enrol(&f, unconfirmed_totp.id, false).await;
        f.passkeys.seed(passkey_only.id, "Clé");
        f.passkeys.seed(both.id, "Clé");
        f.passkeys.seed(both.id, "Autre clé");

        let listing = f.use_case.execute().await.unwrap();

        assert_eq!(
            listing
                .iter()
                .map(|l| (l.user.username.as_str(), l.mfa_enabled))
                .collect::<Vec<_>>(),
            vec![
                ("alice", true),
                ("bob", true),
                ("carol", true),
                ("dave", false),
                ("erin", false)
            ]
        );
    }

    #[tokio::test]
    async fn users_come_back_in_creation_order_whatever_the_storage_order() {
        let now = Utc::now();
        let f = fixture(vec![
            user("third", now),
            user("first", now - Duration::days(2)),
            user("second", now - Duration::days(1)),
        ]);

        let listing = f.use_case.execute().await.unwrap();

        assert_eq!(
            listing
                .iter()
                .map(|l| l.user.username.as_str())
                .collect::<Vec<_>>(),
            vec!["first", "second", "third"]
        );
    }

    #[tokio::test]
    async fn the_list_is_capped_at_1000_users() {
        let now = Utc::now();
        let users = (0..1005)
            .map(|i| user(&format!("user{i}"), now + Duration::seconds(i)))
            .collect();
        let f = fixture(users);

        let listing = f.use_case.execute().await.unwrap();

        assert_eq!(listing.len(), 1000);
        assert_eq!(listing[0].user.username, "user0");
    }

    #[tokio::test]
    async fn an_empty_instance_lists_nobody() {
        let f = fixture(vec![]);

        assert!(f.use_case.execute().await.unwrap().is_empty());
    }
}
