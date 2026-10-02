use std::collections::HashMap;
use std::path::Path;

use super::{
    CommitInfo, ContributorInfo, GitReadError, GixRepositoryReader, commit_by_sha, commit_info,
    newest_first_walk, open, walked_commit,
};

/// Commits `list_contributors` walks. Older contributors aren't counted, there is no full-history fallback.
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

    /// `revision` is resolved like `git rev-parse` (branch, tag, SHA, HEAD) and annotated tags are peeled.
    /// HEAD on a repo with no commits gives an empty list, any other revision that doesn't resolve to a commit
    /// is an error, so callers can tell "no history" from "no such ref".
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
            .map(|info| commit_info(&walked_commit(info)?))
            .collect()
    }

    pub fn list_contributors(
        &self,
        disk_path: &Path,
        revision: &str,
    ) -> Result<Option<Vec<ContributorInfo>>, GitReadError> {
        self.list_contributors_with_window(disk_path, revision, CONTRIBUTOR_SEARCH_WINDOW)
    }

    /// With an explicit window, so tests can use a small one.
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
            let commit = walked_commit(info)?;
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

    /// Like `resolve_revision` but commits only: annotated tags are peeled, and a blob or tree (a blob SHA,
    /// `HEAD^{tree}`, `main:README.md`) is `Ok(None)` just like an unknown revision.
    pub fn resolve_commit(
        &self,
        disk_path: &Path,
        revision: &str,
    ) -> Result<Option<gix::ObjectId>, GitReadError> {
        let repo = open(disk_path)?;
        Ok(resolve_commit_in(&repo, revision))
    }

    /// Resolves a SHA, short SHA, branch, tag or HEAD to an object id, without peeling. `Ok(None)` if it
    /// doesn't resolve or the repo has no commits.
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
