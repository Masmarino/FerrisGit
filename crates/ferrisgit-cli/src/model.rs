//! What the server sends, and what `--json` prints: these structs are the documented contract of
//! `docs/ligne-de-commande/scripts-et-agents.md`. A field the server adds stays out until it is named here, so a server
//! upgrade can't change a script's output. Renaming or removing a field here takes a new version.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserRef {
    pub id: String,
    pub username: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Me {
    pub id: String,
    pub username: String,
    pub email: String,
    pub is_admin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    pub id: String,
    pub name: String,
    pub description: String,
    pub owner: String,
    pub role: Option<String>,
    pub visibility: String,
    pub created_at: String,
    pub path: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub star_count: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_starred: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GroupRef {
    pub id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Resolved {
    PersonalRepository {
        repository_id: String,
        path: Vec<String>,
    },
    GroupRepository {
        repository_id: String,
        path: Vec<String>,
        chain: Vec<GroupRef>,
    },
    Group {},
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Branch {
    pub name: String,
    pub tip_sha: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Label {
    pub id: String,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Milestone {
    pub id: String,
    pub title: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Collaborator {
    pub user_id: String,
    pub username: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub id: String,
    pub number: i64,
    pub title: String,
    pub description: String,
    pub status: String,
    pub kind: String,
    pub milestone_id: Option<String>,
    pub author: Option<UserRef>,
    pub assignee: Option<UserRef>,
    pub labels: Vec<Label>,
    pub comment_count: i64,
    pub created_at: String,
    pub closed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub id: String,
    pub author: Option<UserRef>,
    pub body: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeRequestPipeline {
    pub id: String,
    pub status: String,
    pub commit_sha: String,
    pub source_sha: Option<String>,
    pub created_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeRequest {
    pub id: String,
    pub source_branch: String,
    pub target_branch: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub merge_commit_sha: Option<String>,
    pub milestone_id: Option<String>,
    pub author: Option<UserRef>,
    pub labels: Vec<Label>,
    pub comment_count: i64,
    pub pipeline: Option<MergeRequestPipeline>,
    pub created_at: String,
    pub closed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    pub user_id: String,
    pub username: String,
    pub decision: String,
    pub stale: bool,
    pub created_at: String,
}

/// `merge` answers with the merge request flattened beside `outcome`, or with only `outcome` on a conflict.
#[derive(Debug, Clone, Deserialize)]
pub struct MergeOutcome {
    pub outcome: String,
    #[serde(flatten)]
    pub merge_request: Option<MergeRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffRow {
    pub old_line: Option<i64>,
    pub old_content: Option<String>,
    pub new_line: Option<i64>,
    pub new_content: Option<String>,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    pub rows: Vec<DiffRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffFile {
    pub path: String,
    pub change: String,
    pub hunks: Vec<DiffHunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pipeline {
    pub id: String,
    pub commit_sha: String,
    pub status: String,
    pub created_at: String,
    pub finished_at: Option<String>,
    pub triggered_by: Option<UserRef>,
    pub commit_message: Option<String>,
    pub error: Option<String>,
    pub merge_request_id: Option<String>,
    pub source_sha: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub stage: String,
    pub name: String,
    pub status: String,
    pub needs: Vec<String>,
    pub logs: String,
    pub logs_purged_at: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineDetail {
    #[serde(flatten)]
    pub pipeline: Pipeline,
    pub jobs: Vec<Job>,
}

pub fn is_finished(status: &str) -> bool {
    matches!(status, "success" | "failed" | "canceled")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseSummary {
    pub id: String,
    pub tag_name: String,
    pub title: String,
    pub draft: bool,
    pub prerelease: bool,
    pub author: Option<UserRef>,
    pub notes_excerpt: String,
    pub asset_count: i64,
    pub created_at: String,
    pub published_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseAsset {
    pub id: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub id: String,
    pub tag_name: String,
    pub title: String,
    pub notes: String,
    pub draft: bool,
    pub prerelease: bool,
    pub author: Option<UserRef>,
    pub target_commit_sha: String,
    pub assets: Vec<ReleaseAsset>,
    pub created_at: String,
    pub published_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CiVariable {
    pub id: String,
    pub key: String,
    pub masked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiSecret {
    pub id: String,
    pub key: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStart {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri_complete: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedToken {
    pub token: String,
    pub scopes: Vec<String>,
    pub expires_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserSearch {
    pub users: Vec<UserRef>,
}
