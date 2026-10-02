use std::path::Path;

use super::{
    CommitInfo, GitReadError, GixRepositoryReader, TreeEntryInfo, commit_by_sha, commit_info,
    id_at_path, newest_first_walk, open, walked_commit,
};

/// Commits walked per entry to find the one that last touched it. Past this `last_commit` is None, on
/// purpose: there is no full-history fallback.
const LAST_COMMIT_SEARCH_WINDOW: usize = 200;

impl GixRepositoryReader {
    /// `path` empty means the repo root, `revision` is a commit SHA already resolved by the caller. `Ok(None)` for
    /// an unresolvable revision, a missing path, or a path that is a file.
    pub fn list_tree_at_revision(
        &self,
        disk_path: &Path,
        revision: &str,
        path: &str,
    ) -> Result<Option<Vec<TreeEntryInfo>>, GitReadError> {
        self.list_tree_at_revision_with_window(disk_path, revision, path, LAST_COMMIT_SEARCH_WINDOW)
    }

    /// Same entries and order, but every `last_commit` is None (no history walk).
    pub fn list_tree_names_at_revision(
        &self,
        disk_path: &Path,
        revision: &str,
        path: &str,
    ) -> Result<Option<Vec<TreeEntryInfo>>, GitReadError> {
        self.list_tree_at_revision_with_window(disk_path, revision, path, 0)
    }

    /// With an explicit search window (0 skips the lookup), so tests can use a small one.
    pub fn list_tree_at_revision_with_window(
        &self,
        disk_path: &Path,
        revision: &str,
        path: &str,
        last_commit_window: usize,
    ) -> Result<Option<Vec<TreeEntryInfo>>, GitReadError> {
        let repo = open(disk_path)?;
        let Some(commit) = commit_by_sha(&repo, revision) else {
            return Ok(None);
        };
        let root_tree = commit.tree().map_err(GitReadError::other)?;

        let tree = if path.is_empty() {
            root_tree
        } else {
            let Some(entry) = root_tree
                .lookup_entry_by_path(path)
                .map_err(GitReadError::other)?
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
            let entry = entry.map_err(GitReadError::other)?;
            let name = entry.filename().to_string();
            let entry_path = if path.is_empty() {
                name.clone()
            } else {
                format!("{path}/{name}")
            };
            let last_commit = if last_commit_window == 0 {
                None
            } else {
                find_last_commit_touching_path(&repo, commit.id, &entry_path, last_commit_window)?
            };
            entries.push(TreeEntryInfo {
                name,
                is_dir: entry.mode().is_tree(),
                last_commit,
            });
        }
        Ok(Some(entries))
    }

    /// `Ok(None)` for a missing file or a revision that doesn't exist or isn't a commit, so a malformed sha
    /// doesn't turn into a 500.
    pub fn read_file_at_revision(
        &self,
        disk_path: &Path,
        revision: &str,
        path: &str,
    ) -> Result<Option<Vec<u8>>, GitReadError> {
        let repo = open(disk_path)?;
        let Some(blob_id) = file_blob_id(&repo, revision, path)? else {
            return Ok(None);
        };
        let blob = repo.find_object(blob_id).map_err(GitReadError::other)?;
        Ok(Some(blob.data.clone()))
    }

    /// Reads only the object header, so oversized blobs can be rejected before loading them. `Ok(None)` in the
    /// same cases as `read_file_at_revision`.
    pub fn blob_size_at_revision(
        &self,
        disk_path: &Path,
        revision: &str,
        path: &str,
    ) -> Result<Option<u64>, GitReadError> {
        let repo = open(disk_path)?;
        let Some(blob_id) = file_blob_id(&repo, revision, path)? else {
            return Ok(None);
        };
        let header = repo.find_header(blob_id).map_err(GitReadError::other)?;
        Ok(Some(header.size()))
    }
}

/// `None` when the commit or the path is missing, or the path is a directory.
fn file_blob_id(
    repo: &gix::Repository,
    revision: &str,
    path: &str,
) -> Result<Option<gix::ObjectId>, GitReadError> {
    let Some(commit) = commit_by_sha(repo, revision) else {
        return Ok(None);
    };
    let tree = commit.tree().map_err(GitReadError::other)?;
    let Some(entry) = tree
        .lookup_entry_by_path(path)
        .map_err(GitReadError::other)?
    else {
        return Ok(None);
    };
    if entry.mode().is_tree() {
        return Ok(None);
    }
    Ok(Some(entry.object_id()))
}

/// Newest commit among the `window` most recent whose tree at `path` differs from its first parent's
/// (a root commit always differs).
fn find_last_commit_touching_path(
    repo: &gix::Repository,
    start_id: gix::ObjectId,
    path: &str,
    window: usize,
) -> Result<Option<CommitInfo>, GitReadError> {
    for info in newest_first_walk(repo, start_id)?.take(window) {
        let commit = walked_commit(info)?;
        let current = id_at_path(&commit.tree().map_err(GitReadError::other)?, path)?;

        let parent = match commit.parent_ids().next() {
            Some(parent_id) => {
                let Ok(parent_object) = repo.find_object(parent_id) else {
                    continue;
                };
                let Ok(parent_commit) = parent_object.try_into_commit() else {
                    continue;
                };
                id_at_path(&parent_commit.tree().map_err(GitReadError::other)?, path)?
            }
            None => None,
        };

        if current != parent {
            return commit_info(&commit).map(Some);
        }
    }
    Ok(None)
}
