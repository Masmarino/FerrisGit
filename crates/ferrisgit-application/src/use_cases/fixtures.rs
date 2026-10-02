//! Entity builders for the use case tests: valid values with fresh random ids, overridden with struct update syntax
//! (`User { is_admin: true, ..user("root") }`).

use std::collections::BTreeMap;

use chrono::Utc;
use ferrisgit_domain::diff::{DiffLine, DiffLineKind, FileChangeKind, FileDiff, Hunk};
use ferrisgit_domain::group::Group;
use ferrisgit_domain::issue::{Issue, IssueKind, IssueStatus};
use ferrisgit_domain::job::{Job, JobStatus};
use ferrisgit_domain::label::Label;
use ferrisgit_domain::merge_request::{MergeRequest, MergeRequestStatus};
use ferrisgit_domain::merge_request_comment::MergeRequestComment;
use ferrisgit_domain::milestone::{Milestone, MilestoneState};
use ferrisgit_domain::pipeline::{Pipeline, PipelineStatus};
use ferrisgit_domain::release::{Release, ReleaseAsset};
use ferrisgit_domain::repository::{Repository, RepositoryVisibility};
use ferrisgit_domain::settings::{ExecutionEngine, SystemSettings};
use ferrisgit_domain::user::User;
use uuid::Uuid;

pub(crate) fn user(username: &str) -> User {
    User {
        id: Uuid::new_v4(),
        username: username.to_string(),
        email: format!("{username}@example.com"),
        password_hash: "h".to_string(),
        is_admin: false,
        created_at: Utc::now(),
    }
}

/// A private personal repository called `hello`.
pub(crate) fn repository(owner_id: Uuid) -> Repository {
    repository_with_id(Uuid::new_v4(), owner_id)
}

pub(crate) fn repository_with_id(id: Uuid, owner_id: Uuid) -> Repository {
    Repository {
        id,
        owner_id,
        name: "hello".to_string(),
        group_id: None,
        description: String::new(),
        disk_path: "hello.git".to_string(),
        visibility: RepositoryVisibility::Private,
        created_at: Utc::now(),
    }
}

pub(crate) fn group(parent_group_id: Option<Uuid>, name: &str) -> Group {
    Group {
        id: Uuid::new_v4(),
        parent_group_id,
        name: name.to_string(),
        description: String::new(),
        created_by: None,
        created_at: Utc::now(),
    }
}

/// An open `Todo` bug, number 1, without assignee.
pub(crate) fn issue(repository_id: Uuid, author_id: Uuid) -> Issue {
    Issue {
        id: Uuid::new_v4(),
        repository_id,
        number: 1,
        author_id,
        assignee_id: None,
        milestone_id: None,
        title: "bug".to_string(),
        description: "desc".to_string(),
        status: IssueStatus::Todo,
        kind: IssueKind::Bug,
        parent_issue_id: None,
        created_at: Utc::now(),
        closed_at: None,
    }
}

/// An open merge request from `feature` into `main`.
pub(crate) fn merge_request(repository_id: Uuid, author_id: Uuid) -> MergeRequest {
    MergeRequest {
        id: Uuid::new_v4(),
        repository_id,
        author_id: Some(author_id),
        source_branch: "feature".to_string(),
        target_branch: "main".to_string(),
        title: "t".to_string(),
        description: String::new(),
        status: MergeRequestStatus::Open,
        merge_commit_sha: None,
        milestone_id: None,
        created_at: Utc::now(),
        closed_at: None,
    }
}

/// A general comment (no anchor, not a reply, not a suggestion) by an unknown user.
pub(crate) fn merge_request_comment(merge_request_id: Uuid) -> MergeRequestComment {
    MergeRequestComment {
        id: Uuid::new_v4(),
        merge_request_id,
        author_id: Some(Uuid::new_v4()),
        body: "comment".to_string(),
        created_at: Utc::now(),
        reply_to_id: None,
        file_path: None,
        line_number: None,
        side: None,
        anchor_content: None,
        resolved: false,
        end_line: None,
        suggested_content: None,
        applied_at: None,
        applied_commit_sha: None,
    }
}

