use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::error::DomainError;

/// The slug is the filename without `.md` at the wiki root: alphanumeric, `-` or `_`, not starting with `-`, 1 to 100
/// chars. Strict because it ends up as an unencoded URL path segment.
pub fn is_valid_wiki_slug(slug: &str) -> bool {
    if slug.is_empty() || slug.len() > 100 {
        return false;
    }
    let mut chars = slug.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if first == '-' || !(first.is_ascii_alphanumeric() || first == '_') {
        return false;
    }
    slug.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Derived from the slug, never stored: `Getting-Started` shows as "Getting Started".
pub fn wiki_page_title_from_slug(slug: &str) -> String {
    slug.replace('-', " ")
}

#[derive(Debug, Clone, PartialEq)]
pub struct WikiPageInfo {
    pub slug: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WikiPageContent {
    pub content: String,
    pub head_sha: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WikiRevision {
    pub commit_sha: String,
    pub author_name: String,
    pub author_email: String,
    pub committed_at: DateTime<Utc>,
    pub message: String,
}

#[async_trait]
pub trait WikiReaderPort: Send + Sync {
    /// `None` when the wiki repo doesn't exist or has no commits yet. Clients send it back as `baseSha`.
    async fn current_head_sha(&self, wiki_disk_path: &str) -> Result<Option<String>, DomainError>;
    /// Files pushed with raw git whose stem isn't a valid slug are left out. Empty if the repo is missing or has no commits.
    async fn list_pages(&self, wiki_disk_path: &str) -> Result<Vec<WikiPageInfo>, DomainError>;
    /// `None` if the page or the wiki doesn't exist at HEAD.
    async fn read_page(
        &self,
        wiki_disk_path: &str,
        slug: &str,
    ) -> Result<Option<WikiPageContent>, DomainError>;
    /// Commits that changed the page's content, most recent first. A commit that deleted it isn't listed.
    async fn list_page_revisions(
        &self,
        wiki_disk_path: &str,
        slug: &str,
    ) -> Result<Vec<WikiRevision>, DomainError>;
    /// `None` if the page didn't exist at that commit or the commit doesn't exist.
    async fn read_page_at_revision(
        &self,
        wiki_disk_path: &str,
        slug: &str,
        commit_sha: &str,
    ) -> Result<Option<String>, DomainError>;
}

#[async_trait]
pub trait WikiWriterPort: Send + Sync {
    /// Does nothing if the repo exists. Otherwise creates a bare repo set up for HTTP push and points `HEAD` at
    /// `refs/heads/main` itself, because the first commit may come from a web save and `init.defaultBranch` can't be trusted.
    async fn ensure_wiki_repo_exists(&self, wiki_disk_path: &str) -> Result<(), DomainError>;

    /// `base_sha` is the commit the caller last saw, `None` only for a wiki without commits. `git update-ref` is a
    /// compare-and-swap against it, so any concurrent change to the wiki, any page or a push, gives `Conflict`. Returns the
    /// new revision.
    #[allow(clippy::too_many_arguments)]
    async fn save_page(
        &self,
        wiki_disk_path: &str,
        slug: &str,
        content: &str,
        base_sha: Option<&str>,
        author_name: &str,
        author_email: &str,
        message: &str,
    ) -> Result<WikiRevision, DomainError>;

    /// Same compare-and-swap as `save_page`, with `base_sha` required. `NotFound` if the page isn't there at `base_sha`.
    #[allow(clippy::too_many_arguments)]
    async fn delete_page(
        &self,
        wiki_disk_path: &str,
        slug: &str,
        base_sha: &str,
        author_name: &str,
        author_email: &str,
        message: &str,
    ) -> Result<(), DomainError>;

    /// Repoints `HEAD` off a dangling symref when that's unambiguous. A raw push of a first branch not named main leaves
    /// `HEAD` on a missing `refs/heads/main`, so the wiki would look empty forever. Does nothing if `HEAD` already resolves.
    async fn heal_dangling_head(&self, wiki_disk_path: &str) -> Result<(), DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_validation_accepts_reasonable_names_and_rejects_unsafe_ones() {
        assert!(is_valid_wiki_slug("Home"));
        assert!(is_valid_wiki_slug("Getting-Started"));
        assert!(is_valid_wiki_slug("_internal"));
        assert!(is_valid_wiki_slug("v1_0"));

        assert!(!is_valid_wiki_slug(""));
        assert!(!is_valid_wiki_slug("-leading-dash"));
        assert!(!is_valid_wiki_slug("has/slash"));
        assert!(!is_valid_wiki_slug("has.dot"));
        assert!(!is_valid_wiki_slug("has space"));
        assert!(!is_valid_wiki_slug(&"a".repeat(101)));
    }

    #[test]
    fn title_derivation_replaces_dashes_with_spaces() {
        assert_eq!(
            wiki_page_title_from_slug("Getting-Started"),
            "Getting Started"
        );
        assert_eq!(wiki_page_title_from_slug("Home"), "Home");
    }
}
