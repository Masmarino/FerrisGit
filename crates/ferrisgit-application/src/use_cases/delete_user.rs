use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::GroupStorePort;
use ferrisgit_domain::group_membership::GroupMembershipPort;
use ferrisgit_domain::invitation::UserInvitationPort;
use ferrisgit_domain::password_reset::PasswordResetPort;
use ferrisgit_domain::release_asset_storage::ReleaseAssetStoragePort;
use ferrisgit_domain::repository::Repository;
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use ferrisgit_domain::user::{User, UserRepositoryPort};
use uuid::Uuid;

use crate::use_cases::group_maintainer_guard::would_leave_chain_without_a_maintainer_without_user;
use crate::use_cases::set_admin::ensure_not_last_active_admin;

/// What a deletion destroyed, for the audit trail: once the row is gone nothing else says who it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletedUser {
    pub username: String,
    pub deleted_repositories: Vec<String>,
}

/// An admin deletes another user's account and what it owns: personal repositories (rows, git storage, release assets)
/// and, through cascades, tokens, memberships, factors, stars, notifications and the issues, comments, reviews and
/// pipelines they authored. What they wrote on other people's repositories stays, attributed to a deleted user. A group
/// repo they created goes to the acting admin (see `UserRepositoryPort::delete`).
///
/// Refused while the user is the last Maintainer of a group hierarchy, as when removing a member: admins have no bypass
/// on groups, so nobody could manage it again. Account and repositories go in one store transaction that repeats the
/// refusals atomically; the disk is cleaned after the commit, since a leftover directory is harmless.
pub struct DeleteUserUseCase {
    users: Arc<dyn UserRepositoryPort>,
    invitations: Arc<dyn UserInvitationPort>,
    password_resets: Arc<dyn PasswordResetPort>,
    release_asset_storage: Arc<dyn ReleaseAssetStoragePort>,
    groups: Arc<dyn GroupStorePort>,
    group_membership: Arc<dyn GroupMembershipPort>,
}

impl DeleteUserUseCase {
    pub fn new(
        users: Arc<dyn UserRepositoryPort>,
        invitations: Arc<dyn UserInvitationPort>,
        password_resets: Arc<dyn PasswordResetPort>,
        release_asset_storage: Arc<dyn ReleaseAssetStoragePort>,
        groups: Arc<dyn GroupStorePort>,
        group_membership: Arc<dyn GroupMembershipPort>,
    ) -> Self {
        Self {
            users,
            invitations,
            password_resets,
            release_asset_storage,
            groups,
            group_membership,
        }
    }

    /// `Conflict` naming the first hierarchy where `target` is the only Maintainer. Only direct Maintainer grants are
    /// checked, each through its ancestor chain (which covers descendants). It's the early, readable refusal; the store
    /// repeats it atomically.
    async fn ensure_not_last_group_maintainer(&self, target: &User) -> Result<(), DomainError> {
        for writable in self.groups.list_writable_groups(target.id).await? {
            if self
                .group_membership
                .get_member_role(writable.group.id, target.id)
                .await?
                != Some(CollaboratorRole::Maintainer)
            {
                continue; // writable only through an ancestor's grant, which that ancestor's chain covers
            }
            let chain = self.groups.ancestor_chain(writable.group.id).await?;
            if would_leave_chain_without_a_maintainer_without_user(
                self.group_membership.as_ref(),
                &chain,
                target.id,
            )
            .await?
            {
                return Err(DomainError::Conflict(format!(
                    "the user is the last maintainer of the group {}; promote another member first",
                    writable.path
                )));
            }
        }
        Ok(())
    }

