use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::{
    DiffLineKindRaw, DiffLineRaw, FileChangeKindRaw, FileDiffRaw, GitReadError,
    GixRepositoryReader, HunkRaw, open,
};

impl GixRepositoryReader {
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
        let merge_base_id = repo
            .merge_base(source_id, target_id)
            .map_err(GitReadError::other)?;

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
