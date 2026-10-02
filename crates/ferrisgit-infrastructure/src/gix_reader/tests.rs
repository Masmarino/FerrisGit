use super::*;
use crate::test_git::{git, git_command, rev_parse, run_checked};

fn init_repo_with_one_commit(dir: &Path) {
    let run = |args: &[&str]| git(dir, args);
    run(&["init", "-q", "-b", "main"]);
    run(&["commit", "--allow-empty", "-q", "-m", "first commit"]);
    std::fs::write(dir.join("README.md"), "# hello").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "add readme"]);
}

/// A merge whose first parent is older than its second: BreadthFirst sorting would return the older parent
/// first, while ByCommitTime(NewestFirst) must return the newer feature commit.
fn init_repo_with_merge_commit(dir: &Path) {
    let run = |args: &[&str]| git(dir, args);
    let commit = |message: &str, date: &str| {
        let mut command = git_command(dir);
        command
            .args(["commit", "--allow-empty", "-q", "-m", message])
            .env("GIT_AUTHOR_DATE", date)
            .env("GIT_COMMITTER_DATE", date);
        run_checked(command);
    };

    run(&["init", "-q", "-b", "main"]);
    commit("root", "2024-01-01T00:00:00+00:00");
    run(&["checkout", "-q", "-b", "feature"]);
    commit("feature commit", "2024-01-01T02:00:00+00:00");
    run(&["checkout", "-q", "main"]);
    commit("main commit", "2024-01-01T01:00:00+00:00");
    run(&["merge", "-q", "--no-ff", "feature", "-m", "merge feature"]);
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
    git(tmp.path(), &["init", "-q"]);

    let reader = GixRepositoryReader;
    assert!(reader.list_commits(tmp.path(), 10).unwrap().is_empty());
}

/// `init_repo_with_one_commit` plus a `feature` branch with one more commit and a `v1` tag on main's first commit.
fn init_repo_with_branch_and_tag(dir: &Path) {
    init_repo_with_one_commit(dir);
    git(dir, &["tag", "v1", "HEAD~1"]);
    git(dir, &["checkout", "-q", "-b", "feature"]);
    git(
        dir,
        &["commit", "--allow-empty", "-q", "-m", "feature work"],
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
    git(tmp.path(), &["tag", "-a", "v2", "-m", "release", "feature"]);
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
    git(tmp.path(), &["tag", "-a", "v2", "-m", "release", "feature"]);
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
    let run = |args: &[&str]| git(tmp.path(), args);
    run(&["init", "-q"]);
    std::fs::create_dir_all(tmp.path().join("src/lib")).unwrap();
    std::fs::write(tmp.path().join("src/lib/foo.rs"), "fn foo() {}").unwrap();
    std::fs::write(tmp.path().join("src/main.rs"), "fn main() {}").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "seed"]);
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
    let run = |args: &[&str]| git(tmp.path(), args);
    run(&["init", "-q"]);
    std::fs::create_dir(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/main.rs"), "fn main() {}").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "add src"]);
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
    let run = |args: &[&str]| git(tmp.path(), args);
    run(&["init", "-q"]);
    std::fs::create_dir(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/main.rs"), "fn main() {}").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "add src"]);
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
    let run = |args: &[&str]| git(tmp.path(), args);
    run(&["init", "-q", "-b", "main"]);
    run(&["commit", "--allow-empty", "-q", "-m", "root"]);
    run(&["checkout", "-q", "-b", "feature"]);
    run(&["commit", "--allow-empty", "-q", "-m", "feature work"]);
    // is_default reads HEAD live, so switch back to main after `checkout -b feature`.
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
    git(tmp.path(), &["init", "-q"]);

    let reader = GixRepositoryReader;
    assert!(reader.list_branches(tmp.path()).unwrap().is_empty());
}

#[test]
fn list_tags_returns_every_tag_with_its_target_commit() {
    let tmp = tempfile::tempdir().unwrap();
    init_repo_with_one_commit(tmp.path());
    git(tmp.path(), &["tag", "v1.0.0"]);
    git(tmp.path(), &["tag", "v2.0.0-rc1"]);

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
    git(tmp.path(), &["init", "-q"]);

    let reader = GixRepositoryReader;
    assert!(reader.list_tags(tmp.path()).unwrap().is_empty());
}

#[test]
fn list_wiki_pages_returns_every_markdown_file_at_the_root_ignoring_invalid_names_and_subdirectories()
 {
    let tmp = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| git(tmp.path(), args);
    run(&["init", "-q"]);
    std::fs::write(tmp.path().join("Home.md"), "# Home").unwrap();
    std::fs::write(tmp.path().join("Getting-Started.md"), "# Start").unwrap();
    std::fs::write(tmp.path().join("not-a-page.txt"), "ignored").unwrap();
    std::fs::create_dir(tmp.path().join("subdir")).unwrap();
    std::fs::write(tmp.path().join("subdir/Nested.md"), "ignored").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "seed"]);

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
    git(tmp.path(), &["init", "-q"]);

    let reader = GixRepositoryReader;
    assert!(reader.list_wiki_pages(tmp.path()).unwrap().is_empty());
}

