use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use ferrisgit_application::merge_request_timeline::{
    ExcerptLine, TimelineItem, assemble_timeline, is_outdated,
};
use ferrisgit_application::use_cases::add_merge_request_comment::AddMergeRequestCommentUseCase;
use ferrisgit_application::use_cases::close_merge_request::CloseMergeRequestUseCase;
use ferrisgit_application::use_cases::create_merge_request::CreateMergeRequestUseCase;
use ferrisgit_application::use_cases::merge_merge_request::{
    MergeMergeRequestResult, MergeMergeRequestUseCase,
};
use ferrisgit_application::use_cases::resolve_merge_request_comment::ResolveMergeRequestCommentUseCase;
use ferrisgit_application::use_cases::set_merge_request_labels::SetMergeRequestLabelsUseCase;
use ferrisgit_application::use_cases::submit_merge_request_review::SubmitMergeRequestReviewUseCase;
use ferrisgit_application::use_cases::update_merge_request::UpdateMergeRequestUseCase;
use ferrisgit_domain::diff::{DiffLineKind, FileChangeKind, FileDiff};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::merge_request::{MergeRequest, MergeRequestStatus, ReviewDecision};
use ferrisgit_domain::merge_request_comment::MergeRequestComment;
use ferrisgit_domain::repository::Repository;
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth_middleware::AuthUser;
use crate::authz::require_role_by_id;
use crate::error::ApiError;
use crate::routes::labels::LabelResponse;
use crate::routes::user_ref::{UserRef, load_user_refs};
use crate::state::AppState;

