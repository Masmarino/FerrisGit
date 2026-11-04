use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::{
    DiffLineKindRaw, DiffLineRaw, FileChangeKindRaw, FileDiffRaw, GitReadError,
    GixRepositoryReader, HunkRaw, commit_by_sha, open,
};

impl GixRepositoryReader {
    /// The files that differ between two commits, sorted. `Ok(None)` when a revision doesn't exist or isn't a commit.
    /// Folders whose tree is the same on both sides aren't entered, so that a small change in a large repository stays
    /// cheap: a push waits for this.
    pub fn changed_paths(
        &self,
        disk_path: &Path,
        base: &str,
        head: &str,
    ) -> Result<Option<Vec<String>>, GitReadError> {
        let repo = open(disk_path)?;
        let tree_of = |sha: &str| {
            commit_by_sha(&repo, sha)
                .map(|commit| commit.tree_id().map(|id| id.detach()))
                .transpose()
                .map_err(GitReadError::other)
        };
        let (Some(base_tree), Some(head_tree)) = (tree_of(base)?, tree_of(head)?) else {
            return Ok(None);
        };
        let mut paths = Vec::new();
        collect_changed_paths(&repo, Some(base_tree), Some(head_tree), "", &mut paths)?;
        paths.sort();
        Ok(Some(paths))
    }

    /// Diffs the merge base of the two branches against the source tip. Both trees are walked into BTreeMap
    /// snapshots because gix's tree-diff feature isn't enabled; line diffs come from `similar`.
    pub fn diff_branches(
        &self,
        disk_path: &Path,
        source_branch: &str,
        target_branch: &str,
    ) -> Result<Vec<FileDiffRaw>, GitReadError> {
        let repo = open(disk_path)?;
        let branch_tip = |branch: &str| {
            repo.find_reference(&format!("refs/heads/{branch}"))
                .map_err(GitReadError::other)?
                .peel_to_id()
                .map_err(GitReadError::other)
        };
        let source_id = branch_tip(source_branch)?;
        let target_id = branch_tip(target_branch)?;
        // Branches with unrelated histories have no merge base to diff against.
        let merge_base_id = repo
            .merge_base(source_id, target_id)
            .map_err(GitReadError::other)?
            .ok_or_else(|| {
                GitReadError::other(format!(
                    "{source_branch} and {target_branch} have no common ancestor"
                ))
            })?;

        let base_tree = commit_tree(merge_base_id)?;
        let source_tree = commit_tree(source_id)?;

        let mut base_blobs = BTreeMap::new();
        collect_blobs(&base_tree, "", &mut base_blobs)?;
        let mut source_blobs = BTreeMap::new();
        collect_blobs(&source_tree, "", &mut source_blobs)?;

        let paths: BTreeSet<String> = base_blobs
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

type TreeEntry = (gix::ObjectId, gix::objs::tree::EntryMode);

/// What a tree holds, by name. `None` is a folder that isn't there.
fn tree_entries(
    repo: &gix::Repository,
    tree: Option<gix::ObjectId>,
) -> Result<BTreeMap<String, TreeEntry>, GitReadError> {
    let mut entries = BTreeMap::new();
    let Some(tree) = tree else {
        return Ok(entries);
    };
    let tree = repo
        .find_object(tree)
        .map_err(GitReadError::other)?
        .into_tree();
    for entry in tree.iter() {
        let entry = entry.map_err(GitReadError::other)?;
        entries.insert(
            entry.filename().to_string(),
            (entry.oid().to_owned(), entry.mode()),
        );
    }
    Ok(entries)
}

/// A folder missing on one side counts as empty, so that all it holds on the other side is listed.
fn collect_changed_paths(
    repo: &gix::Repository,
    base: Option<gix::ObjectId>,
    head: Option<gix::ObjectId>,
    prefix: &str,
    out: &mut Vec<String>,
) -> Result<(), GitReadError> {
    let base = tree_entries(repo, base)?;
    let head = tree_entries(repo, head)?;
    let names: BTreeSet<&String> = base.keys().chain(head.keys()).collect();
    for name in names {
        let old = base.get(name);
        let new = head.get(name);
        if old == new {
            continue;
        }
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let folder_of =
            |side: Option<&TreeEntry>| side.filter(|(_, mode)| mode.is_tree()).map(|(id, _)| *id);
        let (old_folder, new_folder) = (folder_of(old), folder_of(new));
        if old_folder.is_some() || new_folder.is_some() {
            collect_changed_paths(repo, old_folder, new_folder, &path, out)?;
        }
        // A file on either side has changed, even when a folder took its place.
        let is_file = |side: Option<&TreeEntry>| side.is_some_and(|(_, mode)| !mode.is_tree());
        if is_file(old) || is_file(new) {
            out.push(path);
        }
    }
    Ok(())
}

fn commit_tree(id: gix::Id<'_>) -> Result<gix::Tree<'_>, GitReadError> {
    id.object()
        .map_err(GitReadError::other)?
        .into_commit()
        .tree()
        .map_err(GitReadError::other)
}

fn collect_blobs(
    tree: &gix::Tree,
    prefix: &str,
    out: &mut BTreeMap<String, gix::ObjectId>,
) -> Result<(), GitReadError> {
    for entry in tree.iter() {
        let entry = entry.map_err(GitReadError::other)?;
        let full_path = if prefix.is_empty() {
            entry.filename().to_string()
        } else {
            format!("{prefix}/{}", entry.filename())
        };
        if entry.mode().is_tree() {
            let subtree = entry.object().map_err(GitReadError::other)?.into_tree();
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
        .map_err(GitReadError::other)?
        .data
        .clone())
}