#[test]
fn wiki_head_sha_is_none_for_a_bare_repo_with_no_commits_yet() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q", "--bare"]);

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
    let run = |args: &[&str]| git(tmp.path(), args);
    run(&["init", "-q"]);
    std::fs::write(tmp.path().join("Home.md"), "# Hello wiki").unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "add home page"]);

    let reader = GixRepositoryReader;
    let head_sha = reader.wiki_head_sha(tmp.path()).unwrap().unwrap();
    let page = reader.read_wiki_page(tmp.path(), "Home").unwrap().unwrap();

    assert_eq!(page.content, "# Hello wiki");
    assert_eq!(page.head_sha, head_sha);
}

#[test]
fn list_wiki_page_revisions_only_includes_commits_that_actually_changed_the_pages_content() {
    let tmp = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| git(tmp.path(), args);
    let commit = |message: &str| {
        run(&["commit", "-q", "-m", message]);
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
    git(dir, &["commit", "--allow-empty", "-q", "-m", message]);
}

fn init_diverged_repo(dir: &Path) {
    let run = |args: &[&str]| git(dir, args);
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
    // An unrelated change on main after the branches diverge: the diff must stay relative to the merge base.
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
    let run = |args: &[&str]| git(tmp.path(), args);
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
    git(tmp.path(), &["tag", "v1.0.0"]);
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
    git(tmp.path(), &["init", "-q"]);
    let reader = GixRepositoryReader;

    assert_eq!(reader.resolve_revision(tmp.path(), "HEAD").unwrap(), None);
}

#[test]
fn list_tree_at_revision_reports_the_last_commit_that_touched_each_entry() {
    let tmp = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| git(tmp.path(), args);
    let commit = |message: &str| run(&["commit", "-q", "-m", message]);
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
    let run = |args: &[&str]| git(tmp.path(), args);
    let commit = |message: &str| run(&["commit", "-q", "-m", message]);
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
    let run = |args: &[&str]| git(tmp.path(), args);
    let commit = |message: &str| run(&["commit", "--allow-empty", "-q", "-m", message]);
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
    let run = |args: &[&str]| git(tmp.path(), args);
    let commit = |message: &str| run(&["commit", "-q", "-m", message]);
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
    let run = |args: &[&str]| git(tmp.path(), args);
    let commit_as = |name: &str, email: &str, message: &str| {
        git(
            tmp.path(),
            &[
                "-c",
                &format!("user.name={name}"),
                "-c",
                &format!("user.email={email}"),
                "commit",
                "--allow-empty",
                "-q",
                "-m",
                message,
            ],
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
    let run = |args: &[&str]| git(tmp.path(), args);
    run(&["init", "-q"]);
    for i in 0..25 {
        git(
            tmp.path(),
            &[
                "-c",
                &format!("user.name=Author{i}"),
                "-c",
                &format!("user.email=author{i}@example.com"),
                "commit",
                "--allow-empty",
                "-q",
                "-m",
                "seed",
            ],
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
    git(tmp.path(), &["init", "-q"]);
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
    let run = |args: &[&str]| git(tmp.path(), args);
    run(&["init", "-q"]);
    git(
        tmp.path(),
        &[
            "-c",
            "user.name=Old",
            "-c",
            "user.email=old@example.com",
            "commit",
            "--allow-empty",
            "-q",
            "-m",
            "old",
        ],
    );
    for _ in 0..5 {
        git(
            tmp.path(),
            &[
                "-c",
                "user.name=New",
                "-c",
                "user.email=new@example.com",
                "commit",
                "--allow-empty",
                "-q",
                "-m",
                "new",
            ],
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
    let run = |args: &[&str]| git(tmp.path(), args);
    run(&["init", "-q"]);
    std::fs::write(tmp.path().join("main.rs"), "a".repeat(75)).unwrap();
    std::fs::write(tmp.path().join("index.html"), "b".repeat(25)).unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "seed"]);

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
    let run = |args: &[&str]| git(tmp.path(), args);
    run(&["init", "-q"]);
    std::fs::create_dir_all(tmp.path().join("src/lib")).unwrap();
    std::fs::write(tmp.path().join("src/lib/deep.rs"), "a".repeat(50)).unwrap();
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "seed"]);

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
    let run = |args: &[&str]| git(tmp.path(), args);
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
    run(&["commit", "-q", "-m", "seed"]);

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
    let run = |args: &[&str]| git(tmp.path(), args);
    run(&["init", "-q"]);
    for i in 0..5 {
        std::fs::write(tmp.path().join(format!("file{i}.rs")), "x").unwrap();
    }
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "seed"]);

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
    git(tmp.path(), &["init", "-q"]);
    let reader = GixRepositoryReader;

    assert_eq!(
        reader
            .compute_language_stats(tmp.path(), &"0".repeat(40))
            .unwrap(),
        None
    );
}
