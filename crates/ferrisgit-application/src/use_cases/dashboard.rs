use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::GroupStorePort;
use ferrisgit_domain::issue::{Issue, IssueStorePort};
use ferrisgit_domain::merge_request::{MergeRequest, MergeRequestStorePort};
use ferrisgit_domain::notification::{Notification, NotificationStorePort};
use ferrisgit_domain::repository::RepositoryStorePort;
use ferrisgit_domain::repository_collaborator::RepositoryCollaboratorStorePort;
use uuid::Uuid;

use crate::access::{member_repository_ids, visible_repository_ids};

const ITEMS_PER_CATEGORY: i64 = 20;
const ACTIVITY_LIMIT: i64 = 20;

pub struct DashboardResults {
    pub assigned_issues: Vec<Issue>,
    pub authored_issues: Vec<Issue>,
    pub authored_merge_requests: Vec<MergeRequest>,
    pub merge_requests_to_review: Vec<MergeRequest>,
    pub activity: Vec<Notification>,
}

pub struct DashboardUseCase {
    repositories: Arc<dyn RepositoryStorePort>,
    repository_collaborators: Arc<dyn RepositoryCollaboratorStorePort>,
    groups: Arc<dyn GroupStorePort>,
    issues: Arc<dyn IssueStorePort>,
    merge_requests: Arc<dyn MergeRequestStorePort>,
    notifications: Arc<dyn NotificationStorePort>,
}

