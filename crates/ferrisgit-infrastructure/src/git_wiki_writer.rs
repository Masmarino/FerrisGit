use std::path::{Path, PathBuf};

use crate::error::infra;
use crate::git_cli::{self, is_plausible_commit_sha};
use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::wiki_page::{WikiRevision, WikiWriterPort};

pub struct GitWikiWriter {
    storage_root: PathBuf,
}

impl GitWikiWriter {
    pub fn new(storage_root: PathBuf) -> Self {
        Self { storage_root }
    }
}

const DEFAULT_BRANCH: &str = "main";

async fn run_git(
    repo_path: &Path,
    args: &[&str],
    envs: &[(&str, &str)],
) -> Result<(bool, String, String), DomainError> {
    let output = git_cli::run(repo_path, args, envs, None).await?;
    Ok((
        output.success,
        output.stdout_without_final_newline(),
        output.stderr,
    ))
}

async fn run_git_with_stdin(
    repo_path: &Path,
    args: &[&str],
    stdin_data: &[u8],
) -> Result<(bool, String, String), DomainError> {
    let output = git_cli::run(repo_path, args, &[], Some(stdin_data)).await?;
    Ok((
        output.success,
        output.stdout_without_final_newline(),
        output.stderr,
    ))
}

/// Builds the `git mktree` stdin with `slug`'s entry replaced or added. Entries are sorted by name
/// here instead of relying on mktree to normalize them.
fn build_tree_input(existing_ls_tree_output: &str, slug: &str, new_blob_sha: &str) -> String {
    let file_name = format!("{slug}.md");
    let mut lines: Vec<String> = existing_ls_tree_output
        .lines()
        .filter(|line| line.split('\t').nth(1) != Some(file_name.as_str()))
        .map(str::to_string)
        .collect();
    lines.push(format!("100644 blob {new_blob_sha}\t{file_name}"));
    lines.sort_by(|a, b| a.split('\t').nth(1).cmp(&b.split('\t').nth(1)));
    let mut input = lines.join("\n");
    input.push('\n');
    input
}

#[async_trait]
impl WikiWriterPort for GitWikiWriter {
    async fn ensure_wiki_repo_exists(&self, wiki_disk_path: &str) -> Result<(), DomainError> {
        let repo_path = self.storage_root.join(wiki_disk_path);
        if repo_path.join("HEAD").exists() {
            // cheap check that saves spawning three `git` processes on every page save
            return Ok(());
        }
        tokio::fs::create_dir_all(&repo_path).await.map_err(infra)?;

        let (ok, _, stderr) = run_git(&repo_path, &["init", "--bare"], &[]).await?;
        if !ok {
            tracing::warn!(
                wiki_disk_path,
                stderr,
                "git init --bare failed for wiki repo"
            );
            return Err(DomainError::Infrastructure(
                "failed to initialize wiki repository".to_string(),
            ));
        }
        let (ok, _, stderr) =
            run_git(&repo_path, &["config", "http.receivepack", "true"], &[]).await?;
        if !ok {
            tracing::warn!(
                wiki_disk_path,
                stderr,
                "git config http.receivepack failed for wiki repo"
            );
            return Err(DomainError::Infrastructure(
                "failed to initialize wiki repository".to_string(),
            ));
        }
        let (ok, _, stderr) = run_git(
            &repo_path,
            &[
                "symbolic-ref",
                "HEAD",
                &format!("refs/heads/{DEFAULT_BRANCH}"),
            ],
            &[],
        )
        .await?;
        if !ok {
            tracing::warn!(
                wiki_disk_path,
                stderr,
                "git symbolic-ref HEAD failed for wiki repo"
            );
            return Err(DomainError::Infrastructure(
                "failed to initialize wiki repository".to_string(),
            ));
        }
        Ok(())
    }

