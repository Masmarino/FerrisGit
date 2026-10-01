use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::invitation::UserInvitationPort;
use ferrisgit_domain::password_reset::PasswordResetPort;
use ferrisgit_domain::user::{User, UserRepositoryPort};
use uuid::Uuid;

/// Grants or removes the admin flag, including one's own. The floor: the instance never ends up without an active
/// administrator, since there is no bootstrap once accounts exist. An admin with a pending invitation or password reset
/// does not count: they cannot sign in and the link may lapse. Same reasoning as `group_maintainer_guard`, at instance
/// scope.
///
/// Self-demotion is not special-cased. The flag is read from the database on every admin request (`AdminUser`), so a
/// demotion takes effect on the next request without revoking sessions.
pub struct SetAdminUseCase {
    users: Arc<dyn UserRepositoryPort>,
    invitations: Arc<dyn UserInvitationPort>,
    password_resets: Arc<dyn PasswordResetPort>,
}

impl SetAdminUseCase {
    pub fn new(
        users: Arc<dyn UserRepositoryPort>,
        invitations: Arc<dyn UserInvitationPort>,
        password_resets: Arc<dyn PasswordResetPort>,
    ) -> Self {
        Self {
            users,
            invitations,
            password_resets,
        }
    }

    /// `true` only if the flag actually changed: promoting an admin or demoting a non-admin succeeds without writing
    /// anything and returns `false`, so the caller audits real changes only. `NotFound` for an unknown user, `Conflict`
    /// for the demotion of the last active admin.
    pub async fn execute(&self, target_user_id: Uuid, is_admin: bool) -> Result<bool, DomainError> {
        let target = self
            .users
            .find_by_id(target_user_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("user".to_string()))?;
        if target.is_admin == is_admin {
            return Ok(false);
        }
        // The clear, early refusal. It is a check-then-act on its own, so `set_admin` enforces the same floor again,
        // atomically, in the store (two admins demoting each other at once must not both pass this check).
        if !is_admin {
            ensure_not_last_active_admin(
                self.users.as_ref(),
                self.invitations.as_ref(),
                self.password_resets.as_ref(),
                &target,
            )
            .await?;
        }
        self.users.set_admin(target_user_id, is_admin).await?;
        Ok(true)
    }
}