impl DashboardUseCase {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repositories: Arc<dyn RepositoryStorePort>,
        repository_collaborators: Arc<dyn RepositoryCollaboratorStorePort>,
        groups: Arc<dyn GroupStorePort>,
        issues: Arc<dyn IssueStorePort>,
        merge_requests: Arc<dyn MergeRequestStorePort>,
        notifications: Arc<dyn NotificationStorePort>,
    ) -> Self {
        Self {
            repositories,
            repository_collaborators,
            groups,
            issues,
            merge_requests,
            notifications,
        }
    }

    pub async fn execute(&self, user_id: Uuid) -> Result<DashboardResults, DomainError> {
        let visible_ids = visible_repository_ids(
            &self.repositories,
            &self.repository_collaborators,
            &self.groups,
            user_id,
        )
        .await?;
        let member_ids = member_repository_ids(
            &self.repositories,
            &self.repository_collaborators,
            &self.groups,
            user_id,
        )
        .await?;

        let (
            assigned_issues,
            authored_issues,
            authored_merge_requests,
            merge_requests_to_review,
            activity,
        ) = tokio::try_join!(
            self.issues
                .list_assigned_to(user_id, &visible_ids, ITEMS_PER_CATEGORY),
            self.issues
                .list_authored_by(user_id, &visible_ids, ITEMS_PER_CATEGORY),
            self.merge_requests
                .list_authored_by(user_id, &visible_ids, ITEMS_PER_CATEGORY),
            self.merge_requests
                .list_awaiting_review_by(user_id, &member_ids, ITEMS_PER_CATEGORY),
            self.notifications
                .list_for_recipient(user_id, ACTIVITY_LIMIT),
        )?;

        Ok(DashboardResults {
            assigned_issues,
            authored_issues,
            authored_merge_requests,
            merge_requests_to_review,
            activity,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        FakeCollaborators, FakeGroups, FakeIssues, FakeMergeRequests, FakeNotifications,
        FakeRepositories,
    };
    use chrono::Utc;
    use ferrisgit_domain::issue::IssueKind;
    use ferrisgit_domain::repository::{Repository, RepositoryVisibility};
    use ferrisgit_domain::repository_collaborator::CollaboratorRole;

    fn repo(id: Uuid, owner_id: Uuid, group_id: Option<Uuid>) -> Repository {
        Repository {
            id,
            owner_id,
            name: "r".to_string(),
            group_id,
            description: String::new(),
            disk_path: "r.git".to_string(),
            visibility: RepositoryVisibility::Private,
            created_at: Utc::now(),
        }
    }

    fn public_repo(id: Uuid, owner_id: Uuid) -> Repository {
        Repository {
            id,
            owner_id,
            name: "r".to_string(),
            group_id: None,
            description: String::new(),
            disk_path: "r.git".to_string(),
            visibility: RepositoryVisibility::Public,
            created_at: Utc::now(),
        }
    }

    fn fake_issue(title: &str) -> Issue {
        Issue {
            id: Uuid::new_v4(),
            repository_id: Uuid::new_v4(),
            number: 1,
            author_id: Uuid::new_v4(),
            assignee_id: None,
            milestone_id: None,
            title: title.to_string(),
            description: String::new(),
            status: ferrisgit_domain::issue::IssueStatus::Todo,
            kind: IssueKind::Bug,
            parent_issue_id: None,
            created_at: chrono::Utc::now(),
            closed_at: None,
        }
    }

    fn fake_merge_request(title: &str) -> MergeRequest {
        MergeRequest {
            id: Uuid::new_v4(),
            repository_id: Uuid::new_v4(),
            author_id: Some(Uuid::new_v4()),
            source_branch: "feature".to_string(),
            target_branch: "main".to_string(),
            title: title.to_string(),
            description: String::new(),
            status: ferrisgit_domain::merge_request::MergeRequestStatus::Open,
            merge_commit_sha: None,
            milestone_id: None,
            created_at: chrono::Utc::now(),
            closed_at: None,
        }
    }

    /// The shared `FakeNotifications` filters by recipient, so tests that care about dashboard activity must seed one
    /// for the user under test.
    fn fake_notification(recipient_id: Uuid) -> Notification {
        Notification {
            id: Uuid::new_v4(),
            recipient_id,
            kind: ferrisgit_domain::notification::NotificationKind::IssueClosed,
            repository_owner: "alice".to_string(),
            repository_name: "hello".to_string(),
            actor_username: Some("bob".to_string()),
            merge_request_id: None,
            merge_request_title: None,
            pipeline_id: None,
            commit_sha: None,
            issue_id: Some(Uuid::new_v4()),
            issue_number: Some(3),
            issue_title: Some("Fixed".to_string()),
            role: None,
            read_at: None,
            created_at: chrono::Utc::now(),
        }
    }

    fn use_case(
        repositories: Vec<Repository>,
        collaborator_rows: Vec<(Uuid, Uuid, CollaboratorRole)>,
        issues: Vec<Issue>,
        notifications: Vec<Notification>,
        merge_requests: Vec<MergeRequest>,
    ) -> (DashboardUseCase, Arc<FakeIssues>, Arc<FakeMergeRequests>) {
        let issues = Arc::new(FakeIssues::new(issues));
        let merge_requests = Arc::new(FakeMergeRequests::new(merge_requests));
        let use_case = DashboardUseCase::new(
            Arc::new(FakeRepositories::new(repositories)),
            Arc::new(FakeCollaborators::new(collaborator_rows)),
            Arc::new(FakeGroups::empty()),
            issues.clone(),
            merge_requests.clone(),
            Arc::new(FakeNotifications::new(notifications)),
        );
        (use_case, issues, merge_requests)
    }

    #[tokio::test]
    async fn execute_scopes_the_assigned_issues_query_to_the_visible_repository_ids() {
        let user_id = Uuid::new_v4();
        let owned_id = Uuid::new_v4();
        let collaborated_id = Uuid::new_v4();

        let repositories = vec![
            repo(owned_id, user_id, None),
            repo(collaborated_id, Uuid::new_v4(), None),
        ];
        let collaborator_rows = vec![(collaborated_id, user_id, CollaboratorRole::Contributor)];

        let mut assigned_issue = fake_issue("assigned to me");
        assigned_issue.repository_id = owned_id;
        assigned_issue.assignee_id = Some(user_id);

        let (use_case, issues, _merge_requests) = use_case(
            repositories,
            collaborator_rows,
            vec![assigned_issue],
            vec![],
            vec![],
        );

        let result = use_case.execute(user_id).await.unwrap();

        assert_eq!(result.assigned_issues.len(), 1);
        assert_eq!(result.assigned_issues[0].title, "assigned to me");

        let calls = issues.list_assigned_to_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, user_id);

        let mut actual_ids = calls[0].1.clone();
        actual_ids.sort();
        let mut expected_ids = vec![owned_id, collaborated_id];
        expected_ids.sort();
        assert_eq!(actual_ids, expected_ids);
    }

    #[tokio::test]
    async fn execute_populates_authored_issues_from_list_authored_by() {
        let user_id = Uuid::new_v4();
        let repository_id = Uuid::new_v4();
        let mut authored_issue = fake_issue("authored by me");
        authored_issue.repository_id = repository_id;
        authored_issue.author_id = user_id;

        let (use_case, _issues, _merge_requests) = use_case(
            vec![repo(repository_id, user_id, None)],
            vec![],
            vec![authored_issue],
            vec![],
            vec![],
        );
        let result = use_case.execute(user_id).await.unwrap();

        assert_eq!(result.authored_issues.len(), 1);
        assert_eq!(result.authored_issues[0].title, "authored by me");
    }

    #[tokio::test]
    async fn execute_populates_authored_merge_requests_from_list_authored_by() {
        let user_id = Uuid::new_v4();
        let repository_id = Uuid::new_v4();
        let mut authored_mr = fake_merge_request("authored by me");
        authored_mr.repository_id = repository_id;
        authored_mr.author_id = Some(user_id);

        let (use_case, _issues, _merge_requests) = use_case(
            vec![repo(repository_id, user_id, None)],
            vec![],
            vec![],
            vec![],
            vec![authored_mr],
        );
        let result = use_case.execute(user_id).await.unwrap();

        assert_eq!(result.authored_merge_requests.len(), 1);
        assert_eq!(result.authored_merge_requests[0].title, "authored by me");
    }

    #[tokio::test]
    async fn execute_populates_merge_requests_to_review_from_list_awaiting_review_by() {
        let user_id = Uuid::new_v4();
        let repository_id = Uuid::new_v4();
        let mut mr_to_review = fake_merge_request("awaiting my review");
        mr_to_review.repository_id = repository_id;
        mr_to_review.author_id = Some(Uuid::new_v4());

        let (use_case, _issues, _merge_requests) = use_case(
            vec![repo(repository_id, user_id, None)],
            vec![],
            vec![],
            vec![],
            vec![mr_to_review],
        );
        let result = use_case.execute(user_id).await.unwrap();

        assert_eq!(result.merge_requests_to_review.len(), 1);
        assert_eq!(
            result.merge_requests_to_review[0].title,
            "awaiting my review"
        );
    }

    #[tokio::test]
    async fn execute_populates_activity_from_notifications() {
        let user_id = Uuid::new_v4();
        let (use_case, _issues, _merge_requests) = use_case(
            vec![],
            vec![],
            vec![],
            vec![fake_notification(user_id)],
            vec![],
        );
        let result = use_case.execute(user_id).await.unwrap();

        assert_eq!(result.activity.len(), 1);
        assert_eq!(result.activity[0].repository_name, "hello");
    }

    #[tokio::test]
    async fn execute_scopes_the_awaiting_review_query_to_membership_only_excluding_public_repos() {
        let user_id = Uuid::new_v4();
        let owned_id = Uuid::new_v4();
        let public_id = Uuid::new_v4();

        let repositories = vec![
            repo(owned_id, user_id, None),
            public_repo(public_id, Uuid::new_v4()),
        ];
        let (use_case, _issues, merge_requests) =
            use_case(repositories, vec![], vec![], vec![], vec![]);

        use_case.execute(user_id).await.unwrap();

        let calls = merge_requests.list_awaiting_review_by_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, user_id);
        assert_eq!(
            calls[0].1,
            vec![owned_id],
            "the public repo must be excluded from the awaiting-review scope even though it's visible for other categories"
        );
    }
}
