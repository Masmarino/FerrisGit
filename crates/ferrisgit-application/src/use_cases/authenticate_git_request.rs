use std::sync::Arc;

use ferrisgit_domain::api_token::ApiTokenRepositoryPort;
use ferrisgit_domain::audit::{EventPublisherPort, SecurityEvent};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::GroupStorePort;
use ferrisgit_domain::group_membership::GroupMembershipPort;
use ferrisgit_domain::public_pages::PublicPagesSettingsPort;
use ferrisgit_domain::repository::{Repository, RepositoryVisibility};
use ferrisgit_domain::repository_authz::effective_repository_role;
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
    public_pages: Arc<dyn PublicPagesSettingsPort>,
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
        public_pages: Arc<dyn PublicPagesSettingsPort>,
    ) -> Self {
        Self {
            api_tokens,
            events,
            runners,
            collaborators,
            groups,
            group_membership,
            public_pages,
        }
    }

    pub async fn execute(
        &self,
        credentials: Option<(String, String)>,
        repo: Repository,
        access: GitAccess,
    ) -> Result<(Repository, Option<Uuid>), DomainError> {
        // A public repo is readable without credentials while the instance has its public pages on, even when
        // credentials are sent (a `user:token@host` clone URL always sends Basic Auth). With them off, anonymous
        // reads are refused like on a private repo; a signed-in user keeps reading it below. A settings read error
        // counts as "off": better to refuse than to open a repository that may have been closed.
        let public_read =
            matches!(access, GitAccess::Read) && repo.visibility == RepositoryVisibility::Public;
        if public_read && self.anonymous_public_read_allowed().await {
            return Ok((repo, None));
        }

        let Some((username, plain_token)) = credentials else {
            self.publish_denied("anonymous".to_string(), &repo, None)
                .await;
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
            // Any signed-in user reads a public repo, as in the API (`require_role_by_id`).
            let authorized = public_read
                || self
                    .effective_role(&repo, stored.user_id)
                    .await?
                    .is_some_and(|r| r >= required);
            if !authorized {
                self.publish_denied(username, &repo, Some(stored.user_id))
                    .await;
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
                self.publish_denied(runner.name.clone(), &repo, None).await;
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

    async fn publish_denied(&self, username: String, repo: &Repository, actor_id: Option<Uuid>) {
        self.events
            .publish_security_event(
                SecurityEvent::GitAccessDenied {
                    username,
                    repository: repo.name.clone(),
                },
                actor_id,
            )
            .await
            .ok();
    }

    async fn anonymous_public_read_allowed(&self) -> bool {
        match self.public_pages.get().await {
            Ok(settings) => settings.public_pages_enabled,
            Err(err) => {
                tracing::error!(error = %err, "failed to read the public pages setting for an anonymous git read");
                false
            }
        }
    }

    /// The max role comes from `effective_repository_role`, the policy shared with
    /// `ferrisgit-api::authz::require_role_by_id`. `authz_parity_flow.rs` checks that both callers behave the same.
    async fn effective_role(
        &self,
        repo: &Repository,
        user_id: Uuid,
    ) -> Result<Option<CollaboratorRole>, DomainError> {
        effective_repository_role(
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
        FakeApiTokens, FakeCollaborators, FakeEvents, FakeGroups, FakePublicPagesSettings,
        FakeRunners,
    };
    use crate::use_cases::fixtures::{group, repository};
    use async_trait::async_trait;
    use chrono::Utc;
    use ferrisgit_domain::api_token::ApiToken;
    use ferrisgit_domain::public_pages::{PublicPagesSettings, PublicPagesSettingsUpdate};
    use ferrisgit_domain::runner::Runner;

    fn public_pages(enabled: bool) -> Arc<dyn PublicPagesSettingsPort> {
        Arc::new(FakePublicPagesSettings::new(PublicPagesSettings {
            public_pages_enabled: enabled,
            ..PublicPagesSettings::default()
        }))
    }

    struct BrokenPublicPagesSettings;
    #[async_trait]
    impl PublicPagesSettingsPort for BrokenPublicPagesSettings {
        async fn get(&self) -> Result<PublicPagesSettings, DomainError> {
            Err(DomainError::Infrastructure("database down".to_string()))
        }
        async fn update(
            &self,
            _update: PublicPagesSettingsUpdate,
        ) -> Result<PublicPagesSettings, DomainError> {
            unimplemented!()
        }
    }

    /// What the use case is wired to: public pages on, and nothing else unless a test adds it.
    struct World {
        tokens: Vec<ApiToken>,
        runners: Vec<Runner>,
        collaborators: Vec<(Uuid, Uuid, CollaboratorRole)>,
        groups: FakeGroups,
        public_pages: Arc<dyn PublicPagesSettingsPort>,
    }

    impl World {
        fn new() -> Self {
            Self {
                tokens: vec![],
                runners: vec![],
                collaborators: vec![],
                groups: FakeGroups::empty(),
                public_pages: public_pages(true),
            }
        }

        fn build(self) -> (AuthenticateGitRequestUseCase, Arc<FakeEvents>) {
            let events = Arc::new(FakeEvents::default());
            let groups = Arc::new(self.groups);
            let use_case = AuthenticateGitRequestUseCase::new(
                Arc::new(FakeApiTokens::new(self.tokens)),
                events.clone(),
                Arc::new(FakeRunners::new(self.runners)),
                Arc::new(FakeCollaborators::new(self.collaborators)),
                groups.clone(),
                groups,
                self.public_pages,
            );
            (use_case, events)
        }
    }

    fn api_token(user_id: Uuid, plain: &str) -> ApiToken {
        ApiToken {
            id: Uuid::new_v4(),
            user_id,
            name: "ci".to_string(),
            token_hash: hash_token(plain),
            created_at: Utc::now(),
            last_used_at: None,
        }
    }

    fn runner(plain: &str) -> Runner {
        Runner {
            id: Uuid::new_v4(),
            name: "vps-1".to_string(),
            token_hash: hash_token(plain),
            tags: vec![],
            last_heartbeat_at: None,
            created_at: Utc::now(),
        }
    }

    fn credentials(username: &str, plain_token: &str) -> Option<(String, String)> {
        Some((username.to_string(), plain_token.to_string()))
    }

    fn personal_repo(owner_id: Uuid, visibility: RepositoryVisibility) -> Repository {
        Repository {
            visibility,
            ..repository(owner_id)
        }
    }

    /// The owner's token is `fg_valid`.
    struct Fixture {
        owner_id: Uuid,
        repo: Repository,
        use_case: AuthenticateGitRequestUseCase,
        events: Arc<FakeEvents>,
    }

    fn fixture(visibility: RepositoryVisibility) -> Fixture {
        let owner_id = Uuid::new_v4();
        let (use_case, events) = World {
            tokens: vec![api_token(owner_id, "fg_valid")],
            ..World::new()
        }
        .build();
        Fixture {
            owner_id,
            repo: personal_repo(owner_id, visibility),
            use_case,
            events,
        }
    }

    #[tokio::test]
    async fn a_valid_token_grants_write_access_to_its_owners_repository() {
        let f = fixture(RepositoryVisibility::Private);
        let result = f
            .use_case
            .execute(credentials("florian", "fg_valid"), f.repo, GitAccess::Write)
            .await;
        assert!(result.is_ok());
        let (_, resolved_user_id) = result.unwrap();
        assert_eq!(resolved_user_id, Some(f.owner_id));
    }

    #[tokio::test]
    async fn reading_a_public_repository_without_credentials_is_allowed() {
        let f = fixture(RepositoryVisibility::Public);
        let result = f.use_case.execute(None, f.repo, GitAccess::Read).await;
        assert!(result.is_ok());
        let (_, resolved_user_id) = result.unwrap();
        assert_eq!(resolved_user_id, None);
    }

    #[tokio::test]
    async fn reading_a_private_repository_without_credentials_is_unauthorized() {
        let f = fixture(RepositoryVisibility::Private);
        let result = f.use_case.execute(None, f.repo, GitAccess::Read).await;
        assert!(matches!(result, Err(DomainError::Unauthorized(_))));

        let published = f.events.security_events();
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
        let f = fixture(RepositoryVisibility::Private);
        let result = f
            .use_case
            .execute(credentials("florian", "fg_wrong"), f.repo, GitAccess::Read)
            .await;
        assert!(matches!(result, Err(DomainError::Unauthorized(_))));

        let published = f.events.security_events();
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
        let repo = personal_repo(Uuid::new_v4(), RepositoryVisibility::Public);
        let (use_case, _) = World {
            tokens: vec![api_token(Uuid::new_v4(), "fg_theirs")],
            ..World::new()
        }
        .build();

        let result = use_case
            .execute(
                credentials("someone-else", "fg_theirs"),
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
        let repo = personal_repo(Uuid::new_v4(), RepositoryVisibility::Private);
        let collaborator_id = Uuid::new_v4();
        let (use_case, _) = World {
            tokens: vec![api_token(collaborator_id, "fg_collab")],
            collaborators: vec![(repo.id, collaborator_id, CollaboratorRole::Contributor)],
            ..World::new()
        }
        .build();

        let result = use_case
            .execute(credentials("collab", "fg_collab"), repo, GitAccess::Write)
            .await;
        assert!(
            result.is_ok(),
            "a listed collaborator's token should grant write access even though they don't own the repository, got {result:?}"
        );
    }

    #[tokio::test]
    async fn a_valid_token_belonging_to_neither_the_owner_nor_a_collaborator_is_unauthorized() {
        let repo = personal_repo(Uuid::new_v4(), RepositoryVisibility::Private);
        // The stranger is not a collaborator, which is the point of this test.
        let (use_case, _) = World {
            tokens: vec![api_token(Uuid::new_v4(), "fg_stranger")],
            ..World::new()
        }
        .build();

        let result = use_case
            .execute(
                credentials("stranger", "fg_stranger"),
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
        let repo = personal_repo(Uuid::new_v4(), RepositoryVisibility::Private);
        let collaborator_id = Uuid::new_v4();
        let token = api_token(collaborator_id, "fg_collab");

        let (while_still_a_collaborator, _) = World {
            tokens: vec![token.clone()],
            collaborators: vec![(repo.id, collaborator_id, CollaboratorRole::Contributor)],
            ..World::new()
        }
        .build();
        let before_removal = while_still_a_collaborator
            .execute(
                credentials("collab", "fg_collab"),
                repo.clone(),
                GitAccess::Write,
            )
            .await;
        assert!(
            before_removal.is_ok(),
            "expected write access while still a listed collaborator, got {before_removal:?}"
        );

        // Same pair, now absent, as after a real removal.
        let (after_removal, _) = World {
            tokens: vec![token],
            ..World::new()
        }
        .build();
        let result_after_removal = after_removal
            .execute(credentials("collab", "fg_collab"), repo, GitAccess::Write)
            .await;
        assert!(
            matches!(result_after_removal, Err(DomainError::Unauthorized(_))),
            "expected write access to be denied once the collaborator row is gone, got {result_after_removal:?}"
        );
    }

    /// A private repository on which `reader` is a Reader collaborator, with the token `fg_reader`.
    fn reader_collaborator() -> (Repository, AuthenticateGitRequestUseCase) {
        let repo = personal_repo(Uuid::new_v4(), RepositoryVisibility::Private);
        let reader_id = Uuid::new_v4();
        let (use_case, _) = World {
            tokens: vec![api_token(reader_id, "fg_reader")],
            collaborators: vec![(repo.id, reader_id, CollaboratorRole::Reader)],
            ..World::new()
        }
        .build();
        (repo, use_case)
    }

    #[tokio::test]
    async fn a_reader_role_collaborator_can_read_but_not_write() {
        let (repo, use_case) = reader_collaborator();

        let read_result = use_case
            .execute(credentials("florian", "fg_reader"), repo, GitAccess::Read)
            .await;
        assert!(read_result.is_ok());
    }

    #[tokio::test]
    async fn a_reader_role_collaborator_cannot_write() {
        let (repo, use_case) = reader_collaborator();

        let write_result = use_case
            .execute(credentials("florian", "fg_reader"), repo, GitAccess::Write)
            .await;
        assert!(matches!(write_result, Err(DomainError::Unauthorized(_))));
    }

    /// The runner's token is `fgr_valid`.
    fn runner_use_case() -> (AuthenticateGitRequestUseCase, Arc<FakeEvents>) {
        World {
            runners: vec![runner("fgr_valid")],
            ..World::new()
        }
        .build()
    }

    #[tokio::test]
    async fn a_valid_runner_token_reads_a_private_repository_it_does_not_own() {
        let (use_case, _) = runner_use_case();
        let repo = personal_repo(Uuid::new_v4(), RepositoryVisibility::Private);

        let result = use_case
            .execute(
                credentials("any-runner-name", "fgr_valid"),
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
        let (use_case, events) = runner_use_case();
        let repo = personal_repo(Uuid::new_v4(), RepositoryVisibility::Private);

        let result = use_case
            .execute(
                credentials("any-runner-name", "fgr_valid"),
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

    /// A private repository `name` in `group_id`, created by `creator_id`.
    fn group_repo(creator_id: Uuid, group_id: Uuid, name: &str) -> Repository {
        Repository {
            group_id: Some(group_id),
            name: name.to_string(),
            ..repository(creator_id)
        }
    }

    #[tokio::test]
    async fn a_reader_on_an_ancestor_group_can_read_a_group_repository_with_no_direct_grant() {
        let creator_id = Uuid::new_v4();
        let reader_id = Uuid::new_v4();
        let acme = group(None, "acme");
        let groups = FakeGroups::new(vec![acme.clone()]);
        groups
            .add_member(acme.id, reader_id, CollaboratorRole::Reader)
            .await
            .unwrap();
        let repo = group_repo(creator_id, acme.id, "backend");
        // The reader has no direct collaborator grant, only the group role.
        let (use_case, _) = World {
            tokens: vec![api_token(reader_id, "fg_reader")],
            groups,
            ..World::new()
        }
        .build();

        let result = use_case
            .execute(credentials("reader", "fg_reader"), repo, GitAccess::Read)
            .await;
        assert!(
            result.is_ok(),
            "a group Reader should be able to read a repository in that group with no direct grant, got {result:?}"
        );
    }

    #[tokio::test]
    async fn a_contributor_two_levels_up_can_push_to_a_repository_three_levels_deep() {
        let creator_id = Uuid::new_v4();
        let contributor_id = Uuid::new_v4();
        let root = group(None, "acme");
        let mid = group(Some(root.id), "backend");
        let leaf = group(Some(mid.id), "infra");
        let repo = group_repo(creator_id, leaf.id, "terraform-modules");
        let groups = FakeGroups::new(vec![root.clone(), mid, leaf]);
        groups
            .add_member(root.id, contributor_id, CollaboratorRole::Contributor)
            .await
            .unwrap();
        let (use_case, _) = World {
            tokens: vec![api_token(contributor_id, "fg_contrib")],
            groups,
            ..World::new()
        }
        .build();

        let result = use_case
            .execute(
                credentials("contributor", "fg_contrib"),
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
        let creator_id = Uuid::new_v4();
        let acme = group(None, "acme");
        let repo = group_repo(creator_id, acme.id, "backend");
        let (use_case, _) = World {
            tokens: vec![api_token(creator_id, "fg_creator")],
            groups: FakeGroups::new(vec![acme]),
            ..World::new()
        }
        .build();

        let read_result = use_case
            .execute(
                credentials("creator", "fg_creator"),
                repo.clone(),
                GitAccess::Read,
            )
            .await;
        assert!(
            matches!(read_result, Err(DomainError::Unauthorized(_))),
            "the repo creator must NOT get an implicit read bypass on a group repository, got {read_result:?}"
        );

        let write_result = use_case
            .execute(credentials("creator", "fg_creator"), repo, GitAccess::Write)
            .await;
        assert!(
            matches!(write_result, Err(DomainError::Unauthorized(_))),
            "the repo creator must NOT get an implicit write bypass on a group repository, got {write_result:?}"
        );
    }

    #[tokio::test]
    async fn removing_a_users_group_role_revokes_git_access_on_the_next_request() {
        // Like the collaborator re-evaluation test, for group membership.
        let creator_id = Uuid::new_v4();
        let member_id = Uuid::new_v4();
        let token = api_token(member_id, "fg_member");
        let acme = group(None, "acme");
        let repo = group_repo(creator_id, acme.id, "backend");

        let groups_while_a_member = FakeGroups::new(vec![acme.clone()]);
        groups_while_a_member
            .add_member(acme.id, member_id, CollaboratorRole::Contributor)
            .await
            .unwrap();
        let (while_still_a_member, _) = World {
            tokens: vec![token.clone()],
            groups: groups_while_a_member,
            ..World::new()
        }
        .build();
        let before_removal = while_still_a_member
            .execute(
                credentials("member", "fg_member"),
                repo.clone(),
                GitAccess::Write,
            )
            .await;
        assert!(
            before_removal.is_ok(),
            "expected write access while still a group Contributor, got {before_removal:?}"
        );

        let (after_removal, _) = World {
            tokens: vec![token],
            groups: FakeGroups::new(vec![acme]),
            ..World::new()
        }
        .build();
        let result_after_removal = after_removal
            .execute(credentials("member", "fg_member"), repo, GitAccess::Write)
            .await;
        assert!(
            matches!(result_after_removal, Err(DomainError::Unauthorized(_))),
            "expected write access to be denied once the group-member row is gone, got {result_after_removal:?}"
        );
    }

    /// A public repository, and a stranger (no role anywhere) whose token is `fg_stranger`.
    fn public_repo_use_case(
        settings: Arc<dyn PublicPagesSettingsPort>,
    ) -> (Repository, AuthenticateGitRequestUseCase, Arc<FakeEvents>) {
        let repo = personal_repo(Uuid::new_v4(), RepositoryVisibility::Public);
        let (use_case, events) = World {
            tokens: vec![api_token(Uuid::new_v4(), "fg_stranger")],
            public_pages: settings,
            ..World::new()
        }
        .build();
        (repo, use_case, events)
    }

    #[tokio::test]
    async fn anonymous_read_of_a_public_repository_is_refused_when_public_pages_are_off() {
        let (repo, use_case, events) = public_repo_use_case(public_pages(false));

        let result = use_case.execute(None, repo, GitAccess::Read).await;

        assert!(matches!(result, Err(DomainError::Unauthorized(_))));
        let published = events.security_events();
        assert!(
            matches!(&published[..], [(SecurityEvent::GitAccessDenied { username, .. }, None)] if username == "anonymous"),
            "the refusal must be audited like a private repository's, got {published:?}"
        );
    }

    #[tokio::test]
    async fn an_unknown_token_cannot_read_a_public_repository_when_public_pages_are_off() {
        let (repo, use_case, _) = public_repo_use_case(public_pages(false));

        let result = use_case
            .execute(credentials("nobody", "fg_unknown"), repo, GitAccess::Read)
            .await;

        assert!(matches!(result, Err(DomainError::Unauthorized(_))));
    }

    #[tokio::test]
    async fn a_signed_in_user_without_a_role_still_reads_a_public_repository_when_public_pages_are_off()
     {
        let (repo, use_case, _) = public_repo_use_case(public_pages(false));

        let (_, user_id) = use_case
            .execute(
                credentials("stranger", "fg_stranger"),
                repo,
                GitAccess::Read,
            )
            .await
            .unwrap();

        assert!(user_id.is_some());
    }

    #[tokio::test]
    async fn a_signed_in_user_without_a_role_cannot_write_to_a_public_repository() {
        let (repo, use_case, _) = public_repo_use_case(public_pages(false));

        let result = use_case
            .execute(
                credentials("stranger", "fg_stranger"),
                repo,
                GitAccess::Write,
            )
            .await;

        assert!(matches!(result, Err(DomainError::Unauthorized(_))));
    }

    #[tokio::test]
    async fn anonymous_read_of_a_public_repository_is_refused_when_the_setting_cannot_be_read() {
        let (repo, use_case, _) = public_repo_use_case(Arc::new(BrokenPublicPagesSettings));

        let result = use_case.execute(None, repo, GitAccess::Read).await;

        assert!(matches!(result, Err(DomainError::Unauthorized(_))));
    }
}