    /// Does nothing on a healthy wiki. If the very first write was a raw push to a branch other than
    /// `main`, `HEAD` stays a dangling symref to `refs/heads/main` and `head_id()` fails, so the wiki
    /// looks empty. The fix renames the only real branch to `refs/heads/main` instead of moving `HEAD`,
    /// because `save_page` and `delete_page` always write `refs/heads/main` directly. It only acts when
    /// there is exactly one branch, since with several it's unclear which one should be `main`.
    async fn heal_dangling_head(&self, wiki_disk_path: &str) -> Result<(), DomainError> {
        let repo_path = self.storage_root.join(wiki_disk_path);
        let main_ref = format!("refs/heads/{DEFAULT_BRANCH}");

        let (ok, current_ref, _) = run_git(&repo_path, &["symbolic-ref", "HEAD"], &[]).await?;
        if !ok {
            return Ok(());
        }

        let (resolves, _, _) = run_git(
            &repo_path,
            &["rev-parse", "--verify", "--quiet", current_ref.as_str()],
            &[],
        )
        .await?;
        if resolves {
            return Ok(());
        }

        let (ok, branches_listing, stderr) = run_git(
            &repo_path,
            &["for-each-ref", "refs/heads/", "--format=%(refname)"],
            &[],
        )
        .await?;
        if !ok {
            tracing::warn!(
                wiki_disk_path,
                stderr,
                "git for-each-ref failed while checking for a dangling wiki HEAD"
            );
            return Err(DomainError::Infrastructure(
                "failed to inspect wiki repository".to_string(),
            ));
        }
        let branches: Vec<&str> = branches_listing.lines().filter(|l| !l.is_empty()).collect();
        if branches.len() != 1 || branches[0] == main_ref {
            // Only heal a single branch that isn't already `main`. With zero there is nothing to heal, and
            // with several it's ambiguous.
            return Ok(());
        }
        let only_branch = branches[0];

        let (ok, target_sha, stderr) =
            run_git(&repo_path, &["rev-parse", only_branch], &[]).await?;
        if !ok {
            tracing::warn!(
                wiki_disk_path,
                stderr,
                only_branch,
                "failed to resolve the orphaned wiki branch's commit"
            );
            return Err(DomainError::Infrastructure(
                "failed to inspect wiki repository".to_string(),
            ));
        }

        // Create-only CAS: if two healers race, one wins and the other's `update-ref` fails harmlessly.
        let (ok, _, stderr) =
            run_git(&repo_path, &["update-ref", &main_ref, &target_sha, ""], &[]).await?;
        if !ok {
            tracing::warn!(
                wiki_disk_path,
                stderr,
                only_branch,
                target_sha,
                "failed to create refs/heads/main while healing a dangling wiki HEAD"
            );
            return Err(DomainError::Infrastructure(
                "failed to repair wiki repository".to_string(),
            ));
        }

        let (ok, _, stderr) = run_git(
            &repo_path,
            &["update-ref", "-d", only_branch, &target_sha],
            &[],
        )
        .await?;
        if !ok {
            // A stale extra branch ref is harmless; just log it.
            tracing::warn!(
                wiki_disk_path,
                stderr,
                only_branch,
                "healed a dangling wiki HEAD but failed to delete the now-redundant branch ref"
            );
        }
        Ok(())
    }

