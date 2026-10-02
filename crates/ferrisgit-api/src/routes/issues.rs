use axum::extract::{Path, Query, State};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use ferrisgit_application::use_cases::add_issue_comment::AddIssueCommentUseCase;
use ferrisgit_application::use_cases::assign_issue::AssignIssueUseCase;
use ferrisgit_application::use_cases::close_issue::{CloseIssueUseCase, ReopenIssueUseCase};
use ferrisgit_application::use_cases::create_issue::CreateIssueUseCase;
use ferrisgit_application::use_cases::move_issue_status::MoveIssueStatusUseCase;
use ferrisgit_application::use_cases::set_issue_labels::SetIssueLabelsUseCase;
use ferrisgit_application::use_cases::update_issue::UpdateIssueUseCase;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::issue::{Issue, IssueComment, IssueKind, IssueStatus};
use ferrisgit_domain::repository::Repository;
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth_middleware::AuthUser;
use crate::authz::require_role_by_id;
use crate::error::ApiError;
use crate::routes::labels::{LabelFilterQuery, LabelResponse, SetLabelsRequest};
use crate::routes::user_ref::{UserRef, load_user_refs};
use crate::state::AppState;

async fn find_accessible_issue(
    state: &AppState,
    user_id: Uuid,
    repository_id: Uuid,
    number: i32,
    min_role: CollaboratorRole,
) -> Result<(Issue, Repository), DomainError> {
    let repo = require_role_by_id(state, user_id, repository_id, min_role).await?;
    let issue = state
        .issues
        .find_by_number(repo.id, number)
        .await?
        .ok_or_else(|| DomainError::NotFound("issue".to_string()))?;
    Ok((issue, repo))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IssueResponse {
    id: Uuid,
    number: i32,
    author_id: Uuid,
    assignee_id: Option<Uuid>,
    milestone_id: Option<Uuid>,
    title: String,
    description: String,
    status: String,
    kind: String,
    parent_issue_id: Option<Uuid>,
    created_at: DateTime<Utc>,
    closed_at: Option<DateTime<Utc>>,
    labels: Vec<LabelResponse>,
    author: Option<UserRef>,
    assignee: Option<UserRef>,
    comment_count: i64,
}

impl From<Issue> for IssueResponse {
    fn from(issue: Issue) -> Self {
        IssueResponse {
            id: issue.id,
            number: issue.number,
            author_id: issue.author_id,
            assignee_id: issue.assignee_id,
            milestone_id: issue.milestone_id,
            title: issue.title,
            description: issue.description,
            status: issue.status.as_str().to_string(),
            kind: issue.kind.as_str().to_string(),
            parent_issue_id: issue.parent_issue_id,
            created_at: issue.created_at,
            closed_at: issue.closed_at,
            labels: Vec::new(),
            author: None,
            assignee: None,
            comment_count: 0,
        }
    }
}

impl IssueResponse {
    /// Every handler that returns issues builds them here, so the fields stay the same.
    async fn build_many(state: &AppState, issues: Vec<Issue>) -> Result<Vec<Self>, DomainError> {
        let ids: Vec<Uuid> = issues.iter().map(|i| i.id).collect();
        let labels_by_issue = state.labels.list_for_issues(&ids).await?;
        let comment_counts = state.issue_comments.comment_counts(&ids).await?;
        let users = load_user_refs(
            state,
            issues
                .iter()
                .flat_map(|i| std::iter::once(i.author_id).chain(i.assignee_id)),
        )
        .await;
        Ok(issues
            .into_iter()
            .map(|issue| {
                let labels = labels_by_issue
                    .iter()
                    .filter(|(id, _)| *id == issue.id)
                    .map(|(_, l)| l.clone().into())
                    .collect();
                let author = users.get(&issue.author_id).cloned();
                let assignee = issue.assignee_id.and_then(|id| users.get(&id).cloned());
                let comment_count = comment_counts.get(&issue.id).copied().unwrap_or(0);
                IssueResponse {
                    labels,
                    author,
                    assignee,
                    comment_count,
                    ..issue.into()
                }
            })
            .collect())
    }

    async fn build(state: &AppState, issue: Issue) -> Result<Self, DomainError> {
        let mut built = Self::build_many(state, vec![issue]).await?;
        Ok(built.remove(0))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CommentResponse {
    id: Uuid,
    author_id: Uuid,
    author: Option<UserRef>,
    body: String,
    created_at: DateTime<Utc>,
}

impl CommentResponse {
    fn from_comment(c: IssueComment, author: Option<UserRef>) -> Self {
        CommentResponse {
            id: c.id,
            author_id: c.author_id,
            author,
            body: c.body,
            created_at: c.created_at,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateIssueRequest {
    title: String,
    description: String,
    kind: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateIssueRequest {
    title: String,
    description: String,
    kind: String,
    /// Double `Option` so an absent field (`None`) differs from an explicit `null` (`Some(None)`, which clears it).
    /// With a plain `Option`, leaving out `milestoneId` would silently clear the milestone.
    #[serde(default, deserialize_with = "crate::routes::deserialize_present")]
    milestone_id: Option<Option<Uuid>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateStatusRequest {
    status: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssignRequest {
    assignee_id: Option<Uuid>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddCommentRequest {
    body: String,
}

async fn list_for_repository(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(repository_id): Path<Uuid>,
    Query(query): Query<LabelFilterQuery>,
) -> Result<Json<Vec<IssueResponse>>, ApiError> {
    let repo = require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Reader).await?;
    let label_ids = query.label_ids()?;
    let issues = state
        .issues
        .list_for_repository_filtered(repo.id, label_ids, query.milestone_id)
        .await?;
    Ok(Json(IssueResponse::build_many(&state, issues).await?))
}

async fn create(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(repository_id): Path<Uuid>,
    Json(req): Json<CreateIssueRequest>,
) -> Result<Json<IssueResponse>, ApiError> {
    let repo = require_role_by_id(
        &state,
        user_id,
        repository_id,
        CollaboratorRole::Contributor,
    )
    .await?;
    let kind = IssueKind::parse(&req.kind)?;
    let use_case = CreateIssueUseCase::new(state.issues.clone());
    let issue = use_case
        .execute(repo.id, user_id, req.title, req.description, kind)
        .await?;
    Ok(Json(IssueResponse::build(&state, issue).await?))
}

async fn detail(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((repository_id, number)): Path<(Uuid, i32)>,
) -> Result<Json<IssueResponse>, ApiError> {
    let (issue, _repo) = find_accessible_issue(
        &state,
        user_id,
        repository_id,
        number,
        CollaboratorRole::Reader,
    )
    .await?;
    Ok(Json(IssueResponse::build(&state, issue).await?))
}

async fn update(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((repository_id, number)): Path<(Uuid, i32)>,
    Json(req): Json<UpdateIssueRequest>,
) -> Result<Json<IssueResponse>, ApiError> {
    let (issue, _repo) = find_accessible_issue(
        &state,
        user_id,
        repository_id,
        number,
        CollaboratorRole::Contributor,
    )
    .await?;
    let kind = IssueKind::parse(&req.kind)?;
    let milestone_id = req.milestone_id.ok_or_else(|| {
        DomainError::Validation(
            "milestoneId is required on an issue update; send null to clear it".to_string(),
        )
    })?;
    let use_case = UpdateIssueUseCase::new(
        state.issues.clone(),
        state.repositories.clone(),
        state.milestones.clone(),
        state.groups.clone(),
    );
    let updated = use_case
        .execute(issue.id, req.title, req.description, kind, milestone_id)
        .await?;
    Ok(Json(IssueResponse::build(&state, updated).await?))
}

async fn update_status(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((repository_id, number)): Path<(Uuid, i32)>,
    Json(req): Json<UpdateStatusRequest>,
) -> Result<Json<IssueResponse>, ApiError> {
    let (issue, _repo) = find_accessible_issue(
        &state,
        user_id,
        repository_id,
        number,
        CollaboratorRole::Contributor,
    )
    .await?;
    let status = IssueStatus::parse(&req.status)?;
    let use_case = MoveIssueStatusUseCase::new(state.issues.clone());
    let moved = use_case.execute(issue.id, status).await?;
    Ok(Json(IssueResponse::build(&state, moved).await?))
}

async fn assign(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((repository_id, number)): Path<(Uuid, i32)>,
    Json(req): Json<AssignRequest>,
) -> Result<Json<IssueResponse>, ApiError> {
    let (issue, _repo) = find_accessible_issue(
        &state,
        user_id,
        repository_id,
        number,
        CollaboratorRole::Contributor,
    )
    .await?;
    let use_case = AssignIssueUseCase::new(
        state.issues.clone(),
        state.repositories.clone(),
        state.repository_collaborators.clone(),
        state.groups.clone(),
        state.group_membership.clone(),
        state.users.clone(),
        state.notifications.clone(),
        state.webhooks.clone(),
    );
    let assigned = use_case.execute(issue.id, user_id, req.assignee_id).await?;
    Ok(Json(IssueResponse::build(&state, assigned).await?))
}

async fn close(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((repository_id, number)): Path<(Uuid, i32)>,
) -> Result<Json<IssueResponse>, ApiError> {
    let (issue, _repo) = find_accessible_issue(
        &state,
        user_id,
        repository_id,
        number,
        CollaboratorRole::Contributor,
    )
    .await?;
    let use_case = CloseIssueUseCase::new(
        state.issues.clone(),
        state.repositories.clone(),
        state.users.clone(),
        state.notifications.clone(),
        state.webhooks.clone(),
    );
    let closed = use_case.execute(issue.id, user_id).await?;
    Ok(Json(IssueResponse::build(&state, closed).await?))
}

async fn reopen(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((repository_id, number)): Path<(Uuid, i32)>,
) -> Result<Json<IssueResponse>, ApiError> {
    let (issue, _repo) = find_accessible_issue(
        &state,
        user_id,
        repository_id,
        number,
        CollaboratorRole::Contributor,
    )
    .await?;
    let use_case = ReopenIssueUseCase::new(state.issues.clone());
    let reopened = use_case.execute(issue.id).await?;
    Ok(Json(IssueResponse::build(&state, reopened).await?))
}

async fn set_labels(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((repository_id, number)): Path<(Uuid, i32)>,
    Json(req): Json<SetLabelsRequest>,
) -> Result<Json<Vec<LabelResponse>>, ApiError> {
    let (issue, repo) = find_accessible_issue(
        &state,
        user_id,
        repository_id,
        number,
        CollaboratorRole::Contributor,
    )
    .await?;
    let use_case = SetIssueLabelsUseCase::new(
        state.issues.clone(),
        state.labels.clone(),
        state.groups.clone(),
    );
    let labels = use_case.execute(issue.id, &repo, req.label_ids).await?;
    Ok(Json(labels.into_iter().map(Into::into).collect()))
}

async fn list_comments(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((repository_id, number)): Path<(Uuid, i32)>,
) -> Result<Json<Vec<CommentResponse>>, ApiError> {
    let (issue, _repo) = find_accessible_issue(
        &state,
        user_id,
        repository_id,
        number,
        CollaboratorRole::Reader,
    )
    .await?;
    let comments = state.issue_comments.list_comments(issue.id).await?;
    let authors = load_user_refs(&state, comments.iter().map(|c| c.author_id)).await;
    Ok(Json(
        comments
            .into_iter()
            .map(|c| {
                let author = authors.get(&c.author_id).cloned();
                CommentResponse::from_comment(c, author)
            })
            .collect(),
    ))
}

async fn add_comment(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((repository_id, number)): Path<(Uuid, i32)>,
    Json(req): Json<AddCommentRequest>,
) -> Result<Json<CommentResponse>, ApiError> {
    let (issue, _repo) = find_accessible_issue(
        &state,
        user_id,
        repository_id,
        number,
        CollaboratorRole::Contributor,
    )
    .await?;
    let use_case = AddIssueCommentUseCase::new(
        state.issues.clone(),
        state.issue_comments.clone(),
        state.repositories.clone(),
        state.users.clone(),
        state.notifications.clone(),
        state.webhooks.clone(),
    );
    let comment = use_case.execute(issue.id, user_id, req.body).await?;
    let author = load_user_refs(&state, [comment.author_id])
        .await
        .remove(&comment.author_id);
    Ok(Json(CommentResponse::from_comment(comment, author)))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/repositories/{repository_id}/issues",
            get(list_for_repository).post(create),
        )
        .route(
            "/repositories/{repository_id}/issues/{number}",
            get(detail).patch(update),
        )
        .route(
            "/repositories/{repository_id}/issues/{number}/status",
            patch(update_status),
        )
        .route(
            "/repositories/{repository_id}/issues/{number}/assign",
            post(assign),
        )
        .route(
            "/repositories/{repository_id}/issues/{number}/labels",
            axum::routing::put(set_labels),
        )
        .route(
            "/repositories/{repository_id}/issues/{number}/close",
            post(close),
        )
        .route(
            "/repositories/{repository_id}/issues/{number}/reopen",
            post(reopen),
        )
        .route(
            "/repositories/{repository_id}/issues/{number}/comments",
            get(list_comments).post(add_comment),
        )
}
