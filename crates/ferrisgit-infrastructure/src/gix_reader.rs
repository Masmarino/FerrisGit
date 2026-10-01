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

/// How many commits `list_tree_at_revision` walks per entry to find the one that last touched it.
/// Past this window `last_commit` is `None`. There is no full-history fallback, on purpose.
const LAST_COMMIT_SEARCH_WINDOW: usize = 200;

/// How many commits `list_contributors` walks. Older contributors are not counted (no full-history fallback).
const CONTRIBUTOR_SEARCH_WINDOW: usize = 1000;

/// File entries `compute_language_stats` visits before giving up with `None`, so the frontend
/// shows no language bar rather than a partial one.
const MAX_FILES_FOR_LANGUAGE_STATS: usize = 2000;

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
        let repo = gix::open(disk_path).map_err(|e| GitReadError::Open(e.to_string()))?;
        let Some(head_id) = Self::resolve_commit_in(&repo, revision) else {
            if revision == "HEAD" {
                return Ok(Vec::new());
            }
            return Err(GitReadError::Other(format!(
                "revision does not resolve to a commit: {revision}"
            )));
        };

        let mut commits = Vec::new();
        let walk = repo
            .rev_walk([head_id])
            .sorting(gix::revision::walk::Sorting::ByCommitTime(
                gix::traverse::commit::simple::CommitTimeOrder::NewestFirst,
            ))
            .all()
            .map_err(|e| GitReadError::Other(e.to_string()))?;
        for info in walk.take(limit) {
            let info = info.map_err(|e| GitReadError::Other(e.to_string()))?;
            let commit = info
                .object()
                .map_err(|e| GitReadError::Other(e.to_string()))?;
            let decoded = commit
                .decode()
                .map_err(|e| GitReadError::Other(e.to_string()))?;
            let author = decoded
                .author()
                .map_err(|e| GitReadError::Other(e.to_string()))?;
            commits.push(CommitInfo {
                sha: commit.id.to_string(),
                message: decoded.message().summary().to_string(),
                author_name: author.name.to_string(),
                author_email: author.email.to_string(),
                committed_at: DateTime::from_timestamp(
                    author
                        .time()
                        .map_err(|e| GitReadError::Other(e.to_string()))?
                        .seconds,
                    0,
                )
                .unwrap_or_default(),
            });
        }
        Ok(commits)
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
        let repo = gix::open(disk_path).map_err(|e| GitReadError::Open(e.to_string()))?;
        let Ok(commit_id) = gix::ObjectId::from_hex(revision.as_bytes()) else {
            return Ok(None);
        };
        let Ok(object) = repo.find_object(commit_id) else {
            return Ok(None);
        };
        if object.try_into_commit().is_err() {
            return Ok(None);
        }

        let walk = repo
            .rev_walk([commit_id])
            .sorting(gix::revision::walk::Sorting::ByCommitTime(
                gix::traverse::commit::simple::CommitTimeOrder::NewestFirst,
            ))
            .all()
            .map_err(|e| GitReadError::Other(e.to_string()))?;

        let mut tallies: std::collections::HashMap<(String, String), u32> =
            std::collections::HashMap::new();
        for info in walk.take(window) {
            let info = info.map_err(|e| GitReadError::Other(e.to_string()))?;
            let commit = info
                .object()
                .map_err(|e| GitReadError::Other(e.to_string()))?;
            let decoded = commit
                .decode()
                .map_err(|e| GitReadError::Other(e.to_string()))?;
            let author = decoded
                .author()
                .map_err(|e| GitReadError::Other(e.to_string()))?;
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
        contributors.truncate(20);
        Ok(Some(contributors))
    }

    pub fn compute_language_stats(
        &self,
        disk_path: &Path,
        revision: &str,
    ) -> Result<Option<Vec<LanguageStat>>, GitReadError> {
        self.compute_language_stats_with_limit(disk_path, revision, MAX_FILES_FOR_LANGUAGE_STATS)
    }

    /// `compute_language_stats` with an explicit file limit, so tests can trip the cutoff cheaply.
    pub fn compute_language_stats_with_limit(
        &self,
        disk_path: &Path,
        revision: &str,
        max_files: usize,
    ) -> Result<Option<Vec<LanguageStat>>, GitReadError> {
        let repo = gix::open(disk_path).map_err(|e| GitReadError::Open(e.to_string()))?;
        let Ok(commit_id) = gix::ObjectId::from_hex(revision.as_bytes()) else {
            return Ok(None);
        };
        let Ok(object) = repo.find_object(commit_id) else {
            return Ok(None);
        };
        let Ok(commit) = object.try_into_commit() else {
            return Ok(None);
        };
        let tree = commit
            .tree()
            .map_err(|e| GitReadError::Other(e.to_string()))?;

        let mut bytes_by_language: std::collections::HashMap<&'static str, u64> =
            std::collections::HashMap::new();
        let mut files_visited = 0usize;
        if !walk_tree_for_languages(
            &repo,
            &tree,
            max_files,
            &mut files_visited,
            &mut bytes_by_language,
        )? {
            return Ok(None);
        }

        let total: u64 = bytes_by_language.values().sum();
        if total == 0 {
            return Ok(Some(Vec::new()));
        }

        let mut stats: Vec<LanguageStat> = bytes_by_language
            .into_iter()
            .map(|(name, bytes)| LanguageStat {
                name: name.to_string(),
                bytes,
                percentage: (bytes as f32 / total as f32) * 100.0,
            })
            .collect();
        stats.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.name.cmp(&b.name)));

        if stats.len() > 6 {
            let other_bytes: u64 = stats[6..].iter().map(|s| s.bytes).sum();
            stats.truncate(6);
            stats.push(LanguageStat {
                name: "Other".to_string(),
                bytes: other_bytes,
                percentage: (other_bytes as f32 / total as f32) * 100.0,
            });
        }
        Ok(Some(stats))
    }

    /// Lists the entries of `path` (empty means the repository root) at `revision`, a concrete commit SHA
    /// already resolved by the caller. `Ok(None)` covers an unresolvable revision, a missing path and a
    /// path that is a file, not a directory.
    pub fn list_tree_at_revision(
        &self,
        disk_path: &Path,
        revision: &str,
        path: &str,
    ) -> Result<Option<Vec<TreeEntryInfo>>, GitReadError> {
        self.list_tree_at_revision_with_window(disk_path, revision, path, LAST_COMMIT_SEARCH_WINDOW)
    }

    /// `list_tree_at_revision` without the per-entry last-commit lookup: same entries and order, every
    /// `last_commit` `None`.
    pub fn list_tree_names_at_revision(
        &self,
        disk_path: &Path,
        revision: &str,
        path: &str,
    ) -> Result<Option<Vec<TreeEntryInfo>>, GitReadError> {
        self.list_tree_at_revision_with_window(disk_path, revision, path, 0)
    }

    /// `list_tree_at_revision` with an explicit search window (0 skips the lookup), so tests can use a small one.
    pub fn list_tree_at_revision_with_window(
        &self,
        disk_path: &Path,
        revision: &str,
        path: &str,
        last_commit_window: usize,
    ) -> Result<Option<Vec<TreeEntryInfo>>, GitReadError> {
        let repo = gix::open(disk_path).map_err(|e| GitReadError::Open(e.to_string()))?;
        let Ok(commit_id) = gix::ObjectId::from_hex(revision.as_bytes()) else {
            return Ok(None);
        };
        let Ok(object) = repo.find_object(commit_id) else {
            return Ok(None);
        };
        let Ok(commit) = object.try_into_commit() else {
            return Ok(None);
        };
        let root_tree = commit
            .tree()
            .map_err(|e| GitReadError::Other(e.to_string()))?;

        let tree = if path.is_empty() {
            root_tree
        } else {
            let Some(entry) = root_tree
                .lookup_entry_by_path(path)
                .map_err(|e| GitReadError::Other(e.to_string()))?
            else {
                return Ok(None);
            };
            if !entry.mode().is_tree() {
                return Ok(None);
            }
            let Ok(object) = entry.object() else {
                return Ok(None);
            };
            object.into_tree()
        };

        let mut entries = Vec::new();
        for entry in tree.iter() {
            let entry = entry.map_err(|e| GitReadError::Other(e.to_string()))?;
            let name = entry.filename().to_string();
            let entry_path = if path.is_empty() {
                name.clone()
            } else {
                format!("{path}/{name}")
            };
            let last_commit = if last_commit_window == 0 {
                None
            } else {
                find_last_commit_touching_path(&repo, commit_id, &entry_path, last_commit_window)?
            };
            entries.push(TreeEntryInfo {
                name,
                is_dir: entry.mode().is_tree(),
                last_commit,
            });
        }
        Ok(Some(entries))
    }

    /// `Ok(None)` covers both a missing file and a revision that doesn't exist or isn't a commit, so a
    /// malformed sha never surfaces as an error (and a 500) at the API layer.
    pub fn read_file_at_revision(
        &self,
        disk_path: &Path,
        revision: &str,
        path: &str,
    ) -> Result<Option<Vec<u8>>, GitReadError> {
        let repo = gix::open(disk_path).map_err(|e| GitReadError::Open(e.to_string()))?;
        let Ok(commit_id) = gix::ObjectId::from_hex(revision.as_bytes()) else {
            return Ok(None);
        };
        let Ok(object) = repo.find_object(commit_id) else {
            return Ok(None);
        };
        let Ok(commit) = object.try_into_commit() else {
            return Ok(None);
        };
        let tree = commit
            .tree()
            .map_err(|e| GitReadError::Other(e.to_string()))?;

        let Some(entry) = tree
            .lookup_entry_by_path(path)
            .map_err(|e| GitReadError::Other(e.to_string()))?
        else {
            return Ok(None);
        };
        if entry.mode().is_tree() {
            return Ok(None);
        }
        let blob = entry
            .object()
            .map_err(|e| GitReadError::Other(e.to_string()))?;
        Ok(Some(blob.data.clone()))
    }

    /// Size in bytes of the blob at `path` at `revision`, read from the object header without loading
    /// the content, so oversized blobs can be rejected first. Returns `Ok(None)` in the same cases as
    /// `read_file_at_revision`.
    pub fn blob_size_at_revision(
        &self,
        disk_path: &Path,
        revision: &str,
        path: &str,
    ) -> Result<Option<u64>, GitReadError> {
        let repo = gix::open(disk_path).map_err(|e| GitReadError::Open(e.to_string()))?;
        let Ok(commit_id) = gix::ObjectId::from_hex(revision.as_bytes()) else {
            return Ok(None);
        };
        let Ok(object) = repo.find_object(commit_id) else {
            return Ok(None);
        };
        let Ok(commit) = object.try_into_commit() else {
            return Ok(None);
        };
        let tree = commit
            .tree()
            .map_err(|e| GitReadError::Other(e.to_string()))?;

        let Some(entry) = tree
            .lookup_entry_by_path(path)
            .map_err(|e| GitReadError::Other(e.to_string()))?
        else {
            return Ok(None);
        };
        if entry.mode().is_tree() {
            return Ok(None);
        }
        let header = repo
            .find_header(entry.oid().to_owned())
            .map_err(|e| GitReadError::Other(e.to_string()))?;
        Ok(Some(header.size()))
    }

    /// Like `resolve_revision`, but only yields a commit: annotated tags are peeled
    /// to their commit, and a revision that resolves to a blob or tree (a blob SHA,
    /// `HEAD^{tree}`, `main:README.md`) is `Ok(None)`, same as an unknown one.
    pub fn resolve_commit(
        &self,
        disk_path: &Path,
        revision: &str,
    ) -> Result<Option<gix::ObjectId>, GitReadError> {
        let repo = gix::open(disk_path).map_err(|e| GitReadError::Open(e.to_string()))?;
        Ok(Self::resolve_commit_in(&repo, revision))
    }

    fn resolve_commit_in(repo: &gix::Repository, revision: &str) -> Option<gix::ObjectId> {
        let id = repo.rev_parse_single(revision.as_bytes()).ok()?;
        Some(id.object().ok()?.peel_to_commit().ok()?.id)
    }

    /// Resolves a SHA, short SHA, branch, tag or `HEAD` to a commit id. `Ok(None)` covers both an
    /// unresolvable spec and a repository without commits.
    pub fn resolve_revision(
        &self,
        disk_path: &Path,
        revision: &str,
    ) -> Result<Option<gix::ObjectId>, GitReadError> {
        let repo = gix::open(disk_path).map_err(|e| GitReadError::Open(e.to_string()))?;
        Ok(repo
            .rev_parse_single(revision.as_bytes())
            .ok()
            .map(|id| id.detach()))
    }

    pub fn list_branches(&self, disk_path: &Path) -> Result<Vec<BranchInfoRaw>, GitReadError> {
        let repo = gix::open(disk_path).map_err(|e| GitReadError::Open(e.to_string()))?;
        let default_branch_name = repo
            .head_name()
            .map_err(|e| GitReadError::Other(e.to_string()))?
            .map(|name| name.shorten().to_string());

        let Ok(references) = repo.references() else {
            return Ok(Vec::new());
        };
        let Ok(local_branches) = references.local_branches() else {
            return Ok(Vec::new());
        };

        let mut branches = Vec::new();
        for reference in local_branches {
            let mut reference = reference.map_err(|e| GitReadError::Other(e.to_string()))?;
            let name = reference.name().shorten().to_string();
            let tip_sha = reference
                .peel_to_id()
                .map_err(|e| GitReadError::Other(e.to_string()))?
                .to_string();
            let is_default = default_branch_name.as_deref() == Some(name.as_str());
            branches.push(BranchInfoRaw {
                name,
                tip_sha,
                is_default,
            });
        }
        Ok(branches)
    }

    pub fn list_tags(&self, disk_path: &Path) -> Result<Vec<TagInfoRaw>, GitReadError> {
        let repo = gix::open(disk_path).map_err(|e| GitReadError::Open(e.to_string()))?;
        let Ok(references) = repo.references() else {
            return Ok(Vec::new());
        };
        let Ok(tags) = references.tags() else {
            return Ok(Vec::new());
        };

        let mut result = Vec::new();
        for reference in tags {
            let mut reference = reference.map_err(|e| GitReadError::Other(e.to_string()))?;
            let name = reference.name().shorten().to_string();
            let target_sha = reference
                .peel_to_id()
                .map_err(|e| GitReadError::Other(e.to_string()))?
                .to_string();
            result.push(TagInfoRaw { name, target_sha });
        }
        Ok(result)
    }

    /// Callers only get here after checking that a `wikis` row exists, so a `gix::open` failure is a
    /// real `GitReadError` (disk and Postgres disagree). Only a repo with zero commits yields `Ok(None)`.
    pub fn wiki_head_sha(&self, disk_path: &Path) -> Result<Option<String>, GitReadError> {
        let repo = gix::open(disk_path).map_err(|e| GitReadError::Open(e.to_string()))?;
        Ok(repo.head_id().ok().map(|id| id.to_string()))
    }

    pub fn list_wiki_pages(&self, disk_path: &Path) -> Result<Vec<WikiPageInfoRaw>, GitReadError> {
        let repo = gix::open(disk_path).map_err(|e| GitReadError::Open(e.to_string()))?;
        let Ok(head_id) = repo.head_id() else {
            return Ok(Vec::new());
        };
        let commit = head_id
            .object()
            .map_err(|e| GitReadError::Other(e.to_string()))?
            .into_commit();
        let tree = commit
            .tree()
            .map_err(|e| GitReadError::Other(e.to_string()))?;

        let mut pages = Vec::new();
        for entry in tree.iter() {
            let entry = entry.map_err(|e| GitReadError::Other(e.to_string()))?;
            if entry.mode().is_tree() {
                continue; // flat namespace — ignore any subdirectory a raw `git push` might have created
            }
            let name = entry.filename().to_string();
            if let Some(slug) = name.strip_suffix(".md") {
                pages.push(WikiPageInfoRaw {
                    slug: slug.to_string(),
                });
            }
        }
        Ok(pages)
    }

    pub fn read_wiki_page(
        &self,
        disk_path: &Path,
        slug: &str,
    ) -> Result<Option<WikiPageContentRaw>, GitReadError> {
        let repo = gix::open(disk_path).map_err(|e| GitReadError::Open(e.to_string()))?;
        let Ok(head_id) = repo.head_id() else {
            return Ok(None);
        };
        let commit = head_id
            .object()
            .map_err(|e| GitReadError::Other(e.to_string()))?
            .into_commit();
        let tree = commit
            .tree()
            .map_err(|e| GitReadError::Other(e.to_string()))?;

        let file_name = format!("{slug}.md");
        let Some(entry) = tree
            .lookup_entry_by_path(&file_name)
            .map_err(|e| GitReadError::Other(e.to_string()))?
        else {
            return Ok(None);
        };
        if entry.mode().is_tree() {
            return Ok(None);
        }
        let blob = entry
            .object()
            .map_err(|e| GitReadError::Other(e.to_string()))?;
        let content = String::from_utf8_lossy(&blob.data).to_string();
        Ok(Some(WikiPageContentRaw {
            content,
            head_sha: head_id.to_string(),
        }))
    }

    /// Commits that changed `{slug}.md`, newest first: the blob differs from the first parent's
    /// (added or modified). A commit that only removed the page is not included.
    pub fn list_wiki_page_revisions(
        &self,
        disk_path: &Path,
        slug: &str,
    ) -> Result<Vec<WikiRevisionRaw>, GitReadError> {
        let repo = gix::open(disk_path).map_err(|e| GitReadError::Open(e.to_string()))?;
        let Ok(head_id) = repo.head_id() else {
            return Ok(Vec::new());
        };
        let file_name = format!("{slug}.md");

        let walk = repo
            .rev_walk([head_id])
            .sorting(gix::revision::walk::Sorting::ByCommitTime(
                gix::traverse::commit::simple::CommitTimeOrder::NewestFirst,
            ))
            .all()
            .map_err(|e| GitReadError::Other(e.to_string()))?;

        let mut revisions = Vec::new();
        for info in walk {
            let info = info.map_err(|e| GitReadError::Other(e.to_string()))?;
            let commit = info
                .object()
                .map_err(|e| GitReadError::Other(e.to_string()))?;
            let tree = commit
                .tree()
                .map_err(|e| GitReadError::Other(e.to_string()))?;
            let Some(current_blob) = tree
                .lookup_entry_by_path(&file_name)
                .map_err(|e| GitReadError::Other(e.to_string()))?
                .map(|e| e.object_id())
            else {
                continue; // page doesn't exist at this commit at all
            };

            let parent_blob = match commit.parent_ids().next() {
                Some(parent_id) => {
                    let parent_commit = repo
                        .find_object(parent_id)
                        .map_err(|e| GitReadError::Other(e.to_string()))?
                        .try_into_commit()
                        .map_err(|e| GitReadError::Other(e.to_string()))?;
                    let parent_tree = parent_commit
                        .tree()
                        .map_err(|e| GitReadError::Other(e.to_string()))?;
                    parent_tree
                        .lookup_entry_by_path(&file_name)
                        .map_err(|e| GitReadError::Other(e.to_string()))?
                        .map(|e| e.object_id())
                }
                None => None, // root commit — nothing to compare against, so it counts as a change
            };

            if Some(current_blob) != parent_blob {
                let decoded = commit
                    .decode()
                    .map_err(|e| GitReadError::Other(e.to_string()))?;
                let author = decoded
                    .author()
                    .map_err(|e| GitReadError::Other(e.to_string()))?;
                revisions.push(WikiRevisionRaw {
                    commit_sha: commit.id.to_string(),
                    author_name: author.name.to_string(),
                    author_email: author.email.to_string(),
                    committed_at: DateTime::from_timestamp(
                        author
                            .time()
                            .map_err(|e| GitReadError::Other(e.to_string()))?
                            .seconds,
                        0,
                    )
                    .unwrap_or_default(),
                    message: decoded.message().summary().to_string(),
                });
            }
        }
        Ok(revisions)
    }

    /// Diffs merge-base(`source_branch`, `target_branch`) to `source_branch`'s tip by walking both trees
    /// into `BTreeMap` snapshots (gix's tree-diff feature is not enabled). Line diffs use `similar`.
    pub fn diff_branches(
        &self,
        disk_path: &Path,
        source_branch: &str,
        target_branch: &str,
    ) -> Result<Vec<FileDiffRaw>, GitReadError> {
        let repo = gix::open(disk_path).map_err(|e| GitReadError::Open(e.to_string()))?;
        let source_id = repo
            .find_reference(&format!("refs/heads/{source_branch}"))
            .map_err(|e| GitReadError::Other(e.to_string()))?
            .peel_to_id()
            .map_err(|e| GitReadError::Other(e.to_string()))?;
        let target_id = repo
            .find_reference(&format!("refs/heads/{target_branch}"))
            .map_err(|e| GitReadError::Other(e.to_string()))?
            .peel_to_id()
            .map_err(|e| GitReadError::Other(e.to_string()))?;
        let merge_base_id = repo
            .merge_base(source_id, target_id)
            .map_err(|e| GitReadError::Other(e.to_string()))?;

        let base_tree = merge_base_id
            .object()
            .map_err(|e| GitReadError::Other(e.to_string()))?
            .into_commit()
            .tree()
            .map_err(|e| GitReadError::Other(e.to_string()))?;
        let source_tree = source_id
            .object()
            .map_err(|e| GitReadError::Other(e.to_string()))?
            .into_commit()
            .tree()
            .map_err(|e| GitReadError::Other(e.to_string()))?;

        let mut base_blobs = std::collections::BTreeMap::new();
        collect_blobs(&base_tree, "", &mut base_blobs)?;
        let mut source_blobs = std::collections::BTreeMap::new();
        collect_blobs(&source_tree, "", &mut source_blobs)?;

        let paths: std::collections::BTreeSet<String> = base_blobs
            .keys()
            .chain(source_blobs.keys())
            .cloned()
            .collect();
        let mut diffs = Vec::new();
        for path in paths {
            let base_blob_id = base_blobs.get(&path).copied();
            let source_blob_id = source_blobs.get(&path).copied();
            if base_blob_id == source_blob_id {
                continue;
            }

            let (change, old_content, new_content) = match (base_blob_id, source_blob_id) {
                (None, Some(id)) => (FileChangeKindRaw::Added, None, Some(read_blob(&repo, id)?)),
                (Some(id), None) => (
                    FileChangeKindRaw::Deleted,
                    Some(read_blob(&repo, id)?),
                    None,
                ),
                (Some(old_id), Some(new_id)) => (
                    FileChangeKindRaw::Modified,
                    Some(read_blob(&repo, old_id)?),
                    Some(read_blob(&repo, new_id)?),
                ),
                (None, None) => unreachable!("path came from at least one of the two maps"),
            };

            let is_binary = [&old_content, &new_content]
                .iter()
                .filter_map(|c| c.as_ref())
                .any(|c| c.contains(&0));
            if is_binary {
                diffs.push(FileDiffRaw {
                    path,
                    change: FileChangeKindRaw::Binary,
                    hunks: vec![],
                });
                continue;
            }

            let old_text = old_content
                .as_deref()
                .map(String::from_utf8_lossy)
                .unwrap_or_default();
            let new_text = new_content
                .as_deref()
                .map(String::from_utf8_lossy)
                .unwrap_or_default();
            let text_diff = similar::TextDiff::from_lines(old_text.as_ref(), new_text.as_ref());
            let lines: Vec<DiffLineRaw> = text_diff
                .iter_all_changes()
                .map(|change| {
                    let kind = match change.tag() {
                        similar::ChangeTag::Equal => DiffLineKindRaw::Context,
                        similar::ChangeTag::Insert => DiffLineKindRaw::Added,
                        similar::ChangeTag::Delete => DiffLineKindRaw::Removed,
                    };
                    DiffLineRaw {
                        kind,
                        content: change.to_string(),
                        old_line: change.old_index().map(|i| i as u32 + 1),
                        new_line: change.new_index().map(|i| i as u32 + 1),
                    }
                })
                .collect();
            diffs.push(FileDiffRaw {
                path,
                change,
                hunks: vec![HunkRaw { lines }],
            });
        }
        Ok(diffs)
    }
}