    async fn save_page(
        &self,
        wiki_disk_path: &str,
        slug: &str,
        content: &str,
        base_sha: Option<&str>,
        author_name: &str,
        author_email: &str,
        message: &str,
    ) -> Result<WikiRevision, DomainError> {
        if let Some(sha) = base_sha
            && !is_plausible_commit_sha(sha)
        {
            return Err(DomainError::Validation(format!(
                "'{sha}' is not a plausible commit sha"
            )));
        }
        let repo_path = self.storage_root.join(wiki_disk_path);

        let (ok, blob_sha, stderr) = run_git_with_stdin(
            &repo_path,
            &["hash-object", "-w", "--stdin"],
            content.as_bytes(),
        )
        .await?;
        if !ok {
            tracing::warn!(
                wiki_disk_path,
                stderr,
                "git hash-object failed for wiki page save"
            );
            return Err(DomainError::Infrastructure(
                "failed to save wiki page".to_string(),
            ));
        }

        let existing_listing = match base_sha {
            Some(sha) => {
                let (ok, output, stderr) = run_git(&repo_path, &["ls-tree", sha], &[]).await?;
                if !ok {
                    tracing::warn!(
                        wiki_disk_path,
                        base_sha = sha,
                        stderr,
                        "git ls-tree failed for wiki page save"
                    );
                    return Err(DomainError::Conflict("base revision not found".to_string()));
                }
                output
            }
            None => String::new(),
        };
        let tree_input = build_tree_input(&existing_listing, slug, &blob_sha);
        let (ok, tree_sha, stderr) =
            run_git_with_stdin(&repo_path, &["mktree"], tree_input.as_bytes()).await?;
        if !ok {
            tracing::warn!(
                wiki_disk_path,
                stderr,
                "git mktree failed for wiki page save"
            );
            return Err(DomainError::Infrastructure(
                "failed to save wiki page".to_string(),
            ));
        }

        let mut commit_args = vec!["commit-tree", tree_sha.as_str(), "-m", message];
        if let Some(sha) = base_sha {
            commit_args.push("-p");
            commit_args.push(sha);
        }
        let envs = [
            ("GIT_AUTHOR_NAME", author_name),
            ("GIT_AUTHOR_EMAIL", author_email),
            ("GIT_COMMITTER_NAME", author_name),
            ("GIT_COMMITTER_EMAIL", author_email),
        ];
        let (ok, commit_sha, stderr) = run_git(&repo_path, &commit_args, &envs).await?;
        if !ok {
            tracing::warn!(
                wiki_disk_path,
                stderr,
                "git commit-tree failed for wiki page save"
            );
            return Err(DomainError::Infrastructure(
                "failed to save wiki page".to_string(),
            ));
        }

        // The atomic ref update is the conflict check. An empty old-value means the ref must not exist
        // yet. Otherwise its current value must equal `base_sha`.
        let branch_ref = format!("refs/heads/{DEFAULT_BRANCH}");
        let old_value = base_sha.unwrap_or("");
        let (ok, _, stderr) = run_git(
            &repo_path,
            &["update-ref", &branch_ref, &commit_sha, old_value],
            &[],
        )
        .await?;
        if !ok {
            tracing::warn!(
                wiki_disk_path,
                stderr,
                "git update-ref failed for wiki page save (base_sha mismatch)"
            );
            return Err(DomainError::Conflict(
                "the wiki changed since you loaded this page".to_string(),
            ));
        }

        Ok(WikiRevision {
            commit_sha,
            author_name: author_name.to_string(),
            author_email: author_email.to_string(),
            committed_at: chrono::Utc::now(),
            message: message.to_string(),
        })
    }