/// The floor's early refusal shared by every operation removing an admin (demotion, `DeleteUserUseCase`): `Conflict`
/// when `target` is an active admin and no other remains. Same definition of active as `count_admins`, or this check
/// would refuse what the store allows. The store enforces it again atomically.
pub(crate) async fn ensure_not_last_active_admin(
    users: &dyn UserRepositoryPort,
    invitations: &dyn UserInvitationPort,
    password_resets: &dyn PasswordResetPort,
    target: &User,
) -> Result<(), DomainError> {
    if !target.is_admin {
        return Ok(());
    }
    let pending = invitations
        .expiries(&[target.id])
        .await?
        .iter()
        .any(|(id, _)| *id == target.id)
        || password_resets.is_pending(target.id).await?;
    if !pending && users.count_admins().await? <= 1 {
        return Err(DomainError::Conflict(
            "cannot remove the last administrator".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeInvitations, FakePasswordResets, FakeUsers};
    use chrono::{Duration, Utc};
    use ferrisgit_domain::user::{NewUser, User};

    fn user(username: &str, is_admin: bool) -> User {
        User {
            id: Uuid::new_v4(),
            username: username.to_string(),
            email: format!("{username}@example.com"),
            password_hash: "h".to_string(),
            is_admin,
            created_at: Utc::now(),
        }
    }

    struct Fixture {
        users: Arc<FakeUsers>,
        invitations: Arc<FakeInvitations>,
        resets: Arc<FakePasswordResets>,
        use_case: SetAdminUseCase,
    }

    fn setup(existing: Vec<User>) -> Fixture {
        let invitations = Arc::new(FakeInvitations::new());
        let resets = Arc::new(FakePasswordResets::new());
        let users = Arc::new(
            FakeUsers::new(existing)
                .with_invitations(invitations.clone())
                .with_password_resets(resets.clone()),
        );
        Fixture {
            use_case: SetAdminUseCase::new(users.clone(), invitations.clone(), resets.clone()),
            users,
            invitations,
            resets,
        }
    }

    fn is_last_admin_conflict<T: std::fmt::Debug>(result: &Result<T, DomainError>) -> bool {
        matches!(result, Err(DomainError::Conflict(m)) if m == "cannot remove the last administrator")
    }

    #[tokio::test]
    async fn a_regular_user_can_be_promoted_and_it_reports_a_change() {
        let root = user("root", true);
        let alice = user("alice", false);
        let f = setup(vec![root, alice.clone()]);

        assert!(f.use_case.execute(alice.id, true).await.unwrap());

        assert!(f.users.get(alice.id).unwrap().is_admin);
    }

    #[tokio::test]
    async fn promoting_an_admin_again_is_a_no_op_that_reports_no_change() {
        let root = user("root", true);
        let f = setup(vec![root.clone()]);

        assert!(!f.use_case.execute(root.id, true).await.unwrap());

        assert!(f.users.get(root.id).unwrap().is_admin);
        assert_eq!(f.users.count_admins().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn an_admin_who_is_not_the_last_can_be_demoted() {
        let root = user("root", true);
        let carol = user("carol", true);
        let f = setup(vec![root.clone(), carol.clone()]);

        assert!(f.use_case.execute(carol.id, false).await.unwrap());

        assert!(!f.users.get(carol.id).unwrap().is_admin);
        assert!(f.users.get(root.id).unwrap().is_admin);
    }

    #[tokio::test]
    async fn demoting_the_last_admin_is_refused_and_changes_nothing() {
        let root = user("root", true);
        let alice = user("alice", false);
        let f = setup(vec![root.clone(), alice]);

        let result = f.use_case.execute(root.id, false).await;

        assert!(is_last_admin_conflict(&result), "{result:?}");
        assert!(f.users.get(root.id).unwrap().is_admin);
    }

    #[tokio::test]
    async fn an_admin_can_demote_themselves_when_another_admin_remains() {
        let root = user("root", true);
        let carol = user("carol", true);
        let f = setup(vec![root.clone(), carol.clone()]);

        assert!(f.use_case.execute(root.id, false).await.unwrap());

        assert!(!f.users.get(root.id).unwrap().is_admin);
        assert!(f.users.get(carol.id).unwrap().is_admin);
    }

    #[tokio::test]
    async fn the_second_of_two_admins_cannot_step_down_after_the_first_did() {
        let root = user("root", true);
        let carol = user("carol", true);
        let f = setup(vec![root.clone(), carol.clone()]);
        f.use_case.execute(root.id, false).await.unwrap();

        let result = f.use_case.execute(carol.id, false).await;

        assert!(is_last_admin_conflict(&result), "{result:?}");
        assert_eq!(f.users.count_admins().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn demoting_a_non_admin_is_a_no_op_that_reports_no_change_even_with_a_single_admin() {
        let root = user("root", true);
        let alice = user("alice", false);
        let f = setup(vec![root.clone(), alice.clone()]);

        assert!(!f.use_case.execute(alice.id, false).await.unwrap());

        assert!(!f.users.get(alice.id).unwrap().is_admin);
        assert!(f.users.get(root.id).unwrap().is_admin);
    }

    #[tokio::test]
    async fn an_unknown_user_is_not_found_either_way() {
        let root = user("root", true);
        let f = setup(vec![root.clone()]);

        for is_admin in [true, false] {
            let result = f.use_case.execute(Uuid::new_v4(), is_admin).await;
            assert!(
                matches!(result, Err(DomainError::NotFound(_))),
                "{is_admin}: {result:?}"
            );
        }
        assert!(f.users.get(root.id).unwrap().is_admin);
    }

    /// carol cannot sign in until she activates, so root is still the last usable admin.
    #[tokio::test]
    async fn an_admin_still_pending_activation_does_not_count_toward_the_floor() {
        let root = user("root", true);
        let carol = user("carol", true);
        let f = setup(vec![root.clone(), carol.clone()]);
        f.invitations
            .insert(carol.id, "carol-hash", Utc::now() + Duration::hours(24));

        let result = f.use_case.execute(root.id, false).await;

        assert!(is_last_admin_conflict(&result), "{result:?}");
        assert!(f.users.get(root.id).unwrap().is_admin);

        f.invitations.remove(carol.id);
        assert!(f.use_case.execute(root.id, false).await.unwrap());
    }

    #[tokio::test]
    async fn an_admin_still_pending_activation_can_always_be_demoted() {
        let root = user("root", true);
        let carol = user("carol", true);
        let f = setup(vec![root.clone(), carol.clone()]);
        f.invitations
            .insert(carol.id, "carol-hash", Utc::now() + Duration::hours(24));

        assert!(f.use_case.execute(carol.id, false).await.unwrap());

        assert!(!f.users.get(carol.id).unwrap().is_admin);
        assert!(f.users.get(root.id).unwrap().is_admin);
    }

    /// If carol's link lapses nobody could sign in to issue another one.
    #[tokio::test]
    async fn an_admin_with_a_pending_password_reset_does_not_count_toward_the_floor() {
        let root = user("root", true);
        let carol = user("carol", true);
        let f = setup(vec![root.clone(), carol.clone()]);
        f.resets.insert(
            carol.id,
            "carol-reset-hash",
            Utc::now() + Duration::hours(1),
        );

        let result = f.use_case.execute(root.id, false).await;

        assert!(is_last_admin_conflict(&result), "{result:?}");
        assert!(f.users.get(root.id).unwrap().is_admin);

        f.resets.consume("carol-reset-hash").await.unwrap().unwrap();
        assert!(f.use_case.execute(root.id, false).await.unwrap());
    }

    #[tokio::test]
    async fn an_admin_with_a_pending_password_reset_can_always_be_demoted() {
        let root = user("root", true);
        let carol = user("carol", true);
        let f = setup(vec![root.clone(), carol.clone()]);
        f.resets.insert(
            carol.id,
            "carol-reset-hash",
            Utc::now() - Duration::hours(1),
        );

        assert!(f.use_case.execute(carol.id, false).await.unwrap());

        assert!(!f.users.get(carol.id).unwrap().is_admin);
        assert!(f.users.get(root.id).unwrap().is_admin);
    }

    /// The count is stale (another demotion landed between count and write). The use case must pass on the store's
    /// refusal instead of reporting success.
    struct StaleCount(Arc<FakeUsers>);

    #[async_trait::async_trait]
    impl UserRepositoryPort for StaleCount {
        async fn find_by_username(&self, username: &str) -> Result<Option<User>, DomainError> {
            self.0.find_by_username(username).await
        }
        async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, DomainError> {
            self.0.find_by_id(id).await
        }
        async fn create(&self, new_user: NewUser) -> Result<User, DomainError> {
            self.0.create(new_user).await
        }
        async fn count(&self) -> Result<i64, DomainError> {
            self.0.count().await
        }
        async fn update_email(&self, user_id: Uuid, email: String) -> Result<User, DomainError> {
            self.0.update_email(user_id, email).await
        }
        async fn update_password_hash(
            &self,
            user_id: Uuid,
            password_hash: String,
        ) -> Result<(), DomainError> {
            self.0.update_password_hash(user_id, password_hash).await
        }
        async fn count_admins(&self) -> Result<i64, DomainError> {
            Ok(2)
        }
        async fn set_admin(&self, _user_id: Uuid, _is_admin: bool) -> Result<(), DomainError> {
            Err(DomainError::Conflict(
                "cannot remove the last administrator".to_string(),
            ))
        }
        async fn delete(
            &self,
            user_id: Uuid,
            heir_id: Uuid,
        ) -> Result<Vec<ferrisgit_domain::repository::Repository>, DomainError> {
            self.0.delete(user_id, heir_id).await
        }
        async fn search(&self, query: &str, limit: i64) -> Result<Vec<User>, DomainError> {
            self.0.search(query, limit).await
        }
        async fn find_by_username_ignore_case(
            &self,
            username: &str,
        ) -> Result<Option<User>, DomainError> {
            self.0.find_by_username_ignore_case(username).await
        }
        async fn find_by_email_ignore_case(
            &self,
            email: &str,
        ) -> Result<Option<User>, DomainError> {
            self.0.find_by_email_ignore_case(email).await
        }
    }

    #[tokio::test]
    async fn a_refusal_by_the_store_after_a_stale_count_is_surfaced_not_swallowed() {
        let root = user("root", true);
        let use_case = SetAdminUseCase::new(
            Arc::new(StaleCount(Arc::new(FakeUsers::new(vec![root.clone()])))),
            Arc::new(FakeInvitations::new()),
            Arc::new(FakePasswordResets::new()),
        );

        let result = use_case.execute(root.id, false).await;

        assert!(is_last_admin_conflict(&result), "{result:?}");
    }
}
