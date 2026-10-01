use std::sync::Arc;

use ferrisgit_domain::api_token::ApiTokenRepositoryPort;
use ferrisgit_domain::audit::{EventPublisherPort, SecurityEvent};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::GroupStorePort;
use ferrisgit_domain::group_membership::GroupMembershipPort;
use ferrisgit_domain::repository::{Repository, RepositoryVisibility};
use ferrisgit_domain::repository_collaborator::{
    CollaboratorRole, RepositoryCollaboratorStorePort,
};
use ferrisgit_domain::runner::RunnerRepositoryPort;
use uuid::Uuid;

use crate::token_hash::hash_token;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitAccess {
    Read,
    Write,
}

/// Authenticates and authorizes a git smart-HTTP request against an already-resolved `Repository` (path resolution
/// happens in the route via `ResolvePathUseCase`).
pub struct AuthenticateGitRequestUseCase {
    api_tokens: Arc<dyn ApiTokenRepositoryPort>,
    events: Arc<dyn EventPublisherPort>,
    runners: Arc<dyn RunnerRepositoryPort>,
    collaborators: Arc<dyn RepositoryCollaboratorStorePort>,
    groups: Arc<dyn GroupStorePort>,
    group_membership: Arc<dyn GroupMembershipPort>,
}

impl AuthenticateGitRequestUseCase {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        api_tokens: Arc<dyn ApiTokenRepositoryPort>,
        events: Arc<dyn EventPublisherPort>,
        runners: Arc<dyn RunnerRepositoryPort>,
        collaborators: Arc<dyn RepositoryCollaboratorStorePort>,
        groups: Arc<dyn GroupStorePort>,
        group_membership: Arc<dyn GroupMembershipPort>,
    ) -> Self {
        Self {
            api_tokens,
            events,
            runners,
            collaborators,
            groups,
            group_membership,
        }
    }

    pub async fn execute(
        &self,
        credentials: Option<(String, String)>,
        repo: Repository,
        access: GitAccess,
    ) -> Result<(Repository, Option<Uuid>), DomainError> {
        // Anyone can read a public repo, even when credentials are sent (a `user:token@host` clone URL always sends
        // Basic Auth). Only a write has to prove access.
        if matches!(access, GitAccess::Read) && repo.visibility == RepositoryVisibility::Public {
            return Ok((repo, None));
        }

        let Some((username, plain_token)) = credentials else {
            self.events
                .publish_security_event(
                    SecurityEvent::GitAccessDenied {
                        username: "anonymous".to_string(),
                        repository: repo.name.clone(),
                    },
                    None,
                )
                .await
                .ok();
            return Err(DomainError::Unauthorized("missing credentials".to_string()));
        };

        let stored = self
            .api_tokens
            .find_by_hash(&hash_token(&plain_token))
            .await?;

        if let Some(stored) = stored {
            let required = if access == GitAccess::Write {
                CollaboratorRole::Contributor
            } else {
                CollaboratorRole::Reader
            };
            let authorized = self
                .effective_role(&repo, stored.user_id)
                .await?
                .is_some_and(|r| r >= required);
            if !authorized {
                self.events
                    .publish_security_event(
                        SecurityEvent::GitAccessDenied {
                            username,
                            repository: repo.name.clone(),
                        },
                        Some(stored.user_id),
                    )
                    .await
                    .ok();
                return Err(DomainError::Unauthorized(
                    "token does not grant access to this repository".to_string(),
                ));
            }
            self.api_tokens.touch_last_used(stored.id).await.ok();
            return Ok((repo, Some(stored.user_id)));
        }

        if let Some(runner) = self
            .runners
            .find_by_token_hash(&hash_token(&plain_token))
            .await?
        {
            if matches!(access, GitAccess::Write) {
                self.events
                    .publish_security_event(
                        SecurityEvent::GitAccessDenied {
                            username: runner.name.clone(),
                            repository: repo.name.clone(),
                        },
                        None,
                    )
                    .await
                    .ok();
                return Err(DomainError::Unauthorized(
                    "runner tokens cannot push".to_string(),
                ));
            }
            self.runners.touch_heartbeat(runner.id).await.ok();
            return Ok((repo, None));
        }

        self.events
            .publish_security_event(SecurityEvent::GitTokenInvalid { username }, None)
            .await
            .ok();
        Err(DomainError::Unauthorized("invalid token".to_string()))
    }

    /// The max role comes from `effective_repository_role`, the policy shared with
    /// `ferrisgit-api::authz::require_role_by_id`. `authz_parity_flow.rs` checks that both callers behave the same.
    async fn effective_role(
        &self,
        repo: &Repository,
        user_id: Uuid,
    ) -> Result<Option<CollaboratorRole>, DomainError> {
        ferrisgit_domain::repository_authz::effective_repository_role(
            self.collaborators.as_ref(),
            self.groups.as_ref(),
            self.group_membership.as_ref(),
            repo,
            user_id,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        FakeApiTokens, FakeCollaborators, FakeEvents, FakeGroups, FakeRunners,
    };
    use async_trait::async_trait;
    use chrono::Utc;
    use ferrisgit_domain::api_token::ApiToken;
    use ferrisgit_domain::group::{Group, GroupMember, GroupWithPath, NewGroup};
    use ferrisgit_domain::user::User;
    use uuid::Uuid;

    /// Never queried: personal repositories (`group_id: None`) never touch `self.groups`.
    struct UnimplementedGroups;
    #[async_trait]
    impl GroupStorePort for UnimplementedGroups {
        async fn create(&self, _new_group: NewGroup) -> Result<Group, DomainError> {
            unimplemented!()
        }
        async fn find_by_id(&self, _id: Uuid) -> Result<Option<Group>, DomainError> {
            unimplemented!()
        }
        async fn find_child_by_name(
            &self,
            _parent_id: Option<Uuid>,
            _name: &str,
        ) -> Result<Option<Group>, DomainError> {
            unimplemented!()
        }
        async fn list_children(&self, _parent_id: Option<Uuid>) -> Result<Vec<Group>, DomainError> {
            unimplemented!()
        }
        async fn ancestor_chain(&self, _group_id: Uuid) -> Result<Vec<Group>, DomainError> {
            unimplemented!()
        }
        async fn list_writable_groups(
            &self,
            _user_id: Uuid,
        ) -> Result<Vec<GroupWithPath>, DomainError> {
            unimplemented!()
        }
        async fn list_member_group_ids(&self, _user_id: Uuid) -> Result<Vec<Uuid>, DomainError> {
            unimplemented!()
        }
    }

    #[async_trait]
    impl GroupMembershipPort for UnimplementedGroups {
        async fn add_member(
            &self,
            _group_id: Uuid,
            _user_id: Uuid,
            _role: CollaboratorRole,
        ) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn set_member_role(
            &self,
            _group_id: Uuid,
            _user_id: Uuid,
            _role: CollaboratorRole,
        ) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn remove_member(&self, _group_id: Uuid, _user_id: Uuid) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn list_members(&self, _group_id: Uuid) -> Result<Vec<GroupMember>, DomainError> {
            unimplemented!()
        }
        async fn get_member_role(
            &self,
            _group_id: Uuid,
            _user_id: Uuid,
        ) -> Result<Option<CollaboratorRole>, DomainError> {
            unimplemented!()
        }
    }

    fn user(username: &str) -> User {
        User {
            id: Uuid::new_v4(),
            username: username.to_string(),
            email: format!("{username}@example.com"),
            password_hash: "h".to_string(),
            is_admin: false,
            created_at: Utc::now(),
        }
    }

    fn personal_repo(owner_id: Uuid, name: &str, visibility: RepositoryVisibility) -> Repository {
        Repository {
            id: Uuid::new_v4(),
            owner_id,
            name: name.to_string(),
            group_id: None,
            description: String::new(),
            disk_path: "path".to_string(),
            visibility,
            created_at: Utc::now(),
        }
    }

    fn fixture(
        visibility: RepositoryVisibility,
    ) -> (
        Uuid,
        Repository,
        ApiToken,
        AuthenticateGitRequestUseCase,
        Arc<FakeEvents>,
    ) {
        let owner = user("florian");
        let repo = personal_repo(owner.id, "hello", visibility);
        let token = ApiToken {
            id: Uuid::new_v4(),
            user_id: owner.id,
            name: "ci".to_string(),
            token_hash: hash_token("fg_valid"),
            created_at: Utc::now(),
            last_used_at: None,
        };
        let owner_id = owner.id;
        let events = Arc::new(FakeEvents::default());
        let use_case = AuthenticateGitRequestUseCase::new(
            Arc::new(FakeApiTokens::new(vec![token.clone()])),
            events.clone(),
            Arc::new(FakeRunners::new(vec![])),
            Arc::new(FakeCollaborators::empty()),
            Arc::new(UnimplementedGroups),
            Arc::new(UnimplementedGroups),
        );
        (owner_id, repo, token, use_case, events)
    }

    #[tokio::test]
    async fn a_valid_token_grants_write_access_to_its_owners_repository() {
        let (owner_id, repo, _, use_case, _) = fixture(RepositoryVisibility::Private);
        let result = use_case
            .execute(
                Some(("florian".to_string(), "fg_valid".to_string())),
                repo,
                GitAccess::Write,
            )
            .await;
        assert!(result.is_ok());
        let (_, resolved_user_id) = result.unwrap();
        assert_eq!(resolved_user_id, Some(owner_id));
    }

    #[tokio::test]
    async fn reading_a_public_repository_without_credentials_is_allowed() {
        let (_, repo, _, use_case, _) = fixture(RepositoryVisibility::Public);
        let result = use_case.execute(None, repo, GitAccess::Read).await;
        assert!(result.is_ok());
        let (_, resolved_user_id) = result.unwrap();
        assert_eq!(resolved_user_id, None);
    }

    #[tokio::test]
    async fn reading_a_private_repository_without_credentials_is_unauthorized() {
        let (_, repo, _, use_case, events) = fixture(RepositoryVisibility::Private);
        let result = use_case.execute(None, repo, GitAccess::Read).await;
        assert!(matches!(result, Err(DomainError::Unauthorized(_))));

        let published = events.security_events();
        assert_eq!(
            published.len(),
            1,
            "expected exactly one published event, got {published:?}"
        );
        assert!(
            matches!(&published[0], (SecurityEvent::GitAccessDenied { username, .. }, None) if username == "anonymous"),
            "expected a GitAccessDenied event for \"anonymous\" with no actor, got {published:?}"
        );
    }

    #[tokio::test]
    async fn an_invalid_token_is_unauthorized() {
        let (_, repo, _, use_case, events) = fixture(RepositoryVisibility::Private);
        let result = use_case
            .execute(
                Some(("florian".to_string(), "fg_wrong".to_string())),
                repo,
                GitAccess::Read,
            )
            .await;
        assert!(matches!(result, Err(DomainError::Unauthorized(_))));

        let published = events.security_events();
        assert_eq!(
            published.len(),
            1,
            "expected exactly one published event, got {published:?}"
        );
        assert!(
            matches!(&published[0], (SecurityEvent::GitTokenInvalid { username }, None) if username == "florian"),
            "expected a GitTokenInvalid event (not LoginFailed) with no actor, got {published:?}"
        );
    }

    #[tokio::test]
    async fn reading_a_public_repository_with_a_foreign_valid_token_is_still_allowed() {
        // A different user's valid token must not matter for a public read.
        let owner = user("florian");
        let repo = personal_repo(owner.id, "hello", RepositoryVisibility::Public);

        let foreign_user = user("someone-else");
        let foreign_token = ApiToken {
            id: Uuid::new_v4(),
            user_id: foreign_user.id,
            name: "their-token".to_string(),
            token_hash: hash_token("fg_theirs"),
            created_at: Utc::now(),
            last_used_at: None,
        };

        let use_case = AuthenticateGitRequestUseCase::new(
            Arc::new(FakeApiTokens::new(vec![foreign_token])),
            Arc::new(FakeEvents::default()),
            Arc::new(FakeRunners::new(vec![])),
            Arc::new(FakeCollaborators::empty()),
            Arc::new(UnimplementedGroups),
            Arc::new(UnimplementedGroups),
        );

        let result = use_case
            .execute(
                Some(("someone-else".to_string(), "fg_theirs".to_string())),
                repo,
                GitAccess::Read,
            )
            .await;
        assert!(
            result.is_ok(),
            "expected public read to succeed regardless of whose valid token was presented, got {result:?}"
        );
    }

    #[tokio::test]
    async fn a_listed_collaborators_token_grants_write_access_to_a_private_repository_it_does_not_own()
     {
        let owner = user("florian");
        let repo = personal_repo(owner.id, "hello", RepositoryVisibility::Private);

        let collaborator = user("collab");
        let collaborator_token = ApiToken {
            id: Uuid::new_v4(),
            user_id: collaborator.id,
            name: "collab-token".to_string(),
            token_hash: hash_token("fg_collab"),
            created_at: Utc::now(),
            last_used_at: None,
        };

        let use_case = AuthenticateGitRequestUseCase::new(
            Arc::new(FakeApiTokens::new(vec![collaborator_token])),
            Arc::new(FakeEvents::default()),
            Arc::new(FakeRunners::new(vec![])),
            Arc::new(FakeCollaborators::new(vec![(
                repo.id,
                collaborator.id,
                CollaboratorRole::Contributor,
            )])),
            Arc::new(UnimplementedGroups),
            Arc::new(UnimplementedGroups),
        );

        let result = use_case
            .execute(
                Some(("collab".to_string(), "fg_collab".to_string())),
                repo,
                GitAccess::Write,
            )
            .await;
        assert!(
            result.is_ok(),
            "a listed collaborator's token should grant write access even though they don't own the repository, got {result:?}"
        );
    }

    #[tokio::test]
    async fn a_valid_token_belonging_to_neither_the_owner_nor_a_collaborator_is_unauthorized() {
        let owner = user("florian");
        let repo = personal_repo(owner.id, "hello", RepositoryVisibility::Private);

        let stranger = user("stranger");
        let stranger_token = ApiToken {
            id: Uuid::new_v4(),
            user_id: stranger.id,
            name: "stranger-token".to_string(),
            token_hash: hash_token("fg_stranger"),
            created_at: Utc::now(),
            last_used_at: None,
        };

        let use_case = AuthenticateGitRequestUseCase::new(
            Arc::new(FakeApiTokens::new(vec![stranger_token])),
            Arc::new(FakeEvents::default()),
            Arc::new(FakeRunners::new(vec![])),
            Arc::new(FakeCollaborators::empty()), // stranger is not in this list, which is the point of this test
            Arc::new(UnimplementedGroups),
            Arc::new(UnimplementedGroups),
        );

        let result = use_case
            .execute(
                Some(("stranger".to_string(), "fg_stranger".to_string())),
                repo,
                GitAccess::Write,
            )
            .await;
        assert!(
            matches!(result, Err(DomainError::Unauthorized(_))),
            "a valid token belonging to neither the owner nor a listed collaborator must be rejected, got {result:?}"
        );
    }

    #[tokio::test]
    async fn access_is_re_evaluated_fresh_against_current_collaborator_state_not_cached() {
        // The use case never caches an authorization decision: two instances differing only in the collaborator pair
        // give opposite outcomes.
        let owner = user("florian");
        let repo = personal_repo(owner.id, "hello", RepositoryVisibility::Private);

        let collaborator = user("collab");
        let collaborator_token = ApiToken {
            id: Uuid::new_v4(),
            user_id: collaborator.id,
            name: "collab-token".to_string(),
            token_hash: hash_token("fg_collab"),
            created_at: Utc::now(),
            last_used_at: None,
        };

        let while_still_a_collaborator = AuthenticateGitRequestUseCase::new(
            Arc::new(FakeApiTokens::new(vec![collaborator_token.clone()])),
            Arc::new(FakeEvents::default()),
            Arc::new(FakeRunners::new(vec![])),
            Arc::new(FakeCollaborators::new(vec![(
                repo.id,
                collaborator.id,
                CollaboratorRole::Contributor,
            )])),
            Arc::new(UnimplementedGroups),
            Arc::new(UnimplementedGroups),
        );
        let before_removal = while_still_a_collaborator
            .execute(
                Some(("collab".to_string(), "fg_collab".to_string())),
                repo.clone(),
                GitAccess::Write,
            )
            .await;
        assert!(
            before_removal.is_ok(),
            "expected write access while still a listed collaborator, got {before_removal:?}"
        );

        let after_removal = AuthenticateGitRequestUseCase::new(
            Arc::new(FakeApiTokens::new(vec![collaborator_token])),
            Arc::new(FakeEvents::default()),
            Arc::new(FakeRunners::new(vec![])),
            Arc::new(FakeCollaborators::empty()), // same pair, now absent, as after a real removal
            Arc::new(UnimplementedGroups),
            Arc::new(UnimplementedGroups),
        );
        let result_after_removal = after_removal
            .execute(
                Some(("collab".to_string(), "fg_collab".to_string())),
                repo,
                GitAccess::Write,
            )
            .await;
        assert!(
            matches!(result_after_removal, Err(DomainError::Unauthorized(_))),
            "expected write access to be denied once the collaborator row is gone, got {result_after_removal:?}"
        );
    }

    #[tokio::test]
    async fn a_reader_role_collaborator_can_read_but_not_write() {
        let owner = user("florian");
        let repo = personal_repo(owner.id, "hello", RepositoryVisibility::Private);
        let collaborator = user("reader");
        let token = ApiToken {
            id: Uuid::new_v4(),
            user_id: collaborator.id,
            name: "ci".to_string(),
            token_hash: hash_token("fg_reader"),
            created_at: Utc::now(),
            last_used_at: None,
        };
        let use_case = AuthenticateGitRequestUseCase::new(
            Arc::new(FakeApiTokens::new(vec![token])),
            Arc::new(FakeEvents::default()),
            Arc::new(FakeRunners::new(vec![])),
            Arc::new(FakeCollaborators::new(vec![(
                repo.id,
                collaborator.id,
                CollaboratorRole::Reader,
            )])),
            Arc::new(UnimplementedGroups),
            Arc::new(UnimplementedGroups),
        );

        let read_result = use_case
            .execute(
                Some(("florian".to_string(), "fg_reader".to_string())),
                repo,
                GitAccess::Read,
            )
            .await;
        assert!(read_result.is_ok());
    }

    #[tokio::test]
    async fn a_reader_role_collaborator_cannot_write() {
        let owner = user("florian");
        let repo = personal_repo(owner.id, "hello", RepositoryVisibility::Private);
        let collaborator = user("reader");
        let token = ApiToken {
            id: Uuid::new_v4(),
            user_id: collaborator.id,
            name: "ci".to_string(),
            token_hash: hash_token("fg_reader2"),
            created_at: Utc::now(),
            last_used_at: None,
        };
        let use_case = AuthenticateGitRequestUseCase::new(
            Arc::new(FakeApiTokens::new(vec![token])),
            Arc::new(FakeEvents::default()),
            Arc::new(FakeRunners::new(vec![])),
            Arc::new(FakeCollaborators::new(vec![(
                repo.id,
                collaborator.id,
                CollaboratorRole::Reader,
            )])),
            Arc::new(UnimplementedGroups),
            Arc::new(UnimplementedGroups),
        );

        let write_result = use_case
            .execute(
                Some(("florian".to_string(), "fg_reader2".to_string())),
                repo,
                GitAccess::Write,
            )
            .await;
        assert!(matches!(write_result, Err(DomainError::Unauthorized(_))));
    }

    fn fixture_with_runner(
        visibility: RepositoryVisibility,
    ) -> (Repository, AuthenticateGitRequestUseCase, Arc<FakeEvents>) {
        let owner = user("florian");
        let repo = personal_repo(owner.id, "hello", visibility);
        let runner = ferrisgit_domain::runner::Runner {
            id: Uuid::new_v4(),
            name: "vps-1".to_string(),
            token_hash: hash_token("fgr_valid"),
            tags: vec![],
            last_heartbeat_at: None,
            created_at: Utc::now(),
        };
        let events = Arc::new(FakeEvents::default());
        let use_case = AuthenticateGitRequestUseCase::new(
            Arc::new(FakeApiTokens::new(vec![])),
            events.clone(),
            Arc::new(FakeRunners::new(vec![runner])),
            Arc::new(FakeCollaborators::empty()),
            Arc::new(UnimplementedGroups),
            Arc::new(UnimplementedGroups),
        );
        (repo, use_case, events)
    }

    #[tokio::test]
    async fn a_valid_runner_token_reads_a_private_repository_it_does_not_own() {
        let (repo, use_case, _) = fixture_with_runner(RepositoryVisibility::Private);
        let result = use_case
            .execute(
                Some(("any-runner-name".to_string(), "fgr_valid".to_string())),
                repo,
                GitAccess::Read,
            )
            .await;
        assert!(
            result.is_ok(),
            "a valid runner token must read any repository regardless of ownership, got {result:?}"
        );
    }

    #[tokio::test]
    async fn a_valid_runner_token_cannot_write() {
        let (repo, use_case, events) = fixture_with_runner(RepositoryVisibility::Private);
        let result = use_case
            .execute(
                Some(("any-runner-name".to_string(), "fgr_valid".to_string())),
                repo,
                GitAccess::Write,
            )
            .await;
        assert!(matches!(result, Err(DomainError::Unauthorized(_))));
        assert!(
            events
                .security_events()
                .iter()
                .any(|(e, _)| matches!(e, SecurityEvent::GitAccessDenied { .. }))
        );
    }

    fn group_repo(
        owner_id: Uuid,
        group_id: Uuid,
        name: &str,
        visibility: RepositoryVisibility,
    ) -> Repository {
        Repository {
            id: Uuid::new_v4(),
            owner_id,
            name: name.to_string(),
            group_id: Some(group_id),
            description: String::new(),
            disk_path: "path".to_string(),
            visibility,
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn a_reader_on_an_ancestor_group_can_read_a_group_repository_with_no_direct_grant() {
        let groups = Arc::new(FakeGroups::empty());
        let creator = user("creator");
        let reader = user("reader");
        let acme = groups
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: creator.id,
            })
            .await
            .unwrap();
        groups
            .add_member(acme.id, reader.id, CollaboratorRole::Reader)
            .await
            .unwrap();
        let repo = group_repo(
            creator.id,
            acme.id,
            "backend",
            RepositoryVisibility::Private,
        );
        let token = ApiToken {
            id: Uuid::new_v4(),
            user_id: reader.id,
            name: "ci".to_string(),
            token_hash: hash_token("fg_reader"),
            created_at: Utc::now(),
            last_used_at: None,
        };

        let use_case = AuthenticateGitRequestUseCase::new(
            Arc::new(FakeApiTokens::new(vec![token])),
            Arc::new(FakeEvents::default()),
            Arc::new(FakeRunners::new(vec![])),
            Arc::new(FakeCollaborators::empty()), // reader has no direct collaborator grant, only the group role
            groups.clone(),
            groups,
        );

        let result = use_case
            .execute(
                Some(("reader".to_string(), "fg_reader".to_string())),
                repo,
                GitAccess::Read,
            )
            .await;
        assert!(
            result.is_ok(),
            "a group Reader should be able to read a repository in that group with no direct grant, got {result:?}"
        );
    }

    #[tokio::test]
    async fn a_contributor_two_levels_up_can_push_to_a_repository_three_levels_deep() {
        let groups = Arc::new(FakeGroups::empty());
        let creator = user("creator");
        let contributor = user("contributor");
        let root = groups
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: creator.id,
            })
            .await
            .unwrap();
        groups
            .add_member(root.id, contributor.id, CollaboratorRole::Contributor)
            .await
            .unwrap();
        let mid = groups
            .create(NewGroup {
                parent_group_id: Some(root.id),
                name: "backend".to_string(),
                description: String::new(),
                created_by: creator.id,
            })
            .await
            .unwrap();
        let leaf = groups
            .create(NewGroup {
                parent_group_id: Some(mid.id),
                name: "infra".to_string(),
                description: String::new(),
                created_by: creator.id,
            })
            .await
            .unwrap();
        let repo = group_repo(
            creator.id,
            leaf.id,
            "terraform-modules",
            RepositoryVisibility::Private,
        );
        let token = ApiToken {
            id: Uuid::new_v4(),
            user_id: contributor.id,
            name: "ci".to_string(),
            token_hash: hash_token("fg_contrib"),
            created_at: Utc::now(),
            last_used_at: None,
        };

        let use_case = AuthenticateGitRequestUseCase::new(
            Arc::new(FakeApiTokens::new(vec![token])),
            Arc::new(FakeEvents::default()),
            Arc::new(FakeRunners::new(vec![])),
            Arc::new(FakeCollaborators::empty()),
            groups.clone(),
            groups,
        );

        let result = use_case
            .execute(
                Some(("contributor".to_string(), "fg_contrib".to_string())),
                repo,
                GitAccess::Write,
            )
            .await;
        assert!(
            result.is_ok(),
            "a root-group Contributor should be able to push to a repository 3 levels deep, got {result:?}"
        );
    }

    #[tokio::test]
    async fn the_creator_of_a_group_repository_with_no_group_role_is_rejected() {
        // A group repository's `owner_id` ("created by") grants no implicit access: the creator must not keep
        // read+write after being removed from the group.
        let groups = Arc::new(FakeGroups::empty());
        let creator = user("creator");
        let acme = groups
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: creator.id,
            })
            .await
            .unwrap();
        let repo = group_repo(
            creator.id,
            acme.id,
            "backend",
            RepositoryVisibility::Private,
        );
        let token = ApiToken {
            id: Uuid::new_v4(),
            user_id: creator.id,
            name: "ci".to_string(),
            token_hash: hash_token("fg_creator"),
            created_at: Utc::now(),
            last_used_at: None,
        };

        let use_case = AuthenticateGitRequestUseCase::new(
            Arc::new(FakeApiTokens::new(vec![token])),
            Arc::new(FakeEvents::default()),
            Arc::new(FakeRunners::new(vec![])),
            Arc::new(FakeCollaborators::empty()),
            groups.clone(),
            groups,
        );

        let read_result = use_case
            .execute(
                Some(("creator".to_string(), "fg_creator".to_string())),
                repo.clone(),
                GitAccess::Read,
            )
            .await;
        assert!(
            matches!(read_result, Err(DomainError::Unauthorized(_))),
            "the repo creator must NOT get an implicit read bypass on a group repository, got {read_result:?}"
        );

        let write_result = use_case
            .execute(
                Some(("creator".to_string(), "fg_creator".to_string())),
                repo,
                GitAccess::Write,
            )
            .await;
        assert!(
            matches!(write_result, Err(DomainError::Unauthorized(_))),
            "the repo creator must NOT get an implicit write bypass on a group repository, got {write_result:?}"
        );
    }

    #[tokio::test]
    async fn removing_a_users_group_role_revokes_git_access_on_the_next_request() {
        // Like the collaborator re-evaluation test, for group membership.
        let creator = user("creator");
        let member = user("member");
        let member_token = ApiToken {
            id: Uuid::new_v4(),
            user_id: member.id,
            name: "ci".to_string(),
            token_hash: hash_token("fg_member"),
            created_at: Utc::now(),
            last_used_at: None,
        };

        let groups_while_a_member = Arc::new(FakeGroups::empty());
        let acme = groups_while_a_member
            .create(NewGroup {
                parent_group_id: None,
                name: "acme".to_string(),
                description: String::new(),
                created_by: creator.id,
            })
            .await
            .unwrap();
        groups_while_a_member
            .add_member(acme.id, member.id, CollaboratorRole::Contributor)
            .await
            .unwrap();
        let repo = group_repo(
            creator.id,
            acme.id,
            "backend",
            RepositoryVisibility::Private,
        );

        let while_still_a_member = AuthenticateGitRequestUseCase::new(
            Arc::new(FakeApiTokens::new(vec![member_token.clone()])),
            Arc::new(FakeEvents::default()),
            Arc::new(FakeRunners::new(vec![])),
            Arc::new(FakeCollaborators::empty()),
            groups_while_a_member.clone(),
            groups_while_a_member,
        );
        let before_removal = while_still_a_member
            .execute(
                Some(("member".to_string(), "fg_member".to_string())),
                repo.clone(),
                GitAccess::Write,
            )
            .await;
        assert!(
            before_removal.is_ok(),
            "expected write access while still a group Contributor, got {before_removal:?}"
        );

        let groups_after_removal = Arc::new(FakeGroups::new(vec![acme]));
        let after_removal = AuthenticateGitRequestUseCase::new(
            Arc::new(FakeApiTokens::new(vec![member_token])),
            Arc::new(FakeEvents::default()),
            Arc::new(FakeRunners::new(vec![])),
            Arc::new(FakeCollaborators::empty()),
            groups_after_removal.clone(),
            groups_after_removal,
        );
        let result_after_removal = after_removal
            .execute(
                Some(("member".to_string(), "fg_member".to_string())),
                repo,
                GitAccess::Write,
            )
            .await;
        assert!(
            matches!(result_after_removal, Err(DomainError::Unauthorized(_))),
            "expected write access to be denied once the group-member row is gone, got {result_after_removal:?}"
        );
    }
}