    async fn delete_page(
        &self,
        wiki_disk_path: &str,
        slug: &str,
        base_sha: &str,
        author_name: &str,
        author_email: &str,
        message: &str,
    ) -> Result<(), DomainError> {
        if !is_plausible_commit_sha(base_sha) {
            return Err(DomainError::Validation(format!(
                "'{base_sha}' is not a plausible commit sha"
            )));
        }
        let repo_path = self.storage_root.join(wiki_disk_path);
        let file_name = format!("{slug}.md");

        let (ok, existing_listing, stderr) =
            run_git(&repo_path, &["ls-tree", base_sha], &[]).await?;
        if !ok {
            tracing::warn!(
                wiki_disk_path,
                base_sha,
                stderr,
                "git ls-tree failed for wiki page delete"
            );
            return Err(DomainError::Conflict("base revision not found".to_string()));
        }
        let mut found = false;
        let tree_lines: Vec<&str> = existing_listing
            .lines()
            .filter(|line| {
                let is_this_page = line.split('\t').nth(1) == Some(file_name.as_str());
                found |= is_this_page;
                !is_this_page
            })
            .collect();
        if !found {
            return Err(DomainError::NotFound(format!("wiki page '{slug}'")));
        }
        // The remaining lines keep their sorted order. An empty tree needs truly empty stdin, because
        // `mktree` rejects a lone newline (a blank line) outside `--batch` mode.
        let mut tree_input = tree_lines.join("\n");
        if !tree_lines.is_empty() {
            tree_input.push('\n');
        }
        let (ok, tree_sha, stderr) =
            run_git_with_stdin(&repo_path, &["mktree"], tree_input.as_bytes()).await?;
        if !ok {
            tracing::warn!(
                wiki_disk_path,
                stderr,
                "git mktree failed for wiki page delete"
            );
            return Err(DomainError::Infrastructure(
                "failed to delete wiki page".to_string(),
            ));
        }

        let envs = [
            ("GIT_AUTHOR_NAME", author_name),
            ("GIT_AUTHOR_EMAIL", author_email),
            ("GIT_COMMITTER_NAME", author_name),
            ("GIT_COMMITTER_EMAIL", author_email),
        ];
        let (ok, commit_sha, stderr) = run_git(
            &repo_path,
            &["commit-tree", &tree_sha, "-p", base_sha, "-m", message],
            &envs,
        )
        .await?;
        if !ok {
            tracing::warn!(
                wiki_disk_path,
                stderr,
                "git commit-tree failed for wiki page delete"
            );
            return Err(DomainError::Infrastructure(
                "failed to delete wiki page".to_string(),
            ));
        }

        let branch_ref = format!("refs/heads/{DEFAULT_BRANCH}");
        let (ok, _, stderr) = run_git(
            &repo_path,
            &["update-ref", &branch_ref, &commit_sha, base_sha],
            &[],
        )
        .await?;
        if !ok {
            tracing::warn!(
                wiki_disk_path,
                stderr,
                "git update-ref failed for wiki page delete (base_sha mismatch)"
            );
            return Err(DomainError::Conflict(
                "the wiki changed since you loaded this page".to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_git::{git, git_stdout, git_stdout_untrimmed, git_succeeds};

    fn temp_storage_root() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    /// A working checkout holding one committed `Home.md`, as a user's clone would before a raw push.
    fn checkout_with_home_page(content: &str) -> tempfile::TempDir {
        let work_dir = tempfile::tempdir().unwrap();
        git(work_dir.path(), &["init", "-q"]);
        std::fs::write(work_dir.path().join("Home.md"), content).unwrap();
        git(work_dir.path(), &["add", "."]);
        git(work_dir.path(), &["commit", "-q", "-m", "seed"]);
        work_dir
    }

    /// Pushes the checkout's `HEAD` straight into the bare wiki repo, bypassing `save_page`.
    fn push_raw(work_dir: &Path, repo_path: &Path, branch: &str) {
        let target = format!("HEAD:refs/heads/{branch}");
        git(
            work_dir,
            &["push", "-q", repo_path.to_str().unwrap(), &target],
        );
    }

    #[tokio::test]
    async fn ensure_wiki_repo_exists_is_idempotent() {
        let root = temp_storage_root();
        let writer = GitWikiWriter::new(root.path().to_path_buf());
        writer
            .ensure_wiki_repo_exists("a/b.wiki.git")
            .await
            .unwrap();
        writer
            .ensure_wiki_repo_exists("a/b.wiki.git")
            .await
            .unwrap(); // must not error the second time
        assert!(root.path().join("a/b.wiki.git/HEAD").exists());
    }

    #[tokio::test]
    async fn saving_the_first_page_creates_a_root_commit_and_a_resolvable_ref() {
        let root = temp_storage_root();
        let writer = GitWikiWriter::new(root.path().to_path_buf());
        writer
            .ensure_wiki_repo_exists("a/b.wiki.git")
            .await
            .unwrap();

        let revision = writer
            .save_page(
                "a/b.wiki.git",
                "Home",
                "# Hello",
                None,
                "Ada",
                "ada@example.com",
                "Create Home",
            )
            .await
            .unwrap();
        assert_eq!(revision.message, "Create Home");

        let repo_path = root.path().join("a/b.wiki.git");
        assert_eq!(
            git_stdout_untrimmed(&repo_path, &["cat-file", "-p", "refs/heads/main:Home.md"]),
            "# Hello"
        );
    }

    #[tokio::test]
    async fn saving_a_second_page_preserves_the_first() {
        let root = temp_storage_root();
        let writer = GitWikiWriter::new(root.path().to_path_buf());
        writer
            .ensure_wiki_repo_exists("a/b.wiki.git")
            .await
            .unwrap();

        let first = writer
            .save_page(
                "a/b.wiki.git",
                "Home",
                "# Hello",
                None,
                "Ada",
                "ada@example.com",
                "Create Home",
            )
            .await
            .unwrap();
        writer
            .save_page(
                "a/b.wiki.git",
                "About",
                "# About",
                Some(&first.commit_sha),
                "Ada",
                "ada@example.com",
                "Create About",
            )
            .await
            .unwrap();

        let repo_path = root.path().join("a/b.wiki.git");
        assert_eq!(
            git_stdout_untrimmed(&repo_path, &["cat-file", "-p", "refs/heads/main:Home.md"]),
            "# Hello"
        );
        assert_eq!(
            git_stdout_untrimmed(&repo_path, &["cat-file", "-p", "refs/heads/main:About.md"]),
            "# About"
        );
    }

    #[tokio::test]
    async fn saving_with_a_stale_base_sha_is_rejected_as_a_conflict() {
        let root = temp_storage_root();
        let writer = GitWikiWriter::new(root.path().to_path_buf());
        writer
            .ensure_wiki_repo_exists("a/b.wiki.git")
            .await
            .unwrap();

        let first = writer
            .save_page(
                "a/b.wiki.git",
                "Home",
                "# v1",
                None,
                "Ada",
                "ada@example.com",
                "v1",
            )
            .await
            .unwrap();
        writer
            .save_page(
                "a/b.wiki.git",
                "Home",
                "# v2",
                Some(&first.commit_sha),
                "Bob",
                "bob@example.com",
                "v2",
            )
            .await
            .unwrap();

        let result = writer
            .save_page(
                "a/b.wiki.git",
                "Home",
                "# v3 (stale)",
                Some(&first.commit_sha),
                "Ada",
                "ada@example.com",
                "v3",
            )
            .await;
        assert!(
            matches!(result, Err(DomainError::Conflict(_))),
            "expected a Conflict, got {result:?}"
        );
    }

    #[tokio::test]
    async fn deleting_a_page_removes_only_that_entry() {
        let root = temp_storage_root();
        let writer = GitWikiWriter::new(root.path().to_path_buf());
        writer
            .ensure_wiki_repo_exists("a/b.wiki.git")
            .await
            .unwrap();

        let first = writer
            .save_page(
                "a/b.wiki.git",
                "Home",
                "# Home",
                None,
                "Ada",
                "ada@example.com",
                "Create Home",
            )
            .await
            .unwrap();
        let second = writer
            .save_page(
                "a/b.wiki.git",
                "About",
                "# About",
                Some(&first.commit_sha),
                "Ada",
                "ada@example.com",
                "Create About",
            )
            .await
            .unwrap();
        writer
            .delete_page(
                "a/b.wiki.git",
                "About",
                &second.commit_sha,
                "Ada",
                "ada@example.com",
                "Remove About",
            )
            .await
            .unwrap();

        let repo_path = root.path().join("a/b.wiki.git");
        assert_eq!(
            git_stdout_untrimmed(&repo_path, &["cat-file", "-p", "refs/heads/main:Home.md"]),
            "# Home"
        );
        assert!(
            !git_succeeds(&repo_path, &["cat-file", "-e", "refs/heads/main:About.md"]),
            "About.md must no longer exist at HEAD"
        );
    }

    /// Regression: deleting a wiki's only page must leave a valid empty tree, not fail `git mktree`.
    #[tokio::test]
    async fn deleting_a_wikis_only_page_leaves_a_valid_empty_tree() {
        let root = temp_storage_root();
        let writer = GitWikiWriter::new(root.path().to_path_buf());
        writer
            .ensure_wiki_repo_exists("a/b.wiki.git")
            .await
            .unwrap();

        let first = writer
            .save_page(
                "a/b.wiki.git",
                "Home",
                "# Home",
                None,
                "Ada",
                "ada@example.com",
                "Create Home",
            )
            .await
            .unwrap();
        writer
            .delete_page(
                "a/b.wiki.git",
                "Home",
                &first.commit_sha,
                "Ada",
                "ada@example.com",
                "Remove Home",
            )
            .await
            .unwrap();

        let repo_path = root.path().join("a/b.wiki.git");
        assert!(
            !git_succeeds(&repo_path, &["cat-file", "-e", "refs/heads/main:Home.md"]),
            "Home.md must no longer exist at HEAD"
        );
    }

    #[tokio::test]
    async fn deleting_a_page_that_does_not_exist_is_not_found() {
        let root = temp_storage_root();
        let writer = GitWikiWriter::new(root.path().to_path_buf());
        writer
            .ensure_wiki_repo_exists("a/b.wiki.git")
            .await
            .unwrap();
        let first = writer
            .save_page(
                "a/b.wiki.git",
                "Home",
                "# Home",
                None,
                "Ada",
                "ada@example.com",
                "Create Home",
            )
            .await
            .unwrap();

        let result = writer
            .delete_page(
                "a/b.wiki.git",
                "DoesNotExist",
                &first.commit_sha,
                "Ada",
                "ada@example.com",
                "x",
            )
            .await;
        assert!(
            matches!(result, Err(DomainError::NotFound(_))),
            "expected NotFound, got {result:?}"
        );
    }

    /// Regression: `base_sha: "--help"` made `git ls-tree` print help and corrupted the tree (500 instead of 400).
    #[tokio::test]
    async fn saving_with_a_base_sha_that_looks_like_a_flag_is_rejected_as_validation_not_a_500() {
        let root = temp_storage_root();
        let writer = GitWikiWriter::new(root.path().to_path_buf());
        writer
            .ensure_wiki_repo_exists("a/b.wiki.git")
            .await
            .unwrap();

        let result = writer
            .save_page(
                "a/b.wiki.git",
                "Home",
                "# Hello",
                Some("--help"),
                "Ada",
                "ada@example.com",
                "msg",
            )
            .await;
        assert!(
            matches!(result, Err(DomainError::Validation(_))),
            "expected Validation, got {result:?}"
        );

        let repo_path = root.path().join("a/b.wiki.git");
        assert!(
            !git_succeeds(&repo_path, &["rev-parse", "--verify", "--quiet", "HEAD"]),
            "no commit must have been created"
        );
    }

    #[tokio::test]
    async fn deleting_with_a_base_sha_that_looks_like_a_flag_is_rejected_as_validation() {
        let root = temp_storage_root();
        let writer = GitWikiWriter::new(root.path().to_path_buf());
        writer
            .ensure_wiki_repo_exists("a/b.wiki.git")
            .await
            .unwrap();
        writer
            .save_page(
                "a/b.wiki.git",
                "Home",
                "# Home",
                None,
                "Ada",
                "ada@example.com",
                "Create Home",
            )
            .await
            .unwrap();

        let result = writer
            .delete_page(
                "a/b.wiki.git",
                "Home",
                "--not-a-sha",
                "Ada",
                "ada@example.com",
                "x",
            )
            .await;
        assert!(
            matches!(result, Err(DomainError::Validation(_))),
            "expected Validation, got {result:?}"
        );
    }

    #[tokio::test]
    async fn a_real_looking_base_sha_still_works_normally() {
        let root = temp_storage_root();
        let writer = GitWikiWriter::new(root.path().to_path_buf());
        writer
            .ensure_wiki_repo_exists("a/b.wiki.git")
            .await
            .unwrap();

        let first = writer
            .save_page(
                "a/b.wiki.git",
                "Home",
                "# v1",
                None,
                "Ada",
                "ada@example.com",
                "v1",
            )
            .await
            .unwrap();
        assert!(is_plausible_commit_sha(&first.commit_sha));
        let second = writer
            .save_page(
                "a/b.wiki.git",
                "Home",
                "# v2",
                Some(&first.commit_sha),
                "Ada",
                "ada@example.com",
                "v2",
            )
            .await;
        assert!(
            second.is_ok(),
            "a real commit sha must still be accepted: {second:?}"
        );
    }

    /// Regression: a raw push to a non-`main` first branch left `HEAD` dangling. Healing must rename that branch to `main`.
    #[tokio::test]
    async fn heal_dangling_head_renames_the_orphaned_branch_onto_main_after_a_raw_push() {
        let root = temp_storage_root();
        let writer = GitWikiWriter::new(root.path().to_path_buf());
        writer
            .ensure_wiki_repo_exists("a/b.wiki.git")
            .await
            .unwrap();
        let repo_path = root.path().join("a/b.wiki.git");

        // A real git client pushes straight to `master`, bypassing `save_page`.
        let work_dir = checkout_with_home_page("# Pushed from git");
        push_raw(work_dir.path(), &repo_path, "master");

        assert!(
            !git_succeeds(&repo_path, &["rev-parse", "--verify", "--quiet", "HEAD"]),
            "HEAD must be dangling before healing, or this test isn't exercising the bug"
        );

        writer.heal_dangling_head("a/b.wiki.git").await.unwrap();

        assert_eq!(
            git_stdout(&repo_path, &["symbolic-ref", "HEAD"]),
            "refs/heads/main",
            "HEAD must stay on main, not follow the pushed branch"
        );
        assert_eq!(
            git_stdout_untrimmed(&repo_path, &["cat-file", "-p", "refs/heads/main:Home.md"]),
            "# Pushed from git"
        );
        assert!(
            !git_succeeds(
                &repo_path,
                &["rev-parse", "--verify", "--quiet", "refs/heads/master"]
            ),
            "the old branch ref must be cleaned up after the rename"
        );
    }

    /// Regression: healing must leave the wiki writable from the web editor, not just readable.
    #[tokio::test]
    async fn a_wiki_healed_after_a_non_main_first_push_is_still_writable_from_the_web_editor() {
        let root = temp_storage_root();
        let writer = GitWikiWriter::new(root.path().to_path_buf());
        writer
            .ensure_wiki_repo_exists("a/b.wiki.git")
            .await
            .unwrap();
        let repo_path = root.path().join("a/b.wiki.git");

        let work_dir = checkout_with_home_page("# Pushed from git");
        push_raw(work_dir.path(), &repo_path, "master");

        writer.heal_dangling_head("a/b.wiki.git").await.unwrap();

        let (_, head_sha, _) = run_git(&repo_path, &["rev-parse", "HEAD"], &[])
            .await
            .unwrap();
        let revision = writer
            .save_page(
                "a/b.wiki.git",
                "About",
                "# About",
                Some(&head_sha),
                "Ada",
                "ada@example.com",
                "Create About",
            )
            .await
            .unwrap();
        assert!(!revision.commit_sha.is_empty());

        assert_eq!(
            git_stdout_untrimmed(&repo_path, &["cat-file", "-p", "refs/heads/main:About.md"]),
            "# About"
        );
        assert_eq!(
            git_stdout_untrimmed(&repo_path, &["cat-file", "-p", "refs/heads/main:Home.md"]),
            "# Pushed from git",
            "the raw-pushed page must survive the web save"
        );
    }

    #[tokio::test]
    async fn heal_dangling_head_is_a_no_op_when_head_already_resolves() {
        let root = temp_storage_root();
        let writer = GitWikiWriter::new(root.path().to_path_buf());
        writer
            .ensure_wiki_repo_exists("a/b.wiki.git")
            .await
            .unwrap();
        writer
            .save_page(
                "a/b.wiki.git",
                "Home",
                "# Hello",
                None,
                "Ada",
                "ada@example.com",
                "Create Home",
            )
            .await
            .unwrap();

        writer.heal_dangling_head("a/b.wiki.git").await.unwrap();

        let repo_path = root.path().join("a/b.wiki.git");
        assert_eq!(
            git_stdout(&repo_path, &["symbolic-ref", "HEAD"]),
            "refs/heads/main",
            "healing an already-healthy HEAD must not move it"
        );
    }

    #[tokio::test]
    async fn heal_dangling_head_leaves_head_alone_when_the_wiki_genuinely_has_no_commits_yet() {
        let root = temp_storage_root();
        let writer = GitWikiWriter::new(root.path().to_path_buf());
        writer
            .ensure_wiki_repo_exists("a/b.wiki.git")
            .await
            .unwrap();

        writer.heal_dangling_head("a/b.wiki.git").await.unwrap();

        let repo_path = root.path().join("a/b.wiki.git");
        assert_eq!(
            git_stdout(&repo_path, &["symbolic-ref", "HEAD"]),
            "refs/heads/main",
            "zero branches means nothing to heal yet"
        );
    }

    #[tokio::test]
    async fn heal_dangling_head_leaves_head_alone_when_more_than_one_branch_exists() {
        let root = temp_storage_root();
        let writer = GitWikiWriter::new(root.path().to_path_buf());
        writer
            .ensure_wiki_repo_exists("a/b.wiki.git")
            .await
            .unwrap();
        let repo_path = root.path().join("a/b.wiki.git");

        let work_dir = checkout_with_home_page("# Pushed");
        for branch in ["master", "other"] {
            push_raw(work_dir.path(), &repo_path, branch);
        }

        writer.heal_dangling_head("a/b.wiki.git").await.unwrap();

        assert_eq!(
            git_stdout(&repo_path, &["symbolic-ref", "HEAD"]),
            "refs/heads/main",
            "an ambiguous HEAD must be left alone, not guessed at"
        );
    }
}