fn language_for_extension(file_name: &str) -> Option<&'static str> {
    let extension = file_name.rsplit('.').next().unwrap_or("").to_lowercase();
    match extension.as_str() {
        "rs" => Some("Rust"),
        "ts" | "tsx" => Some("TypeScript"),
        "js" | "jsx" => Some("JavaScript"),
        "html" => Some("HTML"),
        "scss" | "css" => Some("CSS"),
        "py" => Some("Python"),
        "go" => Some("Go"),
        "java" => Some("Java"),
        "c" | "h" => Some("C"),
        "cpp" | "hpp" => Some("C++"),
        "rb" => Some("Ruby"),
        "sh" | "bash" => Some("Shell"),
        "sql" => Some("SQL"),
        "md" => Some("Markdown"),
        "json" => Some("JSON"),
        "yml" | "yaml" => Some("YAML"),
        _ => None,
    }
}

/// Recursively sums bytes per language across `tree` using `find_header` (no content decode).
/// Returns `Ok(false)` as soon as `*files_visited` exceeds `max_files`.
fn walk_tree_for_languages(
    repo: &gix::Repository,
    tree: &gix::Tree,
    max_files: usize,
    files_visited: &mut usize,
    bytes_by_language: &mut std::collections::HashMap<&'static str, u64>,
) -> Result<bool, GitReadError> {
    for entry in tree.iter() {
        let entry = entry.map_err(|e| GitReadError::Other(e.to_string()))?;
        if entry.mode().is_tree() {
            let Ok(object) = entry.object() else { continue };
            if !walk_tree_for_languages(
                repo,
                &object.into_tree(),
                max_files,
                files_visited,
                bytes_by_language,
            )? {
                return Ok(false);
            }
            continue;
        }
        *files_visited += 1;
        if *files_visited > max_files {
            return Ok(false);
        }
        if let Some(language) = language_for_extension(&entry.filename().to_string()) {
            let header = repo
                .find_header(entry.oid().to_owned())
                .map_err(|e| GitReadError::Other(e.to_string()))?;
            *bytes_by_language.entry(language).or_insert(0) += header.size();
        }
    }
    Ok(true)
}