/// The diff of `README.md` with a single hunk made of `lines`.
pub(crate) fn readme_diff(lines: Vec<DiffLine>) -> Vec<FileDiff> {
    vec![FileDiff {
        path: "README.md".to_string(),
        change: FileChangeKind::Modified,
        hunks: vec![Hunk { lines }],
    }]
}

/// Line `line_number` (new side) holding `line <n>`.
pub(crate) fn added_line(line_number: u32) -> DiffLine {
    DiffLine {
        kind: DiffLineKind::Added,
        content: format!("line {line_number}\n"),
        old_line: None,
        new_line: Some(line_number),
    }
}

/// Line `line_number` (old side) holding `line <n>`.
pub(crate) fn removed_line(line_number: u32) -> DiffLine {
    DiffLine {
        kind: DiffLineKind::Removed,
        content: format!("line {line_number}\n"),
        old_line: Some(line_number),
        new_line: None,
    }
}

/// A `build` / `compile` job of `pipeline_id` that no runner has claimed.
pub(crate) fn job(pipeline_id: Uuid, status: JobStatus) -> Job {
    Job {
        id: Uuid::new_v4(),
        pipeline_id,
        stage: "build".to_string(),
        name: "compile".to_string(),
        image: "rust".to_string(),
        script: vec![],
        variables: BTreeMap::new(),
        needs: vec![],
        tags: vec![],
        cache: vec![],
        status,
        runner_id: None,
        logs: String::new(),
        created_at: Utc::now(),
        started_at: None,
        finished_at: None,
    }
}

/// A pipeline on the Docker runners for commit `abc`, not finished.
pub(crate) fn pipeline(id: Uuid, status: PipelineStatus) -> Pipeline {
    Pipeline {
        id,
        repository_id: Uuid::new_v4(),
        commit_sha: "abc".to_string(),
        execution_engine: ExecutionEngine::DockerRunners,
        status,
        triggered_by: Uuid::new_v4(),
        created_at: Utc::now(),
        finished_at: None,
        error: None,
    }
}

/// A red `Bug` label scoped to a repository.
pub(crate) fn label() -> Label {
    Label {
        id: Uuid::new_v4(),
        name: "Bug".to_string(),
        color: "#dc2626".to_string(),
        repository_id: Some(Uuid::new_v4()),
        group_id: None,
        created_at: Utc::now(),
    }
}

/// An open `v1.0` milestone scoped to a repository, without due date.
pub(crate) fn milestone() -> Milestone {
    Milestone {
        id: Uuid::new_v4(),
        title: "v1.0".to_string(),
        description: String::new(),
        due_date: None,
        state: MilestoneState::Open,
        repository_id: Some(Uuid::new_v4()),
        group_id: None,
        created_at: Utc::now(),
    }
}

/// A published `v1.0.0` release. The stores only find or delete an existing row, so tests have to seed one.
pub(crate) fn release(id: Uuid, repository_id: Uuid) -> Release {
    Release {
        id,
        repository_id,
        tag_name: "v1.0.0".to_string(),
        title: "First release".to_string(),
        notes: String::new(),
        draft: false,
        prerelease: false,
        author_id: Some(Uuid::new_v4()),
        created_at: Utc::now(),
        published_at: Some(Utc::now()),
    }
}

pub(crate) fn release_asset(release_id: Uuid, disk_path: &str) -> ReleaseAsset {
    ReleaseAsset {
        id: Uuid::new_v4(),
        release_id,
        filename: "a.txt".to_string(),
        content_type: "text/plain".to_string(),
        size_bytes: 1,
        disk_path: disk_path.to_string(),
        uploaded_by: Some(Uuid::new_v4()),
        created_at: Utc::now(),
    }
}

/// A 64-char lower-case hex string, the shape of every one-time link token.
pub(crate) fn is_hex64(token: &str) -> bool {
    token.len() == 64
        && token
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

/// The instance settings of a fresh install: Docker runners, no concurrency ceiling.
pub(crate) fn system_settings() -> SystemSettings {
    SystemSettings {
        execution_engine: ExecutionEngine::DockerRunners,
        k8s_namespace: None,
        k8s_cache_storage_class: None,
        runner_registration_token: None,
        log_retention_days: None,
        max_concurrent_jobs: None,
        jwt_ttl_hours: 12,
        max_push_size_mb: 500,
    }
}