    /// `remove_git_storage` runs once per deleted personal repository, after the commit. Every refusal (own account,
    /// unknown user, last active admin, last group Maintainer) comes before anything is written.
    pub async fn execute(
        &self,
        actor_id: Uuid,
        target_user_id: Uuid,
        remove_git_storage: impl Fn(&Repository),
    ) -> Result<DeletedUser, DomainError> {
        // They'd cut off their own session mid-request, and the last-admin check below lets it through when another
        // admin exists.
        if target_user_id == actor_id {
            return Err(DomainError::Validation(
                "you cannot delete your own account".to_string(),
            ));
        }
        let target = self
            .users
            .find_by_id(target_user_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("user".to_string()))?;
        // Same refusal and same meaning of "active" as when demoting: a pending invitation or reset doesn't count.
        ensure_not_last_active_admin(
            self.users.as_ref(),
            self.invitations.as_ref(),
            self.password_resets.as_ref(),
            &target,
        )
        .await?;
        self.ensure_not_last_group_maintainer(&target).await?;

        let deleted = self.users.delete(target.id, actor_id).await?;

        // Committed. Nothing below can fail the request: a retry would find the account already gone.
        for repository in &deleted {
            remove_git_storage(repository);
            if let Err(error) = self
                .release_asset_storage
                .delete_all_for_repository(repository.id)
                .await
            {
                tracing::warn!(%error, repository_id = %repository.id, "failed to remove a deleted user's repository release assets");
            }
        }
        Ok(DeletedUser {
            username: target.username,
            deleted_repositories: deleted.into_iter().map(|r| r.name).collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::test_support::{
        FakeGroups, FakeInvitations, FakePasswordResets, FakeRepositories, FakeStorage, FakeUsers,
    };
    use crate::use_cases::fixtures;
    use chrono::{Duration, Utc};
    use ferrisgit_domain::group::{Group, NewGroup};

    fn user(username: &str, is_admin: bool) -> User {
        User {
            is_admin,
            ..fixtures::user(username)
        }
    }

    fn repository(owner_id: Uuid, name: &str, group_id: Option<Uuid>) -> Repository {
        Repository {
            name: name.to_string(),
            group_id,
            disk_path: format!("{owner_id}/{name}.git"),
            ..fixtures::repository(owner_id)
        }
    }

    struct Fixture {
        users: Arc<FakeUsers>,
        invitations: Arc<FakeInvitations>,
        resets: Arc<FakePasswordResets>,
        repositories: Arc<FakeRepositories>,
        release_assets: Arc<FakeStorage>,
        groups: Arc<FakeGroups>,
        removed_storage: Mutex<Vec<String>>,
        use_case: DeleteUserUseCase,
    }

    impl Fixture {
        async fn delete(&self, actor_id: Uuid, target: Uuid) -> Result<DeletedUser, DomainError> {
            self.use_case
                .execute(actor_id, target, |r| {
                    self.removed_storage
                        .lock()
                        .unwrap()
                        .push(r.disk_path.clone())
                })
                .await
        }

        fn removed_storage(&self) -> Vec<String> {
            self.removed_storage.lock().unwrap().clone()
        }

        async fn group(&self, name: &str, parent: Option<&Group>) -> Group {
            self.groups
                .create(NewGroup {
                    parent_group_id: parent.map(|p| p.id),
                    name: name.to_string(),
                    description: String::new(),
                    created_by: Uuid::new_v4(),
                })
                .await
                .unwrap()
        }

        async fn member(&self, group: &Group, user: &User, role: CollaboratorRole) {
            self.groups
                .add_member(group.id, user.id, role)
                .await
                .unwrap();
        }
    }

    fn setup(existing: Vec<User>, repositories: Vec<Repository>) -> Fixture {
        let invitations = Arc::new(FakeInvitations::new());
        let resets = Arc::new(FakePasswordResets::new());
        let repositories = Arc::new(FakeRepositories::new(repositories));
        let users = Arc::new(
            FakeUsers::new(existing)
                .with_invitations(invitations.clone())
                .with_password_resets(resets.clone())
                .with_repositories(repositories.clone()),
        );
        let release_assets = Arc::new(FakeStorage::new());
        let groups = Arc::new(FakeGroups::empty());
        Fixture {
            use_case: DeleteUserUseCase::new(
                users.clone(),
                invitations.clone(),
                resets.clone(),
                release_assets.clone(),
                groups.clone(),
                groups.clone(),
            ),
            groups,
            users,
            invitations,
            resets,
            repositories,
            release_assets,
            removed_storage: Mutex::new(vec![]),
        }
    }

    fn is_last_admin_conflict<T: std::fmt::Debug>(result: &Result<T, DomainError>) -> bool {
        matches!(result, Err(DomainError::Conflict(m)) if m == "cannot remove the last administrator")
    }

    #[tokio::test]
    async fn a_regular_user_is_deleted() {
        let root = user("root", true);
        let alice = user("alice", false);
        let f = setup(vec![root.clone(), alice.clone()], vec![]);

        f.delete(root.id, alice.id).await.unwrap();

        assert!(f.users.get(alice.id).is_none());
        assert!(f.users.get(root.id).is_some());
    }

    #[tokio::test]
    async fn a_user_without_repositories_is_deleted_without_touching_any_repository() {
        let root = user("root", true);
        let alice = user("alice", false);
        let roots_repo = repository(root.id, "infra", None);
        let f = setup(vec![root.clone(), alice.clone()], vec![roots_repo.clone()]);

        f.delete(root.id, alice.id).await.unwrap();

        assert!(f.users.get(alice.id).is_none());
        assert!(f.repositories.deleted_ids().is_empty());
        assert!(f.removed_storage().is_empty());
        assert!(f.release_assets.deleted_for_repository().is_empty());
        assert!(
            f.repositories.get(roots_repo.id).is_some(),
            "someone else's repository is untouched"
        );
    }

    #[tokio::test]
    async fn every_personal_repository_is_deleted_with_its_git_storage_and_release_assets() {
        let root = user("root", true);
        let alice = user("alice", false);
        let first = repository(alice.id, "first", None);
        let second = repository(alice.id, "second", None);
        let roots_repo = repository(root.id, "infra", None);
        let f = setup(
            vec![root.clone(), alice.clone()],
            vec![first.clone(), second.clone(), roots_repo.clone()],
        );

        let outcome = f.delete(root.id, alice.id).await.unwrap();

        let mut names = outcome.deleted_repositories.clone();
        names.sort();
        assert_eq!(
            (outcome.username.as_str(), names),
            ("alice", vec!["first".to_string(), "second".to_string()]),
            "what the audit event records"
        );
        let mut deleted = f.repositories.deleted_ids();
        deleted.sort();
        let mut expected = vec![first.id, second.id];
        expected.sort();
        assert_eq!(deleted, expected);
        let mut removed = f.removed_storage();
        removed.sort();
        assert_eq!(
            removed,
            vec![first.disk_path.clone(), second.disk_path.clone()]
        );
        let mut assets = f.release_assets.deleted_for_repository();
        assets.sort();
        assert_eq!(assets, expected);
        assert!(f.repositories.get(roots_repo.id).is_some());
        assert!(f.users.get(alice.id).is_none());
    }

    #[tokio::test]
    async fn a_group_repository_the_user_created_is_not_deleted_but_handed_to_the_acting_admin() {
        let root = user("root", true);
        let alice = user("alice", false);
        let group_repo = repository(alice.id, "shared", Some(Uuid::new_v4()));
        let f = setup(vec![root.clone(), alice.clone()], vec![group_repo.clone()]);

        let outcome = f.delete(root.id, alice.id).await.unwrap();

        assert!(outcome.deleted_repositories.is_empty());
        assert!(f.repositories.deleted_ids().is_empty());
        assert!(f.removed_storage().is_empty());
        assert_eq!(
            f.repositories.get(group_repo.id).map(|r| r.owner_id),
            Some(root.id)
        );
    }

    /// A cleanup failure after the commit mustn't fail the request, and every repo's git storage is still removed.
    #[tokio::test]
    async fn a_release_asset_cleanup_failure_after_the_commit_does_not_fail_the_deletion() {
        let root = user("root", true);
        let alice = user("alice", false);
        let first = repository(alice.id, "first", None);
        let second = repository(alice.id, "second", None);
        let repositories = Arc::new(FakeRepositories::new(vec![first.clone(), second.clone()]));
        let users = Arc::new(
            FakeUsers::new(vec![root.clone(), alice.clone()])
                .with_repositories(repositories.clone()),
        );
        let groups = Arc::new(FakeGroups::empty());
        let use_case = DeleteUserUseCase::new(
            users.clone(),
            Arc::new(FakeInvitations::new()),
            Arc::new(FakePasswordResets::new()),
            Arc::new(FakeStorage::failing()),
            groups.clone(),
            groups,
        );
        let removed = Mutex::new(vec![]);

        let outcome = use_case
            .execute(root.id, alice.id, |r| removed.lock().unwrap().push(r.id))
            .await
            .unwrap();

        assert_eq!(outcome.deleted_repositories.len(), 2);
        assert!(users.get(alice.id).is_none());
        assert_eq!(removed.lock().unwrap().len(), 2);
    }

    /// The store refuses after the early checks passed (a concurrent deletion made the admin count stale).
    struct StaleFloor(Arc<FakeUsers>);

    #[async_trait::async_trait]
    impl UserRepositoryPort for StaleFloor {
        async fn find_by_username(&self, username: &str) -> Result<Option<User>, DomainError> {
            self.0.find_by_username(username).await
        }
        async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, DomainError> {
            self.0.find_by_id(id).await
        }
        async fn create(
            &self,
            new_user: ferrisgit_domain::user::NewUser,
        ) -> Result<User, DomainError> {
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
        async fn set_admin(&self, user_id: Uuid, is_admin: bool) -> Result<(), DomainError> {
            self.0.set_admin(user_id, is_admin).await
        }
        async fn delete(
            &self,
            user_id: Uuid,
            heir_id: Uuid,
        ) -> Result<Vec<Repository>, DomainError> {
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
    async fn a_refusal_by_the_store_after_a_stale_early_check_leaves_every_repository_in_place() {
        let carol = user("carol", true);
        let carols = repository(carol.id, "infra", None);
        let repositories = Arc::new(FakeRepositories::new(vec![carols.clone()]));
        let users =
            Arc::new(FakeUsers::new(vec![carol.clone()]).with_repositories(repositories.clone()));
        let release_assets = Arc::new(FakeStorage::new());
        let groups = Arc::new(FakeGroups::empty());
        let use_case = DeleteUserUseCase::new(
            Arc::new(StaleFloor(users.clone())),
            Arc::new(FakeInvitations::new()),
            Arc::new(FakePasswordResets::new()),
            release_assets.clone(),
            groups.clone(),
            groups,
        );

        let result = use_case
            .execute(Uuid::new_v4(), carol.id, |_| {
                panic!("no git storage may be removed on a refusal")
            })
            .await;

        assert!(is_last_admin_conflict(&result), "{result:?}");
        assert!(users.get(carol.id).is_some());
        assert!(repositories.get(carols.id).is_some());
        assert!(repositories.deleted_ids().is_empty());
        assert!(release_assets.deleted_for_repository().is_empty());
    }

    #[tokio::test]
    async fn an_admin_cannot_delete_their_own_account_and_nothing_is_deleted() {
        let root = user("root", true);
        let carol = user("carol", true);
        let own_repo = repository(root.id, "infra", None);
        let f = setup(vec![root.clone(), carol], vec![own_repo]);

        let result = f.delete(root.id, root.id).await;

        assert!(
            matches!(&result, Err(DomainError::Validation(m)) if m == "you cannot delete your own account"),
            "{result:?}"
        );
        assert!(f.users.get(root.id).is_some());
        assert!(f.repositories.deleted_ids().is_empty());
        assert!(f.removed_storage().is_empty());
    }

    #[tokio::test]
    async fn an_unknown_user_is_not_found_and_nothing_is_deleted() {
        let root = user("root", true);
        let f = setup(vec![root.clone()], vec![repository(root.id, "infra", None)]);

        let result = f.delete(root.id, Uuid::new_v4()).await;

        assert!(
            matches!(result, Err(DomainError::NotFound(_))),
            "{result:?}"
        );
        assert!(f.repositories.deleted_ids().is_empty());
        assert_eq!(f.users.snapshot().len(), 1);
    }

    #[tokio::test]
    async fn an_admin_is_deleted_when_another_active_admin_remains() {
        let root = user("root", true);
        let carol = user("carol", true);
        let f = setup(vec![root.clone(), carol.clone()], vec![]);

        f.delete(root.id, carol.id).await.unwrap();

        assert!(f.users.get(carol.id).is_none());
        assert_eq!(f.users.count_admins().await.unwrap(), 1);
    }

    /// The actor is someone else, so the refusal comes from the last-admin check, not the self-deletion rule.
    #[tokio::test]
    async fn deleting_the_last_active_admin_is_refused_and_deletes_nothing() {
        let root = user("root", true);
        let alice = user("alice", false);
        let roots_repo = repository(root.id, "infra", None);
        let f = setup(vec![root.clone(), alice], vec![roots_repo.clone()]);

        let result = f.delete(Uuid::new_v4(), root.id).await;

        assert!(is_last_admin_conflict(&result), "{result:?}");
        assert!(f.users.get(root.id).is_some());
        assert!(f.repositories.deleted_ids().is_empty());
        assert!(f.removed_storage().is_empty());
        assert!(f.release_assets.deleted_for_repository().is_empty());
    }

    /// Carol can't sign in until she activates, so root is still the last usable admin.
    #[tokio::test]
    async fn an_admin_still_pending_activation_does_not_count_toward_the_floor() {
        let root = user("root", true);
        let carol = user("carol", true);
        let f = setup(vec![root.clone(), carol.clone()], vec![]);
        f.invitations
            .insert(carol.id, "carol-hash", Utc::now() + Duration::hours(24));

        let result = f.delete(carol.id, root.id).await;

        assert!(is_last_admin_conflict(&result), "{result:?}");
        assert!(f.users.get(root.id).is_some());

        f.invitations.remove(carol.id);
        f.delete(carol.id, root.id).await.unwrap();
        assert!(f.users.get(root.id).is_none());
    }

    #[tokio::test]
    async fn an_admin_still_pending_activation_can_always_be_deleted() {
        let root = user("root", true);
        let carol = user("carol", true);
        let f = setup(vec![root.clone(), carol.clone()], vec![]);
        f.invitations
            .insert(carol.id, "carol-hash", Utc::now() + Duration::hours(24));

        f.delete(root.id, carol.id).await.unwrap();

        assert!(f.users.get(carol.id).is_none());
    }

    #[tokio::test]
    async fn an_admin_with_a_pending_password_reset_does_not_count_toward_the_floor() {
        let root = user("root", true);
        let carol = user("carol", true);
        let f = setup(vec![root.clone(), carol.clone()], vec![]);
        f.resets.insert(
            carol.id,
            "carol-reset-hash",
            Utc::now() + Duration::hours(1),
        );

        let result = f.delete(carol.id, root.id).await;

        assert!(is_last_admin_conflict(&result), "{result:?}");
        assert!(f.users.get(root.id).is_some());

        f.resets.consume("carol-reset-hash").await.unwrap().unwrap();
        f.delete(carol.id, root.id).await.unwrap();
        assert!(f.users.get(root.id).is_none());
    }

    #[tokio::test]
    async fn an_admin_with_a_pending_password_reset_can_always_be_deleted() {
        let root = user("root", true);
        let carol = user("carol", true);
        let f = setup(vec![root.clone(), carol.clone()], vec![]);
        f.resets.insert(
            carol.id,
            "carol-reset-hash",
            Utc::now() - Duration::hours(1),
        );

        f.delete(root.id, carol.id).await.unwrap();

        assert!(f.users.get(carol.id).is_none());
    }

    #[tokio::test]
    async fn a_non_admin_is_deleted_even_with_a_single_admin() {
        let root = user("root", true);
        let alice = user("alice", false);
        let f = setup(vec![root.clone(), alice.clone()], vec![]);

        f.delete(root.id, alice.id).await.unwrap();

        assert!(f.users.get(alice.id).is_none());
        assert_eq!(f.users.count_admins().await.unwrap(), 1);
    }

    fn is_last_group_maintainer_conflict<T: std::fmt::Debug>(
        result: &Result<T, DomainError>,
        path: &str,
    ) -> bool {
        matches!(result, Err(DomainError::Conflict(m)) if *m == format!("the user is the last maintainer of the group {path}; promote another member first"))
    }

    #[tokio::test]
    async fn the_last_maintainer_of_a_group_cannot_be_deleted_and_nothing_is_deleted() {
        let root = user("root", true);
        let alice = user("alice", false);
        let bob = user("bob", false);
        let alices = repository(alice.id, "hello", None);
        let f = setup(
            vec![root.clone(), alice.clone(), bob.clone()],
            vec![alices.clone()],
        );
        let acme = f.group("acme", None).await;
        f.member(&acme, &alice, CollaboratorRole::Maintainer).await;
        f.member(&acme, &bob, CollaboratorRole::Contributor).await;

        let result = f.delete(root.id, alice.id).await;

        assert!(
            is_last_group_maintainer_conflict(&result, "acme"),
            "{result:?}"
        );
        assert!(f.users.get(alice.id).is_some());
        assert!(
            f.groups
                .has_member(acme.id, alice.id, CollaboratorRole::Maintainer)
        );
        assert!(f.repositories.deleted_ids().is_empty());
        assert!(f.removed_storage().is_empty());
        assert!(f.release_assets.deleted_for_repository().is_empty());
    }

    #[tokio::test]
    async fn the_refusal_names_the_subgroup_left_without_a_maintainer() {
        let root = user("root", true);
        let alice = user("alice", false);
        let bob = user("bob", false);
        let f = setup(vec![root.clone(), alice.clone(), bob.clone()], vec![]);
        let shared = f.group("shared", None).await;
        f.member(&shared, &alice, CollaboratorRole::Maintainer)
            .await;
        f.member(&shared, &bob, CollaboratorRole::Maintainer).await;
        let acme = f.group("acme", None).await;
        f.member(&acme, &bob, CollaboratorRole::Reader).await;
        let backend = f.group("backend", Some(&acme)).await;
        f.member(&backend, &alice, CollaboratorRole::Maintainer)
            .await;

        let result = f.delete(root.id, alice.id).await;

        assert!(
            is_last_group_maintainer_conflict(&result, "acme/backend"),
            "{result:?}"
        );
        assert!(f.users.get(alice.id).is_some());
    }

    /// Maintainer of both a group and its parent doesn't make her the "other" Maintainer of either.
    #[tokio::test]
    async fn maintainer_grants_the_user_holds_across_one_chain_do_not_cover_each_other() {
        let root = user("root", true);
        let alice = user("alice", false);
        let f = setup(vec![root.clone(), alice.clone()], vec![]);
        let acme = f.group("acme", None).await;
        let backend = f.group("backend", Some(&acme)).await;
        f.member(&acme, &alice, CollaboratorRole::Maintainer).await;
        f.member(&backend, &alice, CollaboratorRole::Maintainer)
            .await;

        let result = f.delete(root.id, alice.id).await;

        assert!(
            matches!(&result, Err(DomainError::Conflict(m)) if m.starts_with("the user is the last maintainer of the group ")),
            "{result:?}"
        );
        assert!(f.users.get(alice.id).is_some());
    }

    #[tokio::test]
    async fn a_group_maintainer_is_deleted_when_another_maintainer_remains_in_the_group() {
        let root = user("root", true);
        let alice = user("alice", false);
        let bob = user("bob", false);
        let f = setup(vec![root.clone(), alice.clone(), bob.clone()], vec![]);
        let acme = f.group("acme", None).await;
        f.member(&acme, &alice, CollaboratorRole::Maintainer).await;
        f.member(&acme, &bob, CollaboratorRole::Maintainer).await;

        f.delete(root.id, alice.id).await.unwrap();

        assert!(f.users.get(alice.id).is_none());
    }

    #[tokio::test]
    async fn a_subgroup_maintainer_is_deleted_when_an_ancestor_has_another_maintainer() {
        let root = user("root", true);
        let alice = user("alice", false);
        let bob = user("bob", false);
        let f = setup(vec![root.clone(), alice.clone(), bob.clone()], vec![]);
        let acme = f.group("acme", None).await;
        f.member(&acme, &bob, CollaboratorRole::Maintainer).await;
        let backend = f.group("backend", Some(&acme)).await;
        f.member(&backend, &alice, CollaboratorRole::Maintainer)
            .await;

        f.delete(root.id, alice.id).await.unwrap();

        assert!(f.users.get(alice.id).is_none());
    }

    #[tokio::test]
    async fn a_member_who_maintains_no_group_is_deleted() {
        let root = user("root", true);
        let alice = user("alice", false);
        let bob = user("bob", false);
        let f = setup(vec![root.clone(), alice.clone(), bob.clone()], vec![]);
        let acme = f.group("acme", None).await;
        f.member(&acme, &bob, CollaboratorRole::Maintainer).await;
        f.member(&acme, &alice, CollaboratorRole::Contributor).await;
        let backend = f.group("backend", Some(&acme)).await;
        f.member(&backend, &alice, CollaboratorRole::Reader).await;

        f.delete(root.id, alice.id).await.unwrap();

        assert!(f.users.get(alice.id).is_none());
    }
}