/// The newest commit among the `window` most recent from `start_id` whose tree at `path` differs from
/// its first parent's (a root commit counts as differing). Short-circuits on the first hit.
fn find_last_commit_touching_path(
    repo: &gix::Repository,
    start_id: gix::ObjectId,
    path: &str,
    window: usize,
) -> Result<Option<CommitInfo>, GitReadError> {
    let walk = repo
        .rev_walk([start_id])
        .sorting(gix::revision::walk::Sorting::ByCommitTime(
            gix::traverse::commit::simple::CommitTimeOrder::NewestFirst,
        ))
        .all()
        .map_err(|e| GitReadError::Other(e.to_string()))?;

    for info in walk.take(window) {
        let info = info.map_err(|e| GitReadError::Other(e.to_string()))?;
        let commit = info
            .object()
            .map_err(|e| GitReadError::Other(e.to_string()))?;
        let tree = commit
            .tree()
            .map_err(|e| GitReadError::Other(e.to_string()))?;
        let current = tree
            .lookup_entry_by_path(path)
            .map_err(|e| GitReadError::Other(e.to_string()))?
            .map(|e| e.object_id());

        let parent = match commit.parent_ids().next() {
            Some(parent_id) => {
                let Ok(parent_object) = repo.find_object(parent_id) else {
                    continue;
                };
                let Ok(parent_commit) = parent_object.try_into_commit() else {
                    continue;
                };
                let parent_tree = parent_commit
                    .tree()
                    .map_err(|e| GitReadError::Other(e.to_string()))?;
                parent_tree
                    .lookup_entry_by_path(path)
                    .map_err(|e| GitReadError::Other(e.to_string()))?
                    .map(|e| e.object_id())
            }
            None => None,
        };

        if current != parent {
            let decoded = commit
                .decode()
                .map_err(|e| GitReadError::Other(e.to_string()))?;
            let author = decoded
                .author()
                .map_err(|e| GitReadError::Other(e.to_string()))?;
            return Ok(Some(CommitInfo {
                sha: commit.id.to_string(),
                message: decoded.message().summary().to_string(),
                author_name: author.name.to_string(),
                author_email: author.email.to_string(),
                committed_at: DateTime::from_timestamp(
                    author
                        .time()
                        .map_err(|e| GitReadError::Other(e.to_string()))?
                        .seconds,
                    0,
                )
                .unwrap_or_default(),
            }));
        }
    }
    Ok(None)
}

