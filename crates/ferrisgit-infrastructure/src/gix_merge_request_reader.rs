use std::path::PathBuf;

use async_trait::async_trait;
use ferrisgit_domain::branch::{BranchInfo, BranchReaderPort};
use ferrisgit_domain::diff::{
    DiffLine, DiffLineKind, DiffReaderPort, FileChangeKind, FileDiff, Hunk,
};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::tag::{TagInfo, TagReaderPort};
use ferrisgit_domain::wiki_page::{
    WikiPageContent, WikiPageInfo, WikiReaderPort, WikiRevision, is_valid_wiki_slug,
    wiki_page_title_from_slug,
};

use crate::gix_reader::{DiffLineKindRaw, FileChangeKindRaw, GixRepositoryReader};

/// Reads merge-request data from a repository's git refs and objects. Also implements `DiffReaderPort`.
pub struct GixMergeRequestReader {
    storage_root: PathBuf,
}

impl GixMergeRequestReader {
    pub fn new(storage_root: PathBuf) -> Self {
        Self { storage_root }
    }
}

#[async_trait]
impl BranchReaderPort for GixMergeRequestReader {
    async fn list_branches(
        &self,
        repository_disk_path: &str,
    ) -> Result<Vec<BranchInfo>, DomainError> {
        let full_path = self.storage_root.join(repository_disk_path);
        let raw =
            tokio::task::spawn_blocking(move || GixRepositoryReader.list_branches(&full_path))
                .await
                .map_err(|e| DomainError::Infrastructure(e.to_string()))?
                .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(raw
            .into_iter()
            .map(|b| BranchInfo {
                name: b.name,
                tip_sha: b.tip_sha,
                is_default: b.is_default,
            })
            .collect())
    }
}

#[async_trait]
impl TagReaderPort for GixMergeRequestReader {
    async fn list_tags(&self, repository_disk_path: &str) -> Result<Vec<TagInfo>, DomainError> {
        let full_path = self.storage_root.join(repository_disk_path);
        let raw = tokio::task::spawn_blocking(move || GixRepositoryReader.list_tags(&full_path))
            .await
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(raw
            .into_iter()
            .map(|t| TagInfo {
                name: t.name,
                target_sha: t.target_sha,
            })
            .collect())
    }
}

fn map_change_kind(raw: FileChangeKindRaw) -> FileChangeKind {
    match raw {
        FileChangeKindRaw::Added => FileChangeKind::Added,
        FileChangeKindRaw::Modified => FileChangeKind::Modified,
        FileChangeKindRaw::Deleted => FileChangeKind::Deleted,
        FileChangeKindRaw::Binary => FileChangeKind::Binary,
    }
}

fn map_line_kind(raw: DiffLineKindRaw) -> DiffLineKind {
    match raw {
        DiffLineKindRaw::Context => DiffLineKind::Context,
        DiffLineKindRaw::Added => DiffLineKind::Added,
        DiffLineKindRaw::Removed => DiffLineKind::Removed,
    }
}

