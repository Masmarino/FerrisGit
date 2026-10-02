use std::collections::HashMap;
use std::path::Path;

use super::{
    CommitInfo, ContributorInfo, GitReadError, GixRepositoryReader, commit_by_sha, commit_info,
    newest_first_walk, open,
};

/// How many commits `list_contributors` walks. Older contributors are not counted (no full-history fallback).
const CONTRIBUTOR_SEARCH_WINDOW: usize = 1000;

const MAX_CONTRIBUTORS: usize = 20;

impl GixRepositoryReader {
    pub fn list_commits(
        &self,
        disk_path: &Path,
        limit: usize,
    ) -> Result<Vec<CommitInfo>, GitReadError> {
        self.list_commits_at(disk_path, "HEAD", limit)
    }

    /// Newest-first history reachable from `revision` (branch, tag, SHA or `HEAD`,
    /// resolved like `git rev-parse`, with annotated tags peeled to their commit).
    /// `HEAD` on a repository with no commits yet gives an empty list. Any other
    /// revision that doesn't resolve to a commit is a `GitReadError`, so callers can
    /// tell "no history" from "no such ref".
    pub fn list_commits_at(
        &self,
        disk_path: &Path,
        revision: &str,
        limit: usize,
    ) -> Result<Vec<CommitInfo>, GitReadError> {
        let repo = open(disk_path)?;
        let Some(head_id) = resolve_commit_in(&repo, revision) else {
            if revision == "HEAD" {
                return Ok(Vec::new());
            }
            return Err(GitReadError::Other(format!(
                "revision does not resolve to a commit: {revision}"
            )));
        };

        newest_first_walk(&repo, head_id)?
            .take(limit)
            .map(|info| {
                let commit = info
                    .map_err(GitReadError::other)?
                    .object()
                    .map_err(GitReadError::other)?;
                commit_info(&commit)
            })
            .collect()
    }

    pub fn list_contributors(
        &self,
        disk_path: &Path,
        revision: &str,
    ) -> Result<Option<Vec<ContributorInfo>>, GitReadError> {
        self.list_contributors_with_window(disk_path, revision, CONTRIBUTOR_SEARCH_WINDOW)
    }

    /// `list_contributors` with an explicit window, so tests can use a small one.
    pub fn list_contributors_with_window(
        &self,
        disk_path: &Path,
        revision: &str,
        window: usize,
    ) -> Result<Option<Vec<ContributorInfo>>, GitReadError> {
        let repo = open(disk_path)?;
        let Some(start) = commit_by_sha(&repo, revision) else {
            return Ok(None);
        };

        let mut tallies: HashMap<(String, String), u32> = HashMap::new();
        for info in newest_first_walk(&repo, start.id)?.take(window) {
            let commit = info
                .map_err(GitReadError::other)?
                .object()
                .map_err(GitReadError::other)?;
            let decoded = commit.decode().map_err(GitReadError::other)?;
            let author = decoded.author().map_err(GitReadError::other)?;
            *tallies
                .entry((author.name.to_string(), author.email.to_string()))
                .or_insert(0) += 1;
        }

        let mut contributors: Vec<ContributorInfo> = tallies
            .into_iter()
            .map(|((name, email), commit_count)| ContributorInfo {
                name,
                email,
                commit_count,
            })
            .collect();
        contributors.sort_by(|a, b| {
            b.commit_count
                .cmp(&a.commit_count)
                .then_with(|| a.email.cmp(&b.email))
        });
        contributors.truncate(MAX_CONTRIBUTORS);
        Ok(Some(contributors))
    }

    /// Like `resolve_revision`, but only yields a commit: annotated tags are peeled
    /// to their commit, and a revision that resolves to a blob or tree (a blob SHA,
    /// `HEAD^{tree}`, `main:README.md`) is `Ok(None)`, same as an unknown one.
    pub fn resolve_commit(
        &self,
        disk_path: &Path,
        revision: &str,
    ) -> Result<Option<gix::ObjectId>, GitReadError> {
        let repo = open(disk_path)?;
        Ok(resolve_commit_in(&repo, revision))
    }

    /// Resolves a SHA, short SHA, branch, tag or `HEAD` to a commit id. `Ok(None)` covers both an
    /// unresolvable spec and a repository without commits.
    pub fn resolve_revision(
        &self,
        disk_path: &Path,
        revision: &str,
    ) -> Result<Option<gix::ObjectId>, GitReadError> {
        let repo = open(disk_path)?;
        Ok(repo
            .rev_parse_single(revision.as_bytes())
            .ok()
            .map(|id| id.detach()))
    }
}

fn resolve_commit_in(repo: &gix::Repository, revision: &str) -> Option<gix::ObjectId> {
    let id = repo.rev_parse_single(revision.as_bytes()).ok()?;
    Some(id.object().ok()?.peel_to_commit().ok()?.id)
}
