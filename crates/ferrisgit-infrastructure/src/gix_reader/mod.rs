//! Read-only access to bare repositories through `gix`, split by what is read.

mod diff;
mod history;
mod languages;
mod refs;
mod tree;
mod wiki;

#[cfg(test)]
mod tests;

use std::fmt::Display;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitInfo {
    pub sha: String,
    pub message: String,
    pub author_name: String,
    pub author_email: String,
    pub committed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeEntryInfo {
    pub name: String,
    pub is_dir: bool,
    pub last_commit: Option<CommitInfo>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContributorInfo {
    pub name: String,
    pub email: String,
    pub commit_count: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LanguageStat {
    pub name: String,
    pub bytes: u64,
    pub percentage: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchInfoRaw {
    pub name: String,
    pub tip_sha: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagInfoRaw {
    pub name: String,
    pub target_sha: String,
}

#[derive(Debug, Clone)]
pub struct WikiPageInfoRaw {
    pub slug: String,
}

#[derive(Debug, Clone)]
pub struct WikiPageContentRaw {
    pub content: String,
    pub head_sha: String,
}

#[derive(Debug, Clone)]
pub struct WikiRevisionRaw {
    pub commit_sha: String,
    pub author_name: String,
    pub author_email: String,
    pub committed_at: DateTime<Utc>,
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum GitReadError {
    #[error("failed to open repository: {0}")]
    Open(String),
    #[error("git read error: {0}")]
    Other(String),
}

impl GitReadError {
    fn other(error: impl Display) -> Self {
        Self::Other(error.to_string())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum FileChangeKindRaw {
    Added,
    Modified,
    Deleted,
    Binary,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DiffLineKindRaw {
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiffLineRaw {
    pub kind: DiffLineKindRaw,
    pub content: String,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HunkRaw {
    pub lines: Vec<DiffLineRaw>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FileDiffRaw {
    pub path: String,
    pub change: FileChangeKindRaw,
    pub hunks: Vec<HunkRaw>,
}

pub struct GixRepositoryReader;

fn open(disk_path: &Path) -> Result<gix::Repository, GitReadError> {
    gix::open(disk_path).map_err(|e| GitReadError::Open(e.to_string()))
}

/// The commit whose full 40-character hex id is `sha`. A malformed id, an unknown object and a
/// non-commit all give `None`, so a bad sha never surfaces as an error (and a 500) at the API layer.
fn commit_by_sha<'repo>(repo: &'repo gix::Repository, sha: &str) -> Option<gix::Commit<'repo>> {
    let id = gix::ObjectId::from_hex(sha.as_bytes()).ok()?;
    repo.find_object(id).ok()?.try_into_commit().ok()
}

fn newest_first_walk(
    repo: &gix::Repository,
    start: gix::ObjectId,
) -> Result<gix::revision::Walk<'_>, GitReadError> {
    repo.rev_walk([start])
        .sorting(gix::revision::walk::Sorting::ByCommitTime(
            gix::traverse::commit::simple::CommitTimeOrder::NewestFirst,
        ))
        .all()
        .map_err(GitReadError::other)
}

fn commit_info(commit: &gix::Commit<'_>) -> Result<CommitInfo, GitReadError> {
    let decoded = commit.decode().map_err(GitReadError::other)?;
    let author = decoded.author().map_err(GitReadError::other)?;
    let seconds = author.time().map_err(GitReadError::other)?.seconds;
    Ok(CommitInfo {
        sha: commit.id.to_string(),
        message: decoded.message().summary().to_string(),
        author_name: author.name.to_string(),
        author_email: author.email.to_string(),
        committed_at: DateTime::from_timestamp(seconds, 0).unwrap_or_default(),
    })
}

/// The object id of the entry at `path` in `tree`, if there is one.
fn id_at_path(tree: &gix::Tree<'_>, path: &str) -> Result<Option<gix::ObjectId>, GitReadError> {
    Ok(tree
        .lookup_entry_by_path(path)
        .map_err(GitReadError::other)?
        .map(|entry| entry.object_id()))
}