#[async_trait]
impl DiffReaderPort for GixMergeRequestReader {
    async fn diff_branches(
        &self,
        repository_disk_path: &str,
        source_branch: &str,
        target_branch: &str,
    ) -> Result<Vec<FileDiff>, DomainError> {
        let full_path = self.storage_root.join(repository_disk_path);
        let source_branch = source_branch.to_string();
        let target_branch = target_branch.to_string();
        let raw = tokio::task::spawn_blocking(move || {
            GixRepositoryReader.diff_branches(&full_path, &source_branch, &target_branch)
        })
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(raw
            .into_iter()
            .map(|d| FileDiff {
                path: d.path,
                change: map_change_kind(d.change),
                hunks: d
                    .hunks
                    .into_iter()
                    .map(|h| Hunk {
                        lines: h
                            .lines
                            .into_iter()
                            .map(|l| DiffLine {
                                kind: map_line_kind(l.kind),
                                content: l.content,
                                old_line: l.old_line,
                                new_line: l.new_line,
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect())
    }
}

#[async_trait]
impl WikiReaderPort for GixMergeRequestReader {
    async fn current_head_sha(&self, wiki_disk_path: &str) -> Result<Option<String>, DomainError> {
        let full_path = self.storage_root.join(wiki_disk_path);
        tokio::task::spawn_blocking(move || GixRepositoryReader.wiki_head_sha(&full_path))
            .await
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?
            .map_err(|e| DomainError::Infrastructure(e.to_string()))
    }

    async fn list_pages(&self, wiki_disk_path: &str) -> Result<Vec<WikiPageInfo>, DomainError> {
        let full_path = self.storage_root.join(wiki_disk_path);
        let raw =
            tokio::task::spawn_blocking(move || GixRepositoryReader.list_wiki_pages(&full_path))
                .await
                .map_err(|e| DomainError::Infrastructure(e.to_string()))?
                .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(raw
            .into_iter()
            .filter(|p| is_valid_wiki_slug(&p.slug))
            .map(|p| WikiPageInfo {
                title: wiki_page_title_from_slug(&p.slug),
                slug: p.slug,
            })
            .collect())
    }

    async fn read_page(
        &self,
        wiki_disk_path: &str,
        slug: &str,
    ) -> Result<Option<WikiPageContent>, DomainError> {
        let full_path = self.storage_root.join(wiki_disk_path);
        let slug = slug.to_string();
        let raw = tokio::task::spawn_blocking(move || {
            GixRepositoryReader.read_wiki_page(&full_path, &slug)
        })
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(raw.map(|r| WikiPageContent {
            content: r.content,
            head_sha: r.head_sha,
        }))
    }

    async fn list_page_revisions(
        &self,
        wiki_disk_path: &str,
        slug: &str,
    ) -> Result<Vec<WikiRevision>, DomainError> {
        let full_path = self.storage_root.join(wiki_disk_path);
        let slug = slug.to_string();
        let raw = tokio::task::spawn_blocking(move || {
            GixRepositoryReader.list_wiki_page_revisions(&full_path, &slug)
        })
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(raw
            .into_iter()
            .map(|r| WikiRevision {
                commit_sha: r.commit_sha,
                author_name: r.author_name,
                author_email: r.author_email,
                committed_at: r.committed_at,
                message: r.message,
            })
            .collect())
    }

    async fn read_page_at_revision(
        &self,
        wiki_disk_path: &str,
        slug: &str,
        commit_sha: &str,
    ) -> Result<Option<String>, DomainError> {
        let full_path = self.storage_root.join(wiki_disk_path);
        let commit_sha = commit_sha.to_string();
        let file_name = format!("{slug}.md");
        let raw = tokio::task::spawn_blocking(move || {
            GixRepositoryReader.read_file_at_revision(&full_path, &commit_sha, &file_name)
        })
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(raw.map(|bytes| String::from_utf8_lossy(&bytes).to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn init_repo_with_two_branches(dir: &std::path::Path) {
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
    }

    #[tokio::test]
    async fn list_branches_delegates_to_the_gix_reader_and_maps_the_result() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo_with_two_branches(tmp.path());
        let disk_path = tmp.path().file_name().unwrap().to_str().unwrap();
        let adapter = GixMergeRequestReader::new(tmp.path().parent().unwrap().to_path_buf());

        let branches = adapter.list_branches(disk_path).await.unwrap();

        assert_eq!(branches.len(), 2);
        assert!(branches.iter().any(|b| b.name == "main" && b.is_default));
        assert!(
            branches
                .iter()
                .any(|b| b.name == "feature" && !b.is_default)
        );
    }

    fn init_diverged_repo(dir: &std::path::Path) {
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
        let commit = |dir: &std::path::Path, message: &str| {
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
            )
        };
        run(&["init", "-q", "-b", "main"]);
        commit(dir, "root");
        run(&["checkout", "-q", "-b", "feature"]);
        std::fs::write(dir.join("README.md"), "hello\n").unwrap();
        run(&["add", "."]);
        commit(dir, "feature work");
    }

    #[tokio::test]
    async fn diff_branches_delegates_to_the_gix_reader_and_maps_the_result() {
        let tmp = tempfile::tempdir().unwrap();
        init_diverged_repo(tmp.path());
        let disk_path = tmp.path().file_name().unwrap().to_str().unwrap();
        let adapter = GixMergeRequestReader::new(tmp.path().parent().unwrap().to_path_buf());

        let diffs = adapter
            .diff_branches(disk_path, "feature", "main")
            .await
            .unwrap();

        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].path, "README.md");
        assert_eq!(
            diffs[0].change,
            ferrisgit_domain::diff::FileChangeKind::Added
        );
    }
}