fn collect_blobs(
    tree: &gix::Tree,
    prefix: &str,
    out: &mut std::collections::BTreeMap<String, gix::ObjectId>,
) -> Result<(), GitReadError> {
    for entry in tree.iter() {
        let entry = entry.map_err(|e| GitReadError::Other(e.to_string()))?;
        let full_path = if prefix.is_empty() {
            entry.filename().to_string()
        } else {
            format!("{prefix}/{}", entry.filename())
        };
        if entry.mode().is_tree() {
            let subtree = entry
                .object()
                .map_err(|e| GitReadError::Other(e.to_string()))?
                .into_tree();
            collect_blobs(&subtree, &full_path, out)?;
        } else {
            out.insert(full_path, entry.oid().to_owned());
        }
    }
    Ok(())
}

fn read_blob(repo: &gix::Repository, id: gix::ObjectId) -> Result<Vec<u8>, GitReadError> {
    Ok(repo
        .find_object(id)
        .map_err(|e| GitReadError::Other(e.to_string()))?
        .data
        .clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn init_repo_with_one_commit(dir: &Path) {
        let run = |args: &[&str]| {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(dir)
                    .status()
                    .unwrap()
                    .success()
            )
        };
        run(&["init", "-q", "-b", "main"]);
        run(&[
            "-c",
            "user.email=t@t.com",
            "-c",
            "user.name=t",
            "commit",
            "--allow-empty",
            "-q",
            "-m",
            "first commit",
        ]);
        std::fs::write(dir.join("README.md"), "# hello").unwrap();
        run(&["add", "."]);
        run(&[
            "-c",
            "user.email=t@t.com",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            "add readme",
        ]);
    }

    /// Builds a merge whose first parent is older than its second. `BreadthFirst` sorting would return
    /// the older first parent first, while `ByCommitTime(NewestFirst)` must return the newer feature
    /// commit first.
    fn init_repo_with_merge_commit(dir: &Path) {
        let run = |args: &[&str]| {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(dir)
                    .status()
                    .unwrap()
                    .success()
            )
        };
        let commit = |message: &str, date: &str| {
            assert!(
                Command::new("git")
                    .args([
                        "-c",
                        "user.email=t@t.com",
                        "-c",
                        "user.name=t",
                        "commit",
                        "--allow-empty",
                        "-q",
                        "-m",
                        message
                    ])
                    .env("GIT_AUTHOR_DATE", date)
                    .env("GIT_COMMITTER_DATE", date)
                    .current_dir(dir)
                    .status()
                    .unwrap()
                    .success()
            );
        };

        run(&["init", "-q", "-b", "main"]);
        commit("root", "2024-01-01T00:00:00+00:00");
        run(&["checkout", "-q", "-b", "feature"]);
        commit("feature commit", "2024-01-01T02:00:00+00:00");
        run(&["checkout", "-q", "main"]);
        commit("main commit", "2024-01-01T01:00:00+00:00");
        run(&[
            "-c",
            "user.email=t@t.com",
            "-c",
            "user.name=t",
            "merge",
            "-q",
            "--no-ff",
            "feature",
            "-m",
            "merge feature",
        ]);
    }

    #[test]
    fn list_commits_orders_by_commit_time_not_by_parent_position_for_merge_commits() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_merge_commit(tmp.path());

        let reader = GixRepositoryReader;
        let commits = reader.list_commits(tmp.path(), 10).unwrap();

        let messages: Vec<&str> = commits.iter().map(|c| c.message.as_str()).collect();
        assert_eq!(
            messages,
            vec!["merge feature", "feature commit", "main commit", "root"]
        );
    }

    #[test]
    fn list_commits_returns_them_newest_first() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_one_commit(tmp.path());

        let reader = GixRepositoryReader;
        let commits = reader.list_commits(tmp.path(), 10).unwrap();

        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].message, "add readme");
        assert_eq!(commits[1].message, "first commit");
    }

    #[test]
    fn list_commits_on_an_empty_repository_returns_an_empty_list() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(
            Command::new("git")
                .args(["init", "-q"])
                .current_dir(tmp.path())
                .status()
                .unwrap()
                .success()
        );

        let reader = GixRepositoryReader;
        assert!(reader.list_commits(tmp.path(), 10).unwrap().is_empty());
    }

    fn git(dir: &Path, args: &[&str]) {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(dir)
                .status()
                .unwrap()
                .success(),
            "git {args:?} failed"
        );
    }

    /// `init_repo_with_one_commit` plus a `feature` branch with one extra commit and a `v1` tag on `main`'s first commit.
    fn init_repo_with_branch_and_tag(dir: &Path) {
        init_repo_with_one_commit(dir);
        git(dir, &["tag", "v1", "HEAD~1"]);
        git(dir, &["checkout", "-q", "-b", "feature"]);
        git(
            dir,
            &[
                "-c",
                "user.email=t@t.com",
                "-c",
                "user.name=t",
                "commit",
                "--allow-empty",
                "-q",
                "-m",
                "feature work",
            ],
        );
        git(dir, &["checkout", "-q", "-"]);
    }

    fn messages(commits: &[CommitInfo]) -> Vec<&str> {
        commits.iter().map(|c| c.message.as_str()).collect()
    }

    #[test]
    fn list_commits_at_a_branch_returns_that_branchs_history() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_branch_and_tag(tmp.path());
        let reader = GixRepositoryReader;

        let commits = reader.list_commits_at(tmp.path(), "feature", 10).unwrap();

        assert_eq!(
            messages(&commits),
            vec!["feature work", "add readme", "first commit"]
        );
        assert_ne!(
            messages(&commits),
            messages(&reader.list_commits(tmp.path(), 10).unwrap())
        );
    }

    #[test]
    fn list_commits_at_a_tag_returns_the_history_up_to_the_tag() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_branch_and_tag(tmp.path());
        let reader = GixRepositoryReader;

        let commits = reader.list_commits_at(tmp.path(), "v1", 10).unwrap();

        assert_eq!(messages(&commits), vec!["first commit"]);
    }

    #[test]
    fn list_commits_at_an_annotated_tag_peels_to_its_commit() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_branch_and_tag(tmp.path());
        git(
            tmp.path(),
            &[
                "-c",
                "user.email=t@t.com",
                "-c",
                "user.name=t",
                "tag",
                "-a",
                "v2",
                "-m",
                "release",
                "feature",
            ],
        );
        let reader = GixRepositoryReader;

        let commits = reader.list_commits_at(tmp.path(), "v2", 10).unwrap();

        assert_eq!(
            messages(&commits),
            vec!["feature work", "add readme", "first commit"]
        );
    }

    #[test]
    fn list_commits_at_head_equals_list_commits() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_branch_and_tag(tmp.path());
        let reader = GixRepositoryReader;

        assert_eq!(
            reader.list_commits_at(tmp.path(), "HEAD", 10).unwrap(),
            reader.list_commits(tmp.path(), 10).unwrap()
        );
    }

    #[test]
    fn list_commits_at_honours_the_limit() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_branch_and_tag(tmp.path());
        let reader = GixRepositoryReader;

        let commits = reader.list_commits_at(tmp.path(), "feature", 1).unwrap();

        assert_eq!(messages(&commits), vec!["feature work"]);
    }

    #[test]
    fn list_commits_at_an_unknown_revision_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_branch_and_tag(tmp.path());
        let reader = GixRepositoryReader;

        assert!(matches!(
            reader.list_commits_at(tmp.path(), "no-such-branch", 10),
            Err(GitReadError::Other(_))
        ));
    }

    fn rev_parse(dir: &Path, spec: &str) -> String {
        let out = Command::new("git")
            .args(["rev-parse", spec])
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(out.status.success());
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    #[test]
    fn list_commits_at_a_non_commit_or_hostile_revision_is_an_error_not_a_panic() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_branch_and_tag(tmp.path());
        let reader = GixRepositoryReader;
        let blob_sha = rev_parse(tmp.path(), "HEAD:README.md");
        let tree_sha = rev_parse(tmp.path(), "HEAD^{tree}");

        for revision in [
            blob_sha.as_str(),
            tree_sha.as_str(),
            "HEAD^{tree}",
            "HEAD:README.md",
            "--all",
            "../..",
            "with\0nul",
            "",
            "main..feature",
        ] {
            assert!(
                matches!(
                    reader.list_commits_at(tmp.path(), revision, 10),
                    Err(GitReadError::Other(_))
                ),
                "revision {revision:?} should be an error"
            );
        }
    }

    #[test]
    fn resolve_commit_returns_the_commit_for_refs_and_none_for_non_commits() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_branch_and_tag(tmp.path());
        git(
            tmp.path(),
            &[
                "-c",
                "user.email=t@t.com",
                "-c",
                "user.name=t",
                "tag",
                "-a",
                "v2",
                "-m",
                "release",
                "feature",
            ],
        );
        let reader = GixRepositoryReader;

        assert_eq!(
            reader
                .resolve_commit(tmp.path(), "feature")
                .unwrap()
                .unwrap()
                .to_string(),
            rev_parse(tmp.path(), "feature")
        );
        assert_eq!(
            reader
                .resolve_commit(tmp.path(), "v2")
                .unwrap()
                .unwrap()
                .to_string(),
            rev_parse(tmp.path(), "feature")
        );
        assert_eq!(
            reader
                .resolve_commit(tmp.path(), &rev_parse(tmp.path(), "HEAD:README.md"))
                .unwrap(),
            None
        );
        assert_eq!(
            reader.resolve_commit(tmp.path(), "HEAD^{tree}").unwrap(),
            None
        );
        assert_eq!(reader.resolve_commit(tmp.path(), "nope").unwrap(), None);
    }

    #[test]
    fn list_commits_at_head_on_an_empty_repository_returns_an_empty_list() {
        let tmp = tempfile::tempdir().unwrap();
        git(tmp.path(), &["init", "-q"]);
        let reader = GixRepositoryReader;

        assert!(
            reader
                .list_commits_at(tmp.path(), "HEAD", 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn list_tree_at_revision_returns_the_root_entries_when_path_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_one_commit(tmp.path());
        let reader = GixRepositoryReader;
        let sha = reader.list_commits(tmp.path(), 1).unwrap()[0].sha.clone();

        let entries = reader
            .list_tree_at_revision(tmp.path(), &sha, "")
            .unwrap()
            .unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "README.md");
        assert!(!entries[0].is_dir);
    }

    #[test]
    fn list_tree_at_revision_lists_a_nested_subdirectory() {
        let tmp = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(tmp.path())
                    .status()
                    .unwrap()
                    .success()
            )
        };
        run(&["init", "-q"]);
        std::fs::create_dir_all(tmp.path().join("src/lib")).unwrap();
        std::fs::write(tmp.path().join("src/lib/foo.rs"), "fn foo() {}").unwrap();
        std::fs::write(tmp.path().join("src/main.rs"), "fn main() {}").unwrap();
        run(&["add", "."]);
        run(&[
            "-c",
            "user.email=t@t.com",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            "seed",
        ]);
        let reader = GixRepositoryReader;
        let sha = reader.list_commits(tmp.path(), 1).unwrap()[0].sha.clone();

        let root = reader
            .list_tree_at_revision(tmp.path(), &sha, "")
            .unwrap()
            .unwrap();
        assert_eq!(root.len(), 1);
        assert_eq!(root[0].name, "src");
        assert!(root[0].is_dir);

        let nested = reader
            .list_tree_at_revision(tmp.path(), &sha, "src/lib")
            .unwrap()
            .unwrap();
        assert_eq!(nested.len(), 1);
        assert_eq!(nested[0].name, "foo.rs");
        assert!(!nested[0].is_dir);
    }

    #[test]
    fn list_tree_at_revision_returns_none_for_a_path_that_does_not_exist() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_one_commit(tmp.path());
        let reader = GixRepositoryReader;
        let sha = reader.list_commits(tmp.path(), 1).unwrap()[0].sha.clone();

        assert_eq!(
            reader
                .list_tree_at_revision(tmp.path(), &sha, "does/not/exist")
                .unwrap(),
            None
        );
    }

    #[test]
    fn list_tree_at_revision_returns_none_when_the_path_is_a_file_not_a_directory() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_one_commit(tmp.path());
        let reader = GixRepositoryReader;
        let sha = reader.list_commits(tmp.path(), 1).unwrap()[0].sha.clone();

        assert_eq!(
            reader
                .list_tree_at_revision(tmp.path(), &sha, "README.md")
                .unwrap(),
            None
        );
    }

    #[test]
    fn list_tree_at_revision_returns_none_for_a_nonexistent_revision() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_one_commit(tmp.path());
        let reader = GixRepositoryReader;

        assert_eq!(
            reader
                .list_tree_at_revision(tmp.path(), &"0".repeat(40), "")
                .unwrap(),
            None
        );
    }

    #[test]
    fn read_file_at_revision_returns_the_files_content_at_that_commit() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_one_commit(tmp.path());
        let reader = GixRepositoryReader;
        let sha = reader.list_commits(tmp.path(), 1).unwrap()[0].sha.clone();

        let content = reader
            .read_file_at_revision(tmp.path(), &sha, "README.md")
            .unwrap();

        assert_eq!(content, Some(b"# hello".to_vec()));
    }

    #[test]
    fn read_file_at_revision_returns_none_for_a_missing_file() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_one_commit(tmp.path());
        let reader = GixRepositoryReader;
        let sha = reader.list_commits(tmp.path(), 1).unwrap()[0].sha.clone();

        let content = reader
            .read_file_at_revision(tmp.path(), &sha, "does-not-exist.yml")
            .unwrap();

        assert_eq!(content, None);
    }

    #[test]
    fn read_file_at_revision_returns_none_for_a_nonexistent_commit_sha_rather_than_erroring() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_one_commit(tmp.path());
        let reader = GixRepositoryReader;

        let content = reader
            .read_file_at_revision(tmp.path(), &"0".repeat(40), "README.md")
            .unwrap();
        assert_eq!(content, None);

        let content = reader
            .read_file_at_revision(tmp.path(), "not-a-sha", "README.md")
            .unwrap();
        assert_eq!(content, None);
    }

    #[test]
    fn read_file_at_revision_returns_none_when_the_path_is_a_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(tmp.path())
                    .status()
                    .unwrap()
                    .success()
            )
        };
        run(&["init", "-q"]);
        std::fs::create_dir(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/main.rs"), "fn main() {}").unwrap();
        run(&["add", "."]);
        run(&[
            "-c",
            "user.email=t@t.com",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            "add src",
        ]);
        let reader = GixRepositoryReader;
        let sha = reader.list_commits(tmp.path(), 1).unwrap()[0].sha.clone();

        let content = reader
            .read_file_at_revision(tmp.path(), &sha, "src")
            .unwrap();

        assert_eq!(content, None);
    }

    #[test]
    fn blob_size_at_revision_returns_the_files_size_at_that_commit() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_one_commit(tmp.path());
        let reader = GixRepositoryReader;
        let sha = reader.list_commits(tmp.path(), 1).unwrap()[0].sha.clone();
        let content = reader
            .read_file_at_revision(tmp.path(), &sha, "README.md")
            .unwrap()
            .unwrap();

        let size = reader
            .blob_size_at_revision(tmp.path(), &sha, "README.md")
            .unwrap();

        assert_eq!(size, Some(content.len() as u64));
    }

    #[test]
    fn blob_size_at_revision_returns_none_when_the_path_is_a_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(tmp.path())
                    .status()
                    .unwrap()
                    .success()
            )
        };
        run(&["init", "-q"]);
        std::fs::create_dir(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/main.rs"), "fn main() {}").unwrap();
        run(&["add", "."]);
        run(&[
            "-c",
            "user.email=t@t.com",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            "add src",
        ]);
        let reader = GixRepositoryReader;
        let sha = reader.list_commits(tmp.path(), 1).unwrap()[0].sha.clone();

        let size = reader
            .blob_size_at_revision(tmp.path(), &sha, "src")
            .unwrap();

        assert_eq!(size, None);
    }

    #[test]
    fn list_branches_returns_every_local_branch_with_the_defaults_tip() {
        let tmp = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(tmp.path())
                    .status()
                    .unwrap()
                    .success()
            )
        };
        run(&["init", "-q", "-b", "main"]);
        run(&[
            "-c",
            "user.email=t@t.com",
            "-c",
            "user.name=t",
            "commit",
            "--allow-empty",
            "-q",
            "-m",
            "root",
        ]);
        run(&["checkout", "-q", "-b", "feature"]);
        run(&[
            "-c",
            "user.email=t@t.com",
            "-c",
            "user.name=t",
            "commit",
            "--allow-empty",
            "-q",
            "-m",
            "feature work",
        ]);
        // `is_default` is a live read of HEAD, so switch back to `main` after `checkout -b feature`.
        run(&["checkout", "-q", "main"]);

        let reader = GixRepositoryReader;
        let branches = reader.list_branches(tmp.path()).unwrap();

        let names: Vec<&str> = branches.iter().map(|b| b.name.as_str()).collect();
        assert!(names.contains(&"main"));
        assert!(names.contains(&"feature"));
        let main = branches.iter().find(|b| b.name == "main").unwrap();
        assert!(
            main.is_default,
            "checked-out-at-init branch (main) must be the default"
        );
        let feature = branches.iter().find(|b| b.name == "feature").unwrap();
        assert!(!feature.is_default);
        assert_ne!(
            main.tip_sha, feature.tip_sha,
            "the two branches point at different commits"
        );
    }

    #[test]
    fn list_branches_on_an_empty_repository_returns_an_empty_list() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(
            Command::new("git")
                .args(["init", "-q"])
                .current_dir(tmp.path())
                .status()
                .unwrap()
                .success()
        );

        let reader = GixRepositoryReader;
        assert!(reader.list_branches(tmp.path()).unwrap().is_empty());
    }

    #[test]
    fn list_tags_returns_every_tag_with_its_target_commit() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_one_commit(tmp.path());
        std::process::Command::new("git")
            .args(["tag", "v1.0.0"])
            .current_dir(tmp.path())
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["tag", "v2.0.0-rc1"])
            .current_dir(tmp.path())
            .status()
            .unwrap();

        let reader = GixRepositoryReader;
        let mut tags = reader.list_tags(tmp.path()).unwrap();
        tags.sort_by(|a, b| a.name.cmp(&b.name));

        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].name, "v1.0.0");
        assert_eq!(tags[1].name, "v2.0.0-rc1");
        assert_eq!(
            tags[0].target_sha, tags[1].target_sha,
            "both tags point at the same, only, commit"
        );
    }

    #[test]
    fn list_tags_on_a_repository_with_no_tags_returns_an_empty_list() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(
            Command::new("git")
                .args(["init", "-q"])
                .current_dir(tmp.path())
                .status()
                .unwrap()
                .success()
        );

        let reader = GixRepositoryReader;
        assert!(reader.list_tags(tmp.path()).unwrap().is_empty());
    }

    #[test]
    fn list_wiki_pages_returns_every_markdown_file_at_the_root_ignoring_invalid_names_and_subdirectories()
     {
        let tmp = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(tmp.path())
                    .status()
                    .unwrap()
                    .success()
            )
        };
        run(&["init", "-q"]);
        std::fs::write(tmp.path().join("Home.md"), "# Home").unwrap();
        std::fs::write(tmp.path().join("Getting-Started.md"), "# Start").unwrap();
        std::fs::write(tmp.path().join("not-a-page.txt"), "ignored").unwrap();
        std::fs::create_dir(tmp.path().join("subdir")).unwrap();
        std::fs::write(tmp.path().join("subdir/Nested.md"), "ignored").unwrap();
        run(&["add", "."]);
        run(&[
            "-c",
            "user.email=t@t.com",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            "seed",
        ]);

        let reader = GixRepositoryReader;
        let mut pages = reader.list_wiki_pages(tmp.path()).unwrap();
        pages.sort_by(|a, b| a.slug.cmp(&b.slug));

        assert_eq!(
            pages.iter().map(|p| p.slug.as_str()).collect::<Vec<_>>(),
            vec!["Getting-Started", "Home"]
        );
    }

    #[test]
    fn list_wiki_pages_on_an_empty_repo_returns_an_empty_list() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(
            Command::new("git")
                .args(["init", "-q"])
                .current_dir(tmp.path())
                .status()
                .unwrap()
                .success()
        );

        let reader = GixRepositoryReader;
        assert!(reader.list_wiki_pages(tmp.path()).unwrap().is_empty());
    }

    #[test]
    fn wiki_head_sha_is_none_for_a_bare_repo_with_no_commits_yet() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            Command::new("git")
                .args(["init", "-q", "--bare"])
                .current_dir(dir.path())
                .status()
                .unwrap()
                .success()
        );

        let reader = GixRepositoryReader;
        assert_eq!(reader.wiki_head_sha(dir.path()).unwrap(), None);
    }

    #[test]
    fn wiki_head_sha_returns_the_head_commit_sha_once_a_commit_exists() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_one_commit(tmp.path());

        let reader = GixRepositoryReader;
        let sha = reader.wiki_head_sha(tmp.path()).unwrap();

        assert_eq!(
            sha,
            Some(reader.list_commits(tmp.path(), 1).unwrap()[0].sha.clone())
        );
    }

    #[test]
    fn read_wiki_page_returns_none_when_the_page_does_not_exist() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_one_commit(tmp.path());

        let reader = GixRepositoryReader;
        assert!(
            reader
                .read_wiki_page(tmp.path(), "Nonexistent")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn read_wiki_page_returns_the_content_and_head_sha() {
        let tmp = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(tmp.path())
                    .status()
                    .unwrap()
                    .success()
            )
        };
        run(&["init", "-q"]);
        std::fs::write(tmp.path().join("Home.md"), "# Hello wiki").unwrap();
        run(&["add", "."]);
        run(&[
            "-c",
            "user.email=t@t.com",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            "add home page",
        ]);

        let reader = GixRepositoryReader;
        let head_sha = reader.wiki_head_sha(tmp.path()).unwrap().unwrap();
        let page = reader.read_wiki_page(tmp.path(), "Home").unwrap().unwrap();

        assert_eq!(page.content, "# Hello wiki");
        assert_eq!(page.head_sha, head_sha);
    }

    #[test]
    fn list_wiki_page_revisions_only_includes_commits_that_actually_changed_the_pages_content() {
        let tmp = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(tmp.path())
                    .status()
                    .unwrap()
                    .success()
            )
        };
        let commit = |message: &str| {
            run(&[
                "-c",
                "user.email=t@t.com",
                "-c",
                "user.name=t",
                "commit",
                "-q",
                "-m",
                message,
            ]);
        };
        run(&["init", "-q"]);

        std::fs::write(tmp.path().join("Other.md"), "other page").unwrap();
        run(&["add", "."]);
        commit("root: unrelated file");

        std::fs::write(tmp.path().join("Home.md"), "v1").unwrap();
        run(&["add", "."]);
        commit("create home page");

        std::fs::write(tmp.path().join("Other.md"), "other page v2").unwrap();
        run(&["add", "."]);
        commit("unrelated change");

        std::fs::write(tmp.path().join("Home.md"), "v2").unwrap();
        run(&["add", "."]);
        commit("update home page");

        std::fs::remove_file(tmp.path().join("Home.md")).unwrap();
        run(&["add", "."]);
        commit("delete home page");

        let reader = GixRepositoryReader;
        let revisions = reader.list_wiki_page_revisions(tmp.path(), "Home").unwrap();

        let messages: Vec<&str> = revisions.iter().map(|r| r.message.as_str()).collect();
        assert_eq!(
            messages,
            vec!["update home page", "create home page"],
            "newest first, only commits that actually changed the page's content"
        );
    }

    #[test]
    fn list_wiki_page_revisions_on_a_page_that_never_existed_returns_an_empty_list() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_one_commit(tmp.path());

        let reader = GixRepositoryReader;
        assert!(
            reader
                .list_wiki_page_revisions(tmp.path(), "Nonexistent")
                .unwrap()
                .is_empty()
        );
    }

    fn commit_on_current_branch(dir: &Path, message: &str) {
        assert!(
            Command::new("git")
                .args([
                    "-c",
                    "user.email=t@t.com",
                    "-c",
                    "user.name=t",
                    "commit",
                    "--allow-empty",
                    "-q",
                    "-m",
                    message
                ])
                .current_dir(dir)
                .status()
                .unwrap()
                .success()
        );
    }

    fn init_diverged_repo(dir: &Path) {
        let run = |args: &[&str]| {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(dir)
                    .status()
                    .unwrap()
                    .success()
            )
        };
        run(&["init", "-q", "-b", "main"]);
        std::fs::write(dir.join("README.md"), "line one\n").unwrap();
        run(&["add", "."]);
        commit_on_current_branch(dir, "root");
        run(&["checkout", "-q", "-b", "feature"]);
        std::fs::write(dir.join("README.md"), "line one\nline two\n").unwrap();
        std::fs::write(dir.join("new-file.txt"), "brand new\n").unwrap();
        run(&["add", "."]);
        commit_on_current_branch(dir, "feature work");
        run(&["checkout", "-q", "main"]);
        // unrelated change on main after the branches diverge: the diff must stay relative to the merge base
        std::fs::write(dir.join("unrelated.txt"), "main-only change\n").unwrap();
        run(&["add", "."]);
        commit_on_current_branch(dir, "unrelated main work");
    }

    #[test]
    fn diff_branches_shows_a_modified_file_with_added_and_context_lines() {
        let tmp = tempfile::tempdir().unwrap();
        init_diverged_repo(tmp.path());

        let reader = GixRepositoryReader;
        let diffs = reader.diff_branches(tmp.path(), "feature", "main").unwrap();

        let readme = diffs.iter().find(|d| d.path == "README.md").unwrap();
        assert_eq!(readme.change, FileChangeKindRaw::Modified);
        let lines: Vec<(&DiffLineKindRaw, &str)> = readme.hunks[0]
            .lines
            .iter()
            .map(|l| (&l.kind, l.content.trim_end()))
            .collect();
        assert!(lines.contains(&(&DiffLineKindRaw::Context, "line one")));
        assert!(lines.contains(&(&DiffLineKindRaw::Added, "line two")));
        let context_line = readme.hunks[0]
            .lines
            .iter()
            .find(|l| l.kind == DiffLineKindRaw::Context)
            .unwrap();
        assert_eq!(context_line.old_line, Some(1));
        assert_eq!(context_line.new_line, Some(1));
        let added_line = readme.hunks[0]
            .lines
            .iter()
            .find(|l| l.kind == DiffLineKindRaw::Added)
            .unwrap();
        assert_eq!(added_line.old_line, None);
        assert_eq!(added_line.new_line, Some(2));
    }

    #[test]
    fn diff_branches_shows_a_new_file_as_added() {
        let tmp = tempfile::tempdir().unwrap();
        init_diverged_repo(tmp.path());

        let reader = GixRepositoryReader;
        let diffs = reader.diff_branches(tmp.path(), "feature", "main").unwrap();

        let new_file = diffs.iter().find(|d| d.path == "new-file.txt").unwrap();
        assert_eq!(new_file.change, FileChangeKindRaw::Added);
        assert!(
            new_file.hunks[0]
                .lines
                .iter()
                .all(|l| l.kind == DiffLineKindRaw::Added)
        );
        assert_eq!(new_file.hunks[0].lines[0].old_line, None);
        assert_eq!(new_file.hunks[0].lines[0].new_line, Some(1));
    }

    #[test]
    fn diff_branches_never_shows_changes_the_target_branch_made_on_its_own() {
        let tmp = tempfile::tempdir().unwrap();
        init_diverged_repo(tmp.path());

        let reader = GixRepositoryReader;
        let diffs = reader.diff_branches(tmp.path(), "feature", "main").unwrap();

        assert!(
            diffs.iter().all(|d| d.path != "unrelated.txt"),
            "main's own post-divergence change must not appear in a merge-base-relative diff"
        );
    }

    #[test]
    fn diff_branches_marks_a_file_containing_a_nul_byte_as_binary_with_no_hunks() {
        let tmp = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(tmp.path())
                    .status()
                    .unwrap()
                    .success()
            )
        };
        run(&["init", "-q", "-b", "main"]);
        commit_on_current_branch(tmp.path(), "root");
        run(&["checkout", "-q", "-b", "feature"]);
        std::fs::write(tmp.path().join("image.bin"), [0u8, 1, 2, 3, 0, 4]).unwrap();
        run(&["add", "."]);
        commit_on_current_branch(tmp.path(), "add binary file");

        let reader = GixRepositoryReader;
        let diffs = reader.diff_branches(tmp.path(), "feature", "main").unwrap();

        let binary = diffs.iter().find(|d| d.path == "image.bin").unwrap();
        assert_eq!(binary.change, FileChangeKindRaw::Binary);
        assert!(binary.hunks.is_empty());
    }

    #[test]
    fn resolve_revision_resolves_a_branch_name_to_its_tip_commit() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_one_commit(tmp.path());
        let reader = GixRepositoryReader;
        let expected_sha = reader.list_commits(tmp.path(), 1).unwrap()[0].sha.clone();

        let resolved = reader.resolve_revision(tmp.path(), "main").unwrap();

        assert_eq!(resolved.unwrap().to_string(), expected_sha);
    }

    #[test]
    fn resolve_revision_resolves_head() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_one_commit(tmp.path());
        let reader = GixRepositoryReader;
        let expected_sha = reader.list_commits(tmp.path(), 1).unwrap()[0].sha.clone();

        let resolved = reader.resolve_revision(tmp.path(), "HEAD").unwrap();

        assert_eq!(resolved.unwrap().to_string(), expected_sha);
    }

    #[test]
    fn resolve_revision_resolves_a_tag_name() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_one_commit(tmp.path());
        std::process::Command::new("git")
            .args(["tag", "v1.0.0"])
            .current_dir(tmp.path())
            .status()
            .unwrap();
        let reader = GixRepositoryReader;
        let expected_sha = reader.list_commits(tmp.path(), 1).unwrap()[0].sha.clone();

        let resolved = reader.resolve_revision(tmp.path(), "v1.0.0").unwrap();

        assert_eq!(resolved.unwrap().to_string(), expected_sha);
    }

    #[test]
    fn resolve_revision_resolves_a_full_sha() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_one_commit(tmp.path());
        let reader = GixRepositoryReader;
        let sha = reader.list_commits(tmp.path(), 1).unwrap()[0].sha.clone();

        let resolved = reader.resolve_revision(tmp.path(), &sha).unwrap();

        assert_eq!(resolved.unwrap().to_string(), sha);
    }

    #[test]
    fn resolve_revision_returns_none_for_an_unknown_ref() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_one_commit(tmp.path());
        let reader = GixRepositoryReader;

        assert_eq!(
            reader
                .resolve_revision(tmp.path(), "does-not-exist")
                .unwrap(),
            None
        );
    }

    #[test]
    fn resolve_revision_returns_none_on_an_empty_repository() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(
            Command::new("git")
                .args(["init", "-q"])
                .current_dir(tmp.path())
                .status()
                .unwrap()
                .success()
        );
        let reader = GixRepositoryReader;

        assert_eq!(reader.resolve_revision(tmp.path(), "HEAD").unwrap(), None);
    }

    #[test]
    fn list_tree_at_revision_reports_the_last_commit_that_touched_each_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(tmp.path())
                    .status()
                    .unwrap()
                    .success()
            )
        };
        let commit = |message: &str| {
            run(&[
                "-c",
                "user.email=t@t.com",
                "-c",
                "user.name=t",
                "commit",
                "-q",
                "-m",
                message,
            ])
        };
        run(&["init", "-q"]);
        std::fs::write(tmp.path().join("a.txt"), "a v1").unwrap();
        std::fs::write(tmp.path().join("b.txt"), "b v1").unwrap();
        run(&["add", "."]);
        commit("add a and b");
        std::fs::write(tmp.path().join("a.txt"), "a v2").unwrap();
        run(&["add", "."]);
        commit("update a only");

        let reader = GixRepositoryReader;
        let sha = reader.list_commits(tmp.path(), 1).unwrap()[0].sha.clone();
        let entries = reader
            .list_tree_at_revision(tmp.path(), &sha, "")
            .unwrap()
            .unwrap();

        let a = entries.iter().find(|e| e.name == "a.txt").unwrap();
        assert_eq!(a.last_commit.as_ref().unwrap().message, "update a only");
        let b = entries.iter().find(|e| e.name == "b.txt").unwrap();
        assert_eq!(b.last_commit.as_ref().unwrap().message, "add a and b");
    }

    #[test]
    fn list_tree_at_revision_reports_the_last_commit_for_a_nested_entry_by_its_full_path() {
        let tmp = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(tmp.path())
                    .status()
                    .unwrap()
                    .success()
            )
        };
        let commit = |message: &str| {
            run(&[
                "-c",
                "user.email=t@t.com",
                "-c",
                "user.name=t",
                "commit",
                "-q",
                "-m",
                message,
            ])
        };
        run(&["init", "-q"]);
        std::fs::create_dir(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/main.rs"), "v1").unwrap();
        std::fs::write(tmp.path().join("other.txt"), "unrelated").unwrap();
        run(&["add", "."]);
        commit("add main.rs");
        std::fs::write(tmp.path().join("other.txt"), "unrelated v2").unwrap();
        run(&["add", "."]);
        commit("touch other.txt only");

        let reader = GixRepositoryReader;
        let sha = reader.list_commits(tmp.path(), 1).unwrap()[0].sha.clone();
        let src_entries = reader
            .list_tree_at_revision(tmp.path(), &sha, "src")
            .unwrap()
            .unwrap();

        let main_rs = src_entries.iter().find(|e| e.name == "main.rs").unwrap();
        assert_eq!(
            main_rs.last_commit.as_ref().unwrap().message,
            "add main.rs",
            "must resolve against the full path src/main.rs, not just main.rs"
        );
    }

    #[test]
    fn list_tree_at_revision_reports_no_last_commit_when_the_touching_commit_is_outside_the_search_window()
     {
        let tmp = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(tmp.path())
                    .status()
                    .unwrap()
                    .success()
            )
        };
        let commit = |message: &str| {
            run(&[
                "-c",
                "user.email=t@t.com",
                "-c",
                "user.name=t",
                "commit",
                "--allow-empty",
                "-q",
                "-m",
                message,
            ])
        };
        run(&["init", "-q"]);
        std::fs::write(tmp.path().join("old.txt"), "old").unwrap();
        run(&["add", "."]);
        commit("add old.txt");
        for i in 0..5 {
            commit(&format!("unrelated commit {i}"));
        }

        let reader = GixRepositoryReader;
        let sha = reader.list_commits(tmp.path(), 1).unwrap()[0].sha.clone();
        let entries = reader
            .list_tree_at_revision_with_window(tmp.path(), &sha, "", 3)
            .unwrap()
            .unwrap();

        let old = entries.iter().find(|e| e.name == "old.txt").unwrap();
        assert_eq!(
            old.last_commit, None,
            "the touching commit is outside the 3-commit window"
        );
    }

    #[test]
    fn list_tree_names_at_revision_lists_the_same_entries_without_any_last_commit() {
        let tmp = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(tmp.path())
                    .status()
                    .unwrap()
                    .success()
            )
        };
        let commit = |message: &str| {
            run(&[
                "-c",
                "user.email=t@t.com",
                "-c",
                "user.name=t",
                "commit",
                "-q",
                "-m",
                message,
            ])
        };
        run(&["init", "-q"]);
        std::fs::create_dir(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/main.rs"), "v1").unwrap();
        std::fs::write(tmp.path().join("a.txt"), "a").unwrap();
        run(&["add", "."]);
        commit("first");

        let reader = GixRepositoryReader;
        let sha = reader.list_commits(tmp.path(), 1).unwrap()[0].sha.clone();
        let names_only = |entries: Vec<TreeEntryInfo>| {
            entries
                .into_iter()
                .map(|e| TreeEntryInfo {
                    last_commit: None,
                    ..e
                })
                .collect::<Vec<_>>()
        };

        for path in ["", "src"] {
            let full = reader
                .list_tree_at_revision(tmp.path(), &sha, path)
                .unwrap()
                .unwrap();
            assert!(
                full.iter().all(|e| e.last_commit.is_some()),
                "the default listing carries last commits ({path:?})"
            );
            let names = reader
                .list_tree_names_at_revision(tmp.path(), &sha, path)
                .unwrap()
                .unwrap();
            assert!(
                names.iter().all(|e| e.last_commit.is_none()),
                "no last commit is looked up ({path:?})"
            );
            assert_eq!(
                names,
                names_only(full),
                "same names and kinds, in the same order ({path:?})"
            );
        }
        assert_eq!(
            reader
                .list_tree_names_at_revision(tmp.path(), &sha, "missing")
                .unwrap(),
            None
        );
        assert_eq!(
            reader
                .list_tree_names_at_revision(tmp.path(), &sha, "a.txt")
                .unwrap(),
            None
        );
    }

    #[test]
    fn list_contributors_tallies_commits_per_author_newest_activity_does_not_matter_only_count() {
        let tmp = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(tmp.path())
                    .status()
                    .unwrap()
                    .success()
            )
        };
        let commit_as = |name: &str, email: &str, message: &str| {
            assert!(
                Command::new("git")
                    .args([
                        "-c",
                        &format!("user.name={name}"),
                        "-c",
                        &format!("user.email={email}"),
                        "commit",
                        "--allow-empty",
                        "-q",
                        "-m",
                        message
                    ])
                    .current_dir(tmp.path())
                    .status()
                    .unwrap()
                    .success()
            );
        };
        run(&["init", "-q"]);
        commit_as("Alice", "alice@example.com", "one");
        commit_as("Bob", "bob@example.com", "two");
        commit_as("Alice", "alice@example.com", "three");

        let reader = GixRepositoryReader;
        let sha = reader.list_commits(tmp.path(), 1).unwrap()[0].sha.clone();
        let contributors = reader.list_contributors(tmp.path(), &sha).unwrap().unwrap();

        let alice = contributors
            .iter()
            .find(|c| c.email == "alice@example.com")
            .unwrap();
        assert_eq!(alice.commit_count, 2);
        let bob = contributors
            .iter()
            .find(|c| c.email == "bob@example.com")
            .unwrap();
        assert_eq!(bob.commit_count, 1);
        assert_eq!(
            contributors[0].email, "alice@example.com",
            "must be sorted by commit count descending"
        );
    }

    #[test]
    fn list_contributors_truncates_to_the_top_20() {
        let tmp = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(tmp.path())
                    .status()
                    .unwrap()
                    .success()
            )
        };
        run(&["init", "-q"]);
        for i in 0..25 {
            assert!(
                Command::new("git")
                    .args([
                        "-c",
                        &format!("user.name=Author{i}"),
                        "-c",
                        &format!("user.email=author{i}@example.com"),
                        "commit",
                        "--allow-empty",
                        "-q",
                        "-m",
                        "seed"
                    ])
                    .current_dir(tmp.path())
                    .status()
                    .unwrap()
                    .success()
            );
        }

        let reader = GixRepositoryReader;
        let sha = reader.list_commits(tmp.path(), 1).unwrap()[0].sha.clone();
        let contributors = reader.list_contributors(tmp.path(), &sha).unwrap().unwrap();

        assert_eq!(contributors.len(), 20);
    }

    #[test]
    fn list_contributors_returns_none_for_a_nonexistent_revision() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(
            Command::new("git")
                .args(["init", "-q"])
                .current_dir(tmp.path())
                .status()
                .unwrap()
                .success()
        );
        let reader = GixRepositoryReader;

        assert_eq!(
            reader
                .list_contributors(tmp.path(), &"0".repeat(40))
                .unwrap(),
            None
        );
    }

    #[test]
    fn list_contributors_with_window_does_not_count_commits_outside_the_window() {
        let tmp = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(tmp.path())
                    .status()
                    .unwrap()
                    .success()
            )
        };
        run(&["init", "-q"]);
        assert!(
            Command::new("git")
                .args([
                    "-c",
                    "user.name=Old",
                    "-c",
                    "user.email=old@example.com",
                    "commit",
                    "--allow-empty",
                    "-q",
                    "-m",
                    "old"
                ])
                .current_dir(tmp.path())
                .status()
                .unwrap()
                .success()
        );
        for _ in 0..5 {
            assert!(
                Command::new("git")
                    .args([
                        "-c",
                        "user.name=New",
                        "-c",
                        "user.email=new@example.com",
                        "commit",
                        "--allow-empty",
                        "-q",
                        "-m",
                        "new"
                    ])
                    .current_dir(tmp.path())
                    .status()
                    .unwrap()
                    .success()
            );
        }

        let reader = GixRepositoryReader;
        let sha = reader.list_commits(tmp.path(), 1).unwrap()[0].sha.clone();
        let contributors = reader
            .list_contributors_with_window(tmp.path(), &sha, 3)
            .unwrap()
            .unwrap();

        assert!(contributors.iter().all(|c| c.email != "old@example.com"));
        let new_contributor = contributors
            .iter()
            .find(|c| c.email == "new@example.com")
            .unwrap();
        assert_eq!(new_contributor.commit_count, 3);
    }

    #[test]
    fn compute_language_stats_reports_byte_weighted_percentages() {
        let tmp = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(tmp.path())
                    .status()
                    .unwrap()
                    .success()
            )
        };
        run(&["init", "-q"]);
        std::fs::write(tmp.path().join("main.rs"), "a".repeat(75)).unwrap();
        std::fs::write(tmp.path().join("index.html"), "b".repeat(25)).unwrap();
        run(&["add", "."]);
        run(&[
            "-c",
            "user.email=t@t.com",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            "seed",
        ]);

        let reader = GixRepositoryReader;
        let sha = reader.list_commits(tmp.path(), 1).unwrap()[0].sha.clone();
        let stats = reader
            .compute_language_stats(tmp.path(), &sha)
            .unwrap()
            .unwrap();

        let rust = stats.iter().find(|s| s.name == "Rust").unwrap();
        assert_eq!(rust.bytes, 75);
        assert!((rust.percentage - 75.0).abs() < 0.01);
        let html = stats.iter().find(|s| s.name == "HTML").unwrap();
        assert_eq!(html.bytes, 25);
        assert!((html.percentage - 25.0).abs() < 0.01);
    }

    #[test]
    fn compute_language_stats_recurses_into_subdirectories() {
        let tmp = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(tmp.path())
                    .status()
                    .unwrap()
                    .success()
            )
        };
        run(&["init", "-q"]);
        std::fs::create_dir_all(tmp.path().join("src/lib")).unwrap();
        std::fs::write(tmp.path().join("src/lib/deep.rs"), "a".repeat(50)).unwrap();
        run(&["add", "."]);
        run(&[
            "-c",
            "user.email=t@t.com",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            "seed",
        ]);

        let reader = GixRepositoryReader;
        let sha = reader.list_commits(tmp.path(), 1).unwrap()[0].sha.clone();
        let stats = reader
            .compute_language_stats(tmp.path(), &sha)
            .unwrap()
            .unwrap();

        let rust = stats.iter().find(|s| s.name == "Rust").unwrap();
        assert_eq!(
            rust.bytes, 50,
            "must count a file nested two directories deep"
        );
    }

    #[test]
    fn compute_language_stats_groups_everything_past_the_top_6_into_other() {
        let tmp = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(tmp.path())
                    .status()
                    .unwrap()
                    .success()
            )
        };
        run(&["init", "-q"]);
        for (name, content) in [
            ("a.rs", "x".repeat(70)),
            ("b.py", "x".repeat(60)),
            ("c.go", "x".repeat(50)),
            ("d.rb", "x".repeat(40)),
            ("e.java", "x".repeat(30)),
            ("f.c", "x".repeat(20)),
            ("g.sql", "x".repeat(10)),
        ] {
            std::fs::write(tmp.path().join(name), content).unwrap();
        }
        run(&["add", "."]);
        run(&[
            "-c",
            "user.email=t@t.com",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            "seed",
        ]);

        let reader = GixRepositoryReader;
        let sha = reader.list_commits(tmp.path(), 1).unwrap()[0].sha.clone();
        let stats = reader
            .compute_language_stats(tmp.path(), &sha)
            .unwrap()
            .unwrap();

        assert_eq!(stats.len(), 7, "top 6 languages plus one Other entry");
        let other = stats.iter().find(|s| s.name == "Other").unwrap();
        assert_eq!(
            other.bytes, 10,
            "the 7th-largest language (SQL, 10 bytes) must be folded into Other"
        );
    }

    #[test]
    fn compute_language_stats_returns_none_when_the_file_count_exceeds_the_limit() {
        let tmp = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(tmp.path())
                    .status()
                    .unwrap()
                    .success()
            )
        };
        run(&["init", "-q"]);
        for i in 0..5 {
            std::fs::write(tmp.path().join(format!("file{i}.rs")), "x").unwrap();
        }
        run(&["add", "."]);
        run(&[
            "-c",
            "user.email=t@t.com",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            "seed",
        ]);

        let reader = GixRepositoryReader;
        let sha = reader.list_commits(tmp.path(), 1).unwrap()[0].sha.clone();
        let stats = reader
            .compute_language_stats_with_limit(tmp.path(), &sha, 3)
            .unwrap();

        assert_eq!(stats, None);
    }

    #[test]
    fn compute_language_stats_returns_none_for_a_nonexistent_revision() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(
            Command::new("git")
                .args(["init", "-q"])
                .current_dir(tmp.path())
                .status()
                .unwrap()
                .success()
        );
        let reader = GixRepositoryReader;

        assert_eq!(
            reader
                .compute_language_stats(tmp.path(), &"0".repeat(40))
                .unwrap(),
            None
        );
    }
}
