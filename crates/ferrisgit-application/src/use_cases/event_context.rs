use ferrisgit_domain::notification::{NewNotification, NotificationKind};
use ferrisgit_domain::repository::{Repository, RepositoryStorePort};
use ferrisgit_domain::user::UserRepositoryPort;
use uuid::Uuid;

/// The repository, its owner and the actor: what a webhook payload and its notifications need to name.
///
/// Webhooks and notifications come after an action that already succeeded, so every lookup is best effort: a missing
/// row or a store error gives `None` and the side effects are skipped, never a failure.
pub(crate) struct EventContext {
    pub repository: Repository,
    pub owner_username: String,
    pub actor_username: String,
}

impl EventContext {
    pub(crate) async fn load(
        repositories: &dyn RepositoryStorePort,
        users: &dyn UserRepositoryPort,
        repository_id: Uuid,
        actor_id: Uuid,
    ) -> Option<Self> {
        let repository = repositories
            .find_by_id(repository_id)
            .await
            .ok()
            .flatten()?;
        Self::for_repository(users, repository, actor_id).await
    }

    pub(crate) async fn for_repository(
        users: &dyn UserRepositoryPort,
        repository: Repository,
        actor_id: Uuid,
    ) -> Option<Self> {
        let owner = users.find_by_id(repository.owner_id).await.ok().flatten()?;
        let actor = users.find_by_id(actor_id).await.ok().flatten()?;
        Some(Self {
            repository,
            owner_username: owner.username,
            actor_username: actor.username,
        })
    }

    /// A notification with only the repository and actor filled in; callers add the rest with struct update syntax.
    pub(crate) fn notification(
        &self,
        kind: NotificationKind,
        recipient_id: Uuid,
    ) -> NewNotification {
        NewNotification {
            recipient_id,
            kind,
            repository_owner: self.owner_username.clone(),
            repository_name: self.repository.name.clone(),
            actor_username: Some(self.actor_username.clone()),
            merge_request_id: None,
            merge_request_title: None,
            pipeline_id: None,
            commit_sha: None,
            issue_id: None,
            issue_number: None,
            issue_title: None,
            role: None,
        }
    }
}
