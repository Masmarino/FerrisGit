use std::path::Path;

use super::{
    GitReadError, GixRepositoryReader, WikiPageContentRaw, WikiPageInfoRaw, WikiRevisionRaw,
    commit_info, id_at_path, newest_first_walk, open, walked_commit,
};

impl GixRepositoryReader {
    /// Callers have already checked that a `wikis` row exists, so a failing `gix::open` is a real error (disk
    /// and Postgres disagree). Only a repo with zero commits gives `Ok(None)`.
    pub fn wiki_head_sha(&self, disk_path: &Path) -> Result<Option<String>, GitReadError> {
        let repo = open(disk_path)?;
        Ok(repo.head_id().ok().map(|id| id.to_string()))
    }

    pub fn list_wiki_pages(&self, disk_path: &Path) -> Result<Vec<WikiPageInfoRaw>, GitReadError> {
        let repo = open(disk_path)?;
        let Ok(head_id) = repo.head_id() else {
            return Ok(Vec::new());
        };
        let tree = head_id
            .object()
            .map_err(GitReadError::other)?
            .into_commit()
            .tree()
            .map_err(GitReadError::other)?;

        let mut pages = Vec::new();
        for entry in tree.iter() {
            let entry = entry.map_err(GitReadError::other)?;
            // The namespace is flat: skip any subdirectory a raw git push might have created.
            if entry.mode().is_tree() {
                continue;
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
        let repo = open(disk_path)?;
        let Ok(head_id) = repo.head_id() else {
            return Ok(None);
        };
        let tree = head_id
            .object()
            .map_err(GitReadError::other)?
            .into_commit()
            .tree()
            .map_err(GitReadError::other)?;

        let Some(entry) = tree
            .lookup_entry_by_path(format!("{slug}.md"))
            .map_err(GitReadError::other)?
        else {
            return Ok(None);
        };
        if entry.mode().is_tree() {
            return Ok(None);
        }
        let blob = entry.object().map_err(GitReadError::other)?;
        Ok(Some(WikiPageContentRaw {
            content: String::from_utf8_lossy(&blob.data).to_string(),
            head_sha: head_id.to_string(),
        }))
    }

    /// Commits that added or modified `{slug}.md`, newest first. One that only removed the page is left out.
    pub fn list_wiki_page_revisions(
        &self,
        disk_path: &Path,
        slug: &str,
    ) -> Result<Vec<WikiRevisionRaw>, GitReadError> {
        let repo = open(disk_path)?;
        let Ok(head_id) = repo.head_id() else {
            return Ok(Vec::new());
        };
        let file_name = format!("{slug}.md");

        let mut revisions = Vec::new();
        for info in newest_first_walk(&repo, head_id.detach())? {
            let commit = walked_commit(info)?;
            let Some(current_blob) =
                id_at_path(&commit.tree().map_err(GitReadError::other)?, &file_name)?
            else {
                continue; // the page doesn't exist at this commit at all
            };

            let parent_blob = match commit.parent_ids().next() {
                Some(parent_id) => {
                    let parent_tree = repo
                        .find_object(parent_id)
                        .map_err(GitReadError::other)?
                        .try_into_commit()
                        .map_err(GitReadError::other)?
                        .tree()
                        .map_err(GitReadError::other)?;
                    id_at_path(&parent_tree, &file_name)?
                }
                // A root commit has nothing to compare against, so it counts as a change.
                None => None,
            };

            if Some(current_blob) != parent_blob {
                let info = commit_info(&commit)?;
                revisions.push(WikiRevisionRaw {
                    commit_sha: info.sha,
                    author_name: info.author_name,
                    author_email: info.author_email,
                    committed_at: info.committed_at,
                    message: info.message,
                });
            }
        }
        Ok(revisions)
    }
}
