use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use ferrisgit_application::merge_request_timeline::{ExcerptLine, TimelineItem, assemble_timeline};
use ferrisgit_domain::diff::DiffLineKind;
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use serde::Serialize;
use uuid::Uuid;

use crate::auth_middleware::AuthUser;
use crate::error::ApiError;
use crate::routes::user_ref::{UserRef, load_user_refs};
use crate::state::AppState;

use super::comments::CommentResponse;
use super::diff::live_diffs;
use super::find_accessible_merge_request;

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

pub(super) fn router() -> Router<AppState> {
    Router::new().route("/merge-requests/{id}/timeline", get(timeline))
}