async fn find_accessible_merge_request(
    state: &AppState,
    user_id: Uuid,
    merge_request_id: Uuid,
    min_role: CollaboratorRole,
) -> Result<(MergeRequest, Repository), DomainError> {
    let mr = state
        .merge_requests
        .find_by_id(merge_request_id)
        .await?
        .ok_or_else(|| DomainError::NotFound("merge request".to_string()))?;
    let repo = require_role_by_id(state, user_id, mr.repository_id, min_role).await?;
    Ok((mr, repo))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BranchResponse {
    name: String,
    tip_sha: String,
    is_default: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MergeRequestResponse {
    id: Uuid,
    source_branch: String,
    target_branch: String,
    title: String,
    description: String,
    status: String,
    merge_commit_sha: Option<String>,
    milestone_id: Option<Uuid>,
    created_at: DateTime<Utc>,
    closed_at: Option<DateTime<Utc>>,
    labels: Vec<LabelResponse>,
    author: Option<UserRef>,
    comment_count: i64,
}

impl From<MergeRequest> for MergeRequestResponse {
    fn from(mr: MergeRequest) -> Self {
        MergeRequestResponse {
            id: mr.id,
            source_branch: mr.source_branch,
            target_branch: mr.target_branch,
            title: mr.title,
            description: mr.description,
            status: mr.status.as_str().to_string(),
            merge_commit_sha: mr.merge_commit_sha,
            milestone_id: mr.milestone_id,
            created_at: mr.created_at,
            closed_at: mr.closed_at,
            labels: Vec::new(),
            author: None,
            comment_count: 0,
        }
    }
}

impl MergeRequestResponse {
    /// Every handler returning a merge request goes through here: the detail page replaces its state with the
    /// response of update/merge, so they must all carry the same fields.
    async fn build_many(
        state: &AppState,
        merge_requests: Vec<MergeRequest>,
    ) -> Result<Vec<Self>, DomainError> {
        let ids: Vec<Uuid> = merge_requests.iter().map(|mr| mr.id).collect();
        let labels_by_mr = state.labels.list_for_merge_requests(&ids).await?;
        let comment_counts = state.merge_request_comments.comment_counts(&ids).await?;
        let authors =
            load_user_refs(state, merge_requests.iter().filter_map(|mr| mr.author_id)).await;
        Ok(merge_requests
            .into_iter()
            .map(|mr| {
                let labels = labels_by_mr
                    .iter()
                    .filter(|(id, _)| *id == mr.id)
                    .map(|(_, l)| l.clone().into())
                    .collect();
                let author = mr.author_id.and_then(|id| authors.get(&id).cloned());
                let comment_count = comment_counts.get(&mr.id).copied().unwrap_or(0);
                MergeRequestResponse {
                    labels,
                    author,
                    comment_count,
                    ..mr.into()
                }
            })
            .collect())
    }

    async fn build(state: &AppState, mr: MergeRequest) -> Result<Self, DomainError> {
        let mut built = Self::build_many(state, vec![mr]).await?;
        Ok(built.remove(0))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SplitDiffRow {
    old_line: Option<u32>,
    old_content: Option<String>,
    new_line: Option<u32>,
    new_content: Option<String>,
    kind: String,
}

#[derive(Serialize)]
struct HunkResponse {
    rows: Vec<SplitDiffRow>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileDiffResponse {
    path: String,
    change: String,
    hunks: Vec<HunkResponse>,
}

fn pair_diff_lines(lines: &[ferrisgit_domain::diff::DiffLine]) -> Vec<SplitDiffRow> {
    use ferrisgit_domain::diff::DiffLineKind;
    let mut rows = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];
        match line.kind {
            DiffLineKind::Context => {
                rows.push(SplitDiffRow {
                    old_line: line.old_line,
                    old_content: Some(line.content.clone()),
                    new_line: line.new_line,
                    new_content: Some(line.content.clone()),
                    kind: "context".to_string(),
                });
                i += 1;
            }
            DiffLineKind::Removed => {
                if let Some(next) = lines.get(i + 1)
                    && next.kind == DiffLineKind::Added
                {
                    rows.push(SplitDiffRow {
                        old_line: line.old_line,
                        old_content: Some(line.content.clone()),
                        new_line: next.new_line,
                        new_content: Some(next.content.clone()),
                        kind: "modified".to_string(),
                    });
                    i += 2;
                } else {
                    rows.push(SplitDiffRow {
                        old_line: line.old_line,
                        old_content: Some(line.content.clone()),
                        new_line: None,
                        new_content: None,
                        kind: "removed".to_string(),
                    });
                    i += 1;
                }
            }
            DiffLineKind::Added => {
                rows.push(SplitDiffRow {
                    old_line: None,
                    old_content: None,
                    new_line: line.new_line,
                    new_content: Some(line.content.clone()),
                    kind: "added".to_string(),
                });
                i += 1;
            }
        }
    }
    rows
}

fn change_kind_str(kind: FileChangeKind) -> &'static str {
    match kind {
        FileChangeKind::Added => "added",
        FileChangeKind::Modified => "modified",
        FileChangeKind::Deleted => "deleted",
        FileChangeKind::Binary => "binary",
    }
}

impl From<FileDiff> for FileDiffResponse {
    fn from(d: FileDiff) -> Self {
        FileDiffResponse {
            path: d.path,
            change: change_kind_str(d.change).to_string(),
            hunks: d
                .hunks
                .into_iter()
                .map(|h| HunkResponse {
                    rows: pair_diff_lines(&h.lines),
                })
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CommentResponse {
    id: Uuid,
    author_id: Option<Uuid>,
    body: String,
    created_at: DateTime<Utc>,
    reply_to_id: Option<Uuid>,
    file_path: Option<String>,
    line_number: Option<i32>,
    end_line: Option<i32>,
    side: Option<String>,
    outdated: bool,
    resolved: bool,
    suggested_content: Option<String>,
    applied_at: Option<DateTime<Utc>>,
    applied_commit_sha: Option<String>,
    author: Option<UserRef>,
}

impl CommentResponse {
    fn from_comment(comment: MergeRequestComment, outdated: bool, author: Option<UserRef>) -> Self {
        CommentResponse {
            id: comment.id,
            author_id: comment.author_id,
            body: comment.body,
            created_at: comment.created_at,
            reply_to_id: comment.reply_to_id,
            file_path: comment.file_path,
            line_number: comment.line_number,
            end_line: comment.end_line,
            side: comment.side.map(|s| s.as_str().to_string()),
            outdated,
            resolved: comment.resolved,
            suggested_content: comment.suggested_content,
            applied_at: comment.applied_at,
            applied_commit_sha: comment.applied_commit_sha,
            author,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExcerptLineResponse {
    line: Option<u32>,
    kind: &'static str,
    content: String,
}

impl From<ExcerptLine> for ExcerptLineResponse {
    fn from(line: ExcerptLine) -> Self {
        let kind = match line.kind {
            DiffLineKind::Context => "context",
            DiffLineKind::Added => "added",
            DiffLineKind::Removed => "removed",
        };
        ExcerptLineResponse {
            line: line.line,
            kind,
            content: line.content,
        }
    }
}

// Built once per timeline entry and serialized straight away: boxing the large variant would only add an allocation.
#[allow(clippy::large_enum_variant)]
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum TimelineItemResponse {
    #[serde(rename_all = "camelCase")]
    Comment {
        id: Uuid,
        created_at: DateTime<Utc>,
        author: Option<UserRef>,
        body: String,
    },
    #[serde(rename_all = "camelCase")]
    Thread {
        id: Uuid,
        created_at: DateTime<Utc>,
        file_path: Option<String>,
        line_number: Option<i32>,
        end_line: Option<i32>,
        side: Option<&'static str>,
        outdated: bool,
        resolved: bool,
        resolved_by: Option<UserRef>,
        resolved_at: Option<DateTime<Utc>>,
        excerpt: Vec<ExcerptLineResponse>,
        root: CommentResponse,
        replies: Vec<CommentResponse>,
    },
    #[serde(rename_all = "camelCase")]
    Event {
        id: Uuid,
        created_at: DateTime<Utc>,
        actor: Option<UserRef>,
        kind: &'static str,
        payload: serde_json::Value,
    },
}

#[derive(Serialize)]
struct TimelineResponse {
    author: Option<UserRef>,
    items: Vec<TimelineItemResponse>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateMergeRequestRequest {
    source_branch: String,
    target_branch: String,
    title: String,
    description: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddCommentRequest {
    body: String,
    #[serde(default)]
    reply_to_id: Option<Uuid>,
    #[serde(default)]
    file_path: Option<String>,
    #[serde(default)]
    line_number: Option<i32>,
    #[serde(default)]
    end_line: Option<i32>,
    #[serde(default)]
    side: Option<String>,
    #[serde(default)]
    suggested_content: Option<String>,
}

#[derive(Deserialize)]
struct SubmitReviewRequest {
    decision: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReviewResponse {
    user_id: Uuid,
    username: String,
    decision: String,
    stale: bool,
    created_at: DateTime<Utc>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReviewSummaryResponse {
    reviews: Vec<ReviewResponse>,
    required_approvals: i32,
    live_approval_count: i32,
    blocked: bool,
}

// Built once per merge attempt and serialized straight away: boxing the large variant would only add an allocation.
#[allow(clippy::large_enum_variant)]
#[derive(Serialize)]
#[serde(tag = "outcome", rename_all = "camelCase")]
enum MergeAttemptResponse {
    #[serde(rename = "merged")]
    Merged {
        #[serde(flatten)]
        merge_request: MergeRequestResponse,
    },
    #[serde(rename = "conflicting")]
    Conflicting,
}

async fn list_branches(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(repository_id): Path<Uuid>,
) -> Result<Json<Vec<BranchResponse>>, ApiError> {
    let repo = require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Reader).await?;
    branches_response(&state, &repo).await
}

pub(crate) async fn branches_response(
    state: &AppState,
    repo: &Repository,
) -> Result<Json<Vec<BranchResponse>>, ApiError> {
    let branches = state.branch_reader.list_branches(&repo.disk_path).await?;
    Ok(Json(
        branches
            .into_iter()
            .map(|b| BranchResponse {
                name: b.name,
                tip_sha: b.tip_sha,
                is_default: b.is_default,
            })
            .collect(),
    ))
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ListMergeRequestsQuery {
    #[serde(default)]
    label_ids: Option<String>,
    #[serde(default)]
    milestone_id: Option<Uuid>,
}

fn parse_label_ids(raw: Option<String>) -> Result<Option<Vec<Uuid>>, ApiError> {
    let Some(raw) = raw.filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let ids: Result<Vec<Uuid>, _> = raw.split(',').map(Uuid::parse_str).collect();
    Ok(Some(ids.map_err(|_| {
        DomainError::Validation("labelIds must be a comma-separated list of UUIDs".to_string())
    })?))
}

async fn list_for_repository(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(repository_id): Path<Uuid>,
    Query(query): Query<ListMergeRequestsQuery>,
) -> Result<Json<Vec<MergeRequestResponse>>, ApiError> {
    let repo = require_role_by_id(&state, user_id, repository_id, CollaboratorRole::Reader).await?;
    let label_ids = parse_label_ids(query.label_ids)?;
    let merge_requests = state
        .merge_requests
        .list_for_repository_filtered(repo.id, label_ids, query.milestone_id)
        .await?;
    Ok(Json(
        MergeRequestResponse::build_many(&state, merge_requests).await?,
    ))
}

async fn detail(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(merge_request_id): Path<Uuid>,
) -> Result<Json<MergeRequestResponse>, ApiError> {
    let (mr, _repo) =
        find_accessible_merge_request(&state, user_id, merge_request_id, CollaboratorRole::Reader)
            .await?;
    Ok(Json(MergeRequestResponse::build(&state, mr).await?))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateMergeRequestRequest {
    title: String,
    description: String,
    /// Double `Option` to tell an absent field (`None`) from an explicit `null` (`Some(None)`, which clears it).
    /// Serde turns a missing plain `Option` into `None`, so an omitted `milestoneId` would silently clear the milestone.
    #[serde(default, deserialize_with = "crate::routes::deserialize_present")]
    milestone_id: Option<Option<Uuid>>,
}

/// A failed lookup is an error, not "no milestone": the caller must not journal a change it could not verify.
async fn milestone_title(
    state: &AppState,
    milestone_id: Option<Uuid>,
) -> Result<Option<String>, DomainError> {
    let Some(milestone_id) = milestone_id else {
        return Ok(None);
    };
    Ok(state
        .milestones
        .find_by_id(milestone_id)
        .await?
        .map(|milestone| milestone.title))
}

async fn update(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(merge_request_id): Path<Uuid>,
    Json(req): Json<UpdateMergeRequestRequest>,
) -> Result<Json<MergeRequestResponse>, ApiError> {
    let (mr, repo) = find_accessible_merge_request(
        &state,
        user_id,
        merge_request_id,
        CollaboratorRole::Contributor,
    )
    .await?;
    let milestone_id = req.milestone_id.ok_or_else(|| {
        DomainError::Validation(
            "milestoneId is required on a merge request update; send null to clear it".to_string(),
        )
    })?;
    let use_case = UpdateMergeRequestUseCase::new(
        state.merge_requests.clone(),
        state.milestones.clone(),
        state.groups.clone(),
    );
    let updated = use_case
        .execute(
            merge_request_id,
            &repo,
            req.title,
            req.description,
            milestone_id,
        )
        .await?;
    let milestones = match (
        milestone_title(&state, mr.milestone_id).await,
        milestone_title(&state, updated.milestone_id).await,
    ) {
        (Ok(before), Ok(after)) => Some((before, after)),
        (before, after) => {
            let err = before.err().or(after.err());
            tracing::warn!(error = ?err, merge_request_id = %merge_request_id, "milestone lookup failed; not recording a milestone change");
            None
        }
    };
    state
        .merge_request_activity
        .edited(merge_request_id, user_id, &mr, &updated, milestones)
        .await;
    Ok(Json(MergeRequestResponse::build(&state, updated).await?))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetLabelsRequest {
    label_ids: Vec<Uuid>,
}

async fn set_labels(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(merge_request_id): Path<Uuid>,
    Json(req): Json<SetLabelsRequest>,
) -> Result<Json<Vec<LabelResponse>>, ApiError> {
    let (_mr, repo) = find_accessible_merge_request(
        &state,
        user_id,
        merge_request_id,
        CollaboratorRole::Contributor,
    )
    .await?;
    // Best-effort: the journal must not become a new way for a label update to fail.
    let before = match state.labels.list_for_merge_request(merge_request_id).await {
        Ok(before) => Some(before),
        Err(err) => {
            tracing::warn!(error = %err, merge_request_id = %merge_request_id, "could not read the labels before the update; not recording a label change");
            None
        }
    };
    let use_case = SetMergeRequestLabelsUseCase::new(state.labels.clone(), state.groups.clone());
    let labels = use_case
        .execute(merge_request_id, &repo, req.label_ids)
        .await?;
    if let Some(before) = before {
        state
            .merge_request_activity
            .labels_changed(merge_request_id, user_id, &before, &labels)
            .await;
    }
    Ok(Json(labels.into_iter().map(Into::into).collect()))
}

async fn diff(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(merge_request_id): Path<Uuid>,
) -> Result<Json<Vec<FileDiffResponse>>, ApiError> {
    let (mr, repo) =
        find_accessible_merge_request(&state, user_id, merge_request_id, CollaboratorRole::Reader)
            .await?;
    let diffs = state
        .diff_reader
        .diff_branches(&repo.disk_path, &mr.source_branch, &mr.target_branch)
        .await?;
    Ok(Json(diffs.into_iter().map(Into::into).collect()))
}

async fn live_diffs(
    state: &AppState,
    mr: &MergeRequest,
    repo: &Repository,
) -> Result<Option<Vec<FileDiff>>, DomainError> {
    if mr.status == MergeRequestStatus::Open {
        Ok(Some(
            state
                .diff_reader
                .diff_branches(&repo.disk_path, &mr.source_branch, &mr.target_branch)
                .await?,
        ))
    } else {
        Ok(None)
    }
}

async fn list_comments(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(merge_request_id): Path<Uuid>,
) -> Result<Json<Vec<CommentResponse>>, ApiError> {
    let (mr, repo) =
        find_accessible_merge_request(&state, user_id, merge_request_id, CollaboratorRole::Reader)
            .await?;
    let comments = state
        .merge_request_comments
        .list_comments(merge_request_id)
        .await?;
    let diffs = live_diffs(&state, &mr, &repo).await?;
    let authors = load_user_refs(&state, comments.iter().filter_map(|c| c.author_id)).await;

    Ok(Json(
        comments
            .into_iter()
            .map(|c| {
                let outdated = diffs.as_deref().is_some_and(|diffs| is_outdated(diffs, &c));
                let author = c.author_id.and_then(|id| authors.get(&id).cloned());
                CommentResponse::from_comment(c, outdated, author)
            })
            .collect(),
    ))
}

async fn timeline(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(merge_request_id): Path<Uuid>,
) -> Result<Json<TimelineResponse>, ApiError> {
    let (mr, repo) =
        find_accessible_merge_request(&state, user_id, merge_request_id, CollaboratorRole::Reader)
            .await?;
    let comments = state
        .merge_request_comments
        .list_comments(merge_request_id)
        .await?;
    let events = state.merge_request_events.list(merge_request_id).await?;
    // The diff only feeds excerpts and the outdated flag. Without it (for example when the source
    // branch is gone) the timeline still answers, just without them.
    let diffs = live_diffs(&state, &mr, &repo).await.unwrap_or_else(|err| {
        tracing::warn!(error = %err, merge_request_id = %merge_request_id, "could not compute the diff for the timeline; omitting excerpts");
        None
    });
    let items = assemble_timeline(comments, events, diffs.as_deref());

    let mut user_ids: Vec<Uuid> = mr.author_id.into_iter().collect();
    for item in &items {
        match item {
            TimelineItem::Comment(comment) => user_ids.extend(comment.author_id),
            TimelineItem::Thread(thread) => {
                user_ids.extend(thread.root.author_id);
                user_ids.extend(thread.replies.iter().filter_map(|reply| reply.author_id));
                user_ids.extend(thread.resolved_by);
            }
            TimelineItem::Event(event) => user_ids.extend(event.actor_id),
        }
    }
    let users = load_user_refs(&state, user_ids).await;
    let user_ref = |id: Uuid| users.get(&id).cloned();

    let items = items
        .into_iter()
        .map(|item| match item {
            TimelineItem::Comment(comment) => TimelineItemResponse::Comment {
                id: comment.id,
                created_at: comment.created_at,
                author: comment.author_id.and_then(user_ref),
                body: comment.body,
            },
            TimelineItem::Thread(thread) => {
                let root_author = thread.root.author_id.and_then(user_ref);
                let replies = thread.replies.into_iter().map(|reply| {
                    let author = reply.author_id.and_then(user_ref);
                    CommentResponse::from_comment(reply, false, author)
                });
                TimelineItemResponse::Thread {
                    id: thread.root.id,
                    created_at: thread.root.created_at,
                    file_path: thread.root.file_path.clone(),
                    line_number: thread.root.line_number,
                    end_line: thread.root.end_line,
                    side: thread.root.side.map(|side| side.as_str()),
                    outdated: thread.outdated,
                    resolved: thread.root.resolved,
                    resolved_by: thread.resolved_by.and_then(user_ref),
                    resolved_at: thread.resolved_at,
                    excerpt: thread.excerpt.into_iter().map(Into::into).collect(),
                    replies: replies.collect(),
                    root: CommentResponse::from_comment(thread.root, thread.outdated, root_author),
                }
            }
            TimelineItem::Event(event) => TimelineItemResponse::Event {
                id: event.id,
                created_at: event.created_at,
                actor: event.actor_id.and_then(user_ref),
                kind: event.kind.as_str(),
                payload: event.payload,
            },
        })
        .collect();

    Ok(Json(TimelineResponse {
        author: mr.author_id.and_then(user_ref),
        items,
    }))
}

async fn create(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(repository_id): Path<Uuid>,
    Json(req): Json<CreateMergeRequestRequest>,
) -> Result<Json<MergeRequestResponse>, ApiError> {
    let repo = require_role_by_id(
        &state,
        user_id,
        repository_id,
        CollaboratorRole::Contributor,
    )
    .await?;
    let use_case =
        CreateMergeRequestUseCase::new(state.merge_requests.clone(), state.branch_reader.clone());
    let mr = use_case
        .execute(
            repo.id,
            &repo.disk_path,
            user_id,
            req.source_branch,
            req.target_branch,
            req.title,
            req.description,
        )
        .await?;
    // Best-effort: remember where the source branch stands now, so a later push can be
    // recorded as "commits pushed" from this sha.
    if let Ok(branches) = state.branch_reader.list_branches(&repo.disk_path).await
        && let Some(tip) = branches.iter().find(|b| b.name == mr.source_branch)
    {
        state
            .merge_request_events
            .set_head_sha(mr.id, &tip.tip_sha)
            .await
            .ok();
    }
    Ok(Json(MergeRequestResponse::build(&state, mr).await?))
}

async fn add_comment(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(merge_request_id): Path<Uuid>,
    Json(req): Json<AddCommentRequest>,
) -> Result<Json<CommentResponse>, ApiError> {
    find_accessible_merge_request(
        &state,
        user_id,
        merge_request_id,
        CollaboratorRole::Contributor,
    )
    .await?;

    let anchor = match (req.file_path, req.line_number, req.side) {
        (None, None, None) => {
            if req.end_line.is_some() || req.suggested_content.is_some() {
                return Err(DomainError::Validation(
                    "endLine and suggestedContent require filePath, lineNumber and side"
                        .to_string(),
                )
                .into());
            }
            None
        }
        (Some(file_path), Some(line_number), Some(side)) => {
            let side = ferrisgit_domain::diff::DiffSide::parse(&side)?;
            Some(
                ferrisgit_application::use_cases::add_merge_request_comment::PostedAnchor {
                    file_path,
                    line_number,
                    end_line: req.end_line,
                    side,
                },
            )
        }
        _ => {
            return Err(DomainError::Validation(
                "filePath, lineNumber and side must all be present, or all absent".to_string(),
            )
            .into());
        }
    };

    let use_case = AddMergeRequestCommentUseCase::new(
        state.merge_requests.clone(),
        state.merge_request_comments.clone(),
        state.repositories.clone(),
        state.users.clone(),
        state.notifications.clone(),
        state.diff_reader.clone(),
        state.webhooks.clone(),
    );
    let comment = use_case
        .execute(
            merge_request_id,
            user_id,
            req.body,
            req.reply_to_id,
            anchor,
            req.suggested_content,
        )
        .await?;
    let author = load_user_refs(&state, comment.author_id)
        .await
        .into_values()
        .next();
    Ok(Json(CommentResponse::from_comment(comment, false, author)))
}

async fn apply_suggestion(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((merge_request_id, comment_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<CommentResponse>, ApiError> {
    find_accessible_merge_request(
        &state,
        user_id,
        merge_request_id,
        CollaboratorRole::Contributor,
    )
    .await?;
    let use_case = ferrisgit_application::use_cases::apply_suggestion_comment::ApplySuggestionCommentUseCase::new(
        state.merge_requests.clone(),
        state.merge_request_comments.clone(),
        state.repositories.clone(),
        state.users.clone(),
        state.diff_reader.clone(),
        state.branch_reader.clone(),
        state.suggestion_executor.clone(),
    );
    let comment = use_case
        .execute(merge_request_id, comment_id, user_id)
        .await?;
    let author = load_user_refs(&state, comment.author_id)
        .await
        .into_values()
        .next();
    Ok(Json(CommentResponse::from_comment(comment, false, author)))
}

async fn resolve_comment(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((merge_request_id, comment_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    find_accessible_merge_request(
        &state,
        user_id,
        merge_request_id,
        CollaboratorRole::Contributor,
    )
    .await?;
    let use_case = ResolveMergeRequestCommentUseCase::new(state.merge_request_comments.clone());
    use_case.execute(merge_request_id, comment_id, true).await?;
    state
        .merge_request_activity
        .thread_resolved(merge_request_id, user_id, comment_id, true)
        .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn unresolve_comment(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path((merge_request_id, comment_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    find_accessible_merge_request(
        &state,
        user_id,
        merge_request_id,
        CollaboratorRole::Contributor,
    )
    .await?;
    let use_case = ResolveMergeRequestCommentUseCase::new(state.merge_request_comments.clone());
    use_case
        .execute(merge_request_id, comment_id, false)
        .await?;
    state
        .merge_request_activity
        .thread_resolved(merge_request_id, user_id, comment_id, false)
        .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn submit_review(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(merge_request_id): Path<Uuid>,
    Json(req): Json<SubmitReviewRequest>,
) -> Result<Json<ReviewResponse>, ApiError> {
    let (mr, repo) = find_accessible_merge_request(
        &state,
        user_id,
        merge_request_id,
        CollaboratorRole::Contributor,
    )
    .await?;
    let decision = ReviewDecision::parse(&req.decision)?;
    let use_case = SubmitMergeRequestReviewUseCase::new(
        state.merge_requests.clone(),
        state.merge_request_reviews.clone(),
        state.branch_reader.clone(),
        state.repositories.clone(),
        state.users.clone(),
        state.notifications.clone(),
        state.webhooks.clone(),
    );
    let review = use_case
        .execute(
            merge_request_id,
            &repo.disk_path,
            &mr.source_branch,
            user_id,
            decision,
        )
        .await?;
    state
        .merge_request_activity
        .review_submitted(merge_request_id, user_id, decision)
        .await;
    Ok(Json(ReviewResponse {
        user_id: review.user_id,
        username: review.username,
        decision: review.decision.as_str().to_string(),
        stale: false,
        created_at: review.created_at,
    }))
}

async fn list_reviews(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(merge_request_id): Path<Uuid>,
) -> Result<Json<ReviewSummaryResponse>, ApiError> {
    let (mr, repo) =
        find_accessible_merge_request(&state, user_id, merge_request_id, CollaboratorRole::Reader)
            .await?;
    let settings = state
        .repository_settings
        .get_or_create_default(repo.id)
        .await?;
    let branches = state.branch_reader.list_branches(&repo.disk_path).await?;
    let tip_sha = branches
        .into_iter()
        .find(|b| b.name == mr.source_branch)
        .map(|b| b.tip_sha);
    let reviews = state
        .merge_request_reviews
        .list_reviews(merge_request_id)
        .await?;

    let review_responses: Vec<ReviewResponse> = reviews
        .into_iter()
        .map(|r| {
            let stale = tip_sha.as_deref() != Some(r.source_sha.as_str());
            ReviewResponse {
                user_id: r.user_id,
                username: r.username,
                decision: r.decision.as_str().to_string(),
                stale,
                created_at: r.created_at,
            }
        })
        .collect();

    let live_approval_count = review_responses
        .iter()
        .filter(|r| !r.stale && r.decision == "approved")
        .count() as i32;
    let blocked_by_changes = review_responses
        .iter()
        .any(|r| !r.stale && r.decision == "changes_requested");
    let blocked = settings.required_approvals > 0
        && (blocked_by_changes || live_approval_count < settings.required_approvals);

    Ok(Json(ReviewSummaryResponse {
        reviews: review_responses,
        required_approvals: settings.required_approvals,
        live_approval_count,
        blocked,
    }))
}

async fn merge(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(merge_request_id): Path<Uuid>,
) -> Result<Json<MergeAttemptResponse>, ApiError> {
    find_accessible_merge_request(
        &state,
        user_id,
        merge_request_id,
        CollaboratorRole::Maintainer,
    )
    .await?;
    let use_case = MergeMergeRequestUseCase::new(
        state.merge_requests.clone(),
        state.merge_request_reviews.clone(),
        state.merge_executor.clone(),
        state.repositories.clone(),
        state.repository_settings.clone(),
        state.system_settings.clone(),
        state.pipelines.clone(),
        state.jobs.clone(),
        state.pipeline_file_reader.clone(),
        state.pipeline_events.clone(),
        state.job_execution.clone(),
        state.branch_reader.clone(),
        state.users.clone(),
        state.notifications.clone(),
        state.webhooks.clone(),
    );
    match use_case.execute(merge_request_id, user_id).await? {
        MergeMergeRequestResult::Merged(mr) => {
            state
                .merge_request_activity
                .merged(mr.id, user_id, mr.merge_commit_sha.as_deref())
                .await;
            Ok(Json(MergeAttemptResponse::Merged {
                merge_request: MergeRequestResponse::build(&state, *mr).await?,
            }))
        }
        MergeMergeRequestResult::Conflicting => Ok(Json(MergeAttemptResponse::Conflicting)),
    }
}

async fn close(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(merge_request_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    find_accessible_merge_request(
        &state,
        user_id,
        merge_request_id,
        CollaboratorRole::Contributor,
    )
    .await?;
    let use_case = CloseMergeRequestUseCase::new(
        state.merge_requests.clone(),
        state.repositories.clone(),
        state.users.clone(),
        state.notifications.clone(),
        state.webhooks.clone(),
    );
    use_case.execute(merge_request_id, user_id).await?;
    state
        .merge_request_activity
        .closed(merge_request_id, user_id)
        .await;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/repositories/{repository_id}/branches", get(list_branches))
        .route(
            "/repositories/{repository_id}/merge-requests",
            get(list_for_repository).post(create),
        )
        .route("/merge-requests/{id}", get(detail).patch(update))
        .route(
            "/merge-requests/{id}/labels",
            axum::routing::put(set_labels),
        )
        .route("/merge-requests/{id}/diff", get(diff))
        .route("/merge-requests/{id}/timeline", get(timeline))
        .route(
            "/merge-requests/{id}/comments",
            get(list_comments).post(add_comment),
        )
        .route(
            "/merge-requests/{id}/comments/{comment_id}/resolve",
            post(resolve_comment),
        )
        .route(
            "/merge-requests/{id}/comments/{comment_id}/unresolve",
            post(unresolve_comment),
        )
        .route(
            "/merge-requests/{id}/comments/{comment_id}/apply-suggestion",
            post(apply_suggestion),
        )
        .route(
            "/merge-requests/{id}/reviews",
            get(list_reviews).post(submit_review),
        )
        .route("/merge-requests/{id}/merge", post(merge))
        .route("/merge-requests/{id}/close", post(close))
}

#[cfg(test)]
mod pairing_tests {
    use super::*;
    use ferrisgit_domain::diff::{DiffLine, DiffLineKind};

    fn context(n: u32, content: &str) -> DiffLine {
        DiffLine {
            kind: DiffLineKind::Context,
            content: content.to_string(),
            old_line: Some(n),
            new_line: Some(n),
        }
    }
    fn removed(n: u32, content: &str) -> DiffLine {
        DiffLine {
            kind: DiffLineKind::Removed,
            content: content.to_string(),
            old_line: Some(n),
            new_line: None,
        }
    }
    fn added(n: u32, content: &str) -> DiffLine {
        DiffLine {
            kind: DiffLineKind::Added,
            content: content.to_string(),
            old_line: None,
            new_line: Some(n),
        }
    }

    #[test]
    fn a_pure_context_run_pairs_each_line_with_itself() {
        let rows = pair_diff_lines(&[context(1, "a\n"), context(2, "b\n")]);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].kind, "context");
        assert_eq!(rows[0].old_line, Some(1));
        assert_eq!(rows[0].new_line, Some(1));
        assert_eq!(rows[0].old_content.as_deref(), Some("a\n"));
        assert_eq!(rows[0].new_content.as_deref(), Some("a\n"));
    }

    #[test]
    fn a_lone_addition_has_no_old_side() {
        let rows = pair_diff_lines(&[added(1, "new\n")]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, "added");
        assert!(rows[0].old_line.is_none());
        assert!(rows[0].old_content.is_none());
        assert_eq!(rows[0].new_content.as_deref(), Some("new\n"));
    }

    #[test]
    fn a_lone_deletion_has_no_new_side() {
        let rows = pair_diff_lines(&[removed(1, "gone\n")]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, "removed");
        assert!(rows[0].new_line.is_none());
        assert_eq!(rows[0].old_content.as_deref(), Some("gone\n"));
    }

    #[test]
    fn a_removed_line_immediately_followed_by_an_added_line_is_paired_into_one_modified_row() {
        let rows = pair_diff_lines(&[removed(1, "old text\n"), added(1, "new text\n")]);
        assert_eq!(
            rows.len(),
            1,
            "a delete+insert pair must become ONE row, not two"
        );
        assert_eq!(rows[0].kind, "modified");
        assert_eq!(rows[0].old_content.as_deref(), Some("old text\n"));
        assert_eq!(rows[0].new_content.as_deref(), Some("new text\n"));
    }

    #[test]
    fn a_mixed_hunk_pairs_correctly_end_to_end() {
        let rows = pair_diff_lines(&[
            context(1, "keep\n"),
            removed(2, "old\n"),
            added(2, "new\n"),
            added(3, "extra\n"),
        ]);
        assert_eq!(
            rows.len(),
            3,
            "context, then one modified pair, then one lone addition"
        );
        assert_eq!(rows[0].kind, "context");
        assert_eq!(rows[1].kind, "modified");
        assert_eq!(rows[2].kind, "added");
        assert_eq!(rows[2].new_content.as_deref(), Some("extra\n"));
    }
}
