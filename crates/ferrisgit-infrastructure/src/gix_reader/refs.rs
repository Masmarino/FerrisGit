use std::path::Path;

use super::{BranchInfoRaw, GitReadError, GixRepositoryReader, TagInfoRaw, open};

impl GixRepositoryReader {
    pub fn list_branches(&self, disk_path: &Path) -> Result<Vec<BranchInfoRaw>, GitReadError> {
        let repo = open(disk_path)?;
        let default_branch_name = repo
            .head_name()
            .map_err(GitReadError::other)?
            .map(|name| name.shorten().to_string());

        let Ok(references) = repo.references() else {
            return Ok(Vec::new());
        };
        let Ok(local_branches) = references.local_branches() else {
            return Ok(Vec::new());
        };

        let mut branches = Vec::new();
        for reference in local_branches {
            let mut reference = reference.map_err(GitReadError::other)?;
            let name = reference.name().shorten().to_string();
            let tip_sha = reference
                .peel_to_id()
                .map_err(GitReadError::other)?
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
        let repo = open(disk_path)?;
        let Ok(references) = repo.references() else {
            return Ok(Vec::new());
        };
        let Ok(tags) = references.tags() else {
            return Ok(Vec::new());
        };

        let mut result = Vec::new();
        for reference in tags {
            let mut reference = reference.map_err(GitReadError::other)?;
            let name = reference.name().shorten().to_string();
            let target_sha = reference
                .peel_to_id()
                .map_err(GitReadError::other)?
                .to_string();
            result.push(TagInfoRaw { name, target_sha });
        }
        Ok(result)
    }
}
