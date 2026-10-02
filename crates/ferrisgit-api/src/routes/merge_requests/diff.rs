use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use ferrisgit_domain::diff::{FileChangeKind, FileDiff};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::merge_request::{MergeRequest, MergeRequestStatus};
use ferrisgit_domain::repository::Repository;
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use serde::Serialize;
use uuid::Uuid;

use crate::auth_middleware::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;

use super::find_accessible_merge_request;

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

pub(super) async fn live_diffs(
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

pub(super) fn router() -> Router<AppState> {
    Router::new().route("/merge-requests/{id}/diff", get(diff))
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
