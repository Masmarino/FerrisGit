use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::error::DomainError;

/// Slug = filename (without `.md`) at the wiki repo root. Alphanumeric, `-`, `_`, not starting with `-`, 1-100 chars.
/// Kept strict because the slug ends up as an unencoded URL path segment.
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

/// Derived from the slug, never stored: `Getting-Started` displays as "Getting Started".
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
    /// `None` when the wiki's bare repo doesn't exist or has no commits yet (`SaveWikiPageUseCase` creates the repo
    /// before its first save, which may fail). Clients round-trip it as `baseSha`.
    async fn current_head_sha(&self, wiki_disk_path: &str) -> Result<Option<String>, DomainError>;
    /// Files whose stem fails `is_valid_wiki_slug` (pushed via raw git) are silently excluded. Empty if the repo
    /// doesn't exist or has no commits.
    async fn list_pages(&self, wiki_disk_path: &str) -> Result<Vec<WikiPageInfo>, DomainError>;
    /// `None` if the page (or the wiki) doesn't exist at HEAD.
    async fn read_page(
        &self,
        wiki_disk_path: &str,
        slug: &str,
    ) -> Result<Option<WikiPageContent>, DomainError>;
    /// Commits that changed the page's content, most recent first; a commit that deleted the page is not included.
    async fn list_page_revisions(
        &self,
        wiki_disk_path: &str,
        slug: &str,
    ) -> Result<Vec<WikiRevision>, DomainError>;
    /// `None` if the page didn't exist at that commit, or the commit doesn't exist.
    async fn read_page_at_revision(
        &self,
        wiki_disk_path: &str,
        slug: &str,
        commit_sha: &str,
    ) -> Result<Option<String>, DomainError>;
}

#[async_trait]
pub trait WikiWriterPort: Send + Sync {
    /// Idempotent, and a cheap no-op when the repo exists. Otherwise creates a bare repo configured for HTTP push and
    /// points `HEAD` at `refs/heads/main` itself, since the first commit may come from a web save and
    /// `init.defaultBranch` is unreliable.
    async fn ensure_wiki_repo_exists(&self, wiki_disk_path: &str) -> Result<(), DomainError>;

    /// `base_sha` is the commit the caller last observed (`None` only for a wiki without commits). `git update-ref`
    /// is a compare-and-swap against it: any concurrent change to the wiki (any page, or a push) yields
    /// `DomainError::Conflict`. Returns the new revision.
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

    /// Same compare-and-swap as `save_page`, `base_sha` required. `NotFound` if the page doesn't exist at `base_sha`,
    /// `Conflict` on mismatch.
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

    /// Repoints `HEAD` off a dangling symref when unambiguous. A raw push of a differently named first branch leaves
    /// `HEAD` pointing at the nonexistent `refs/heads/main`, so the wiki would look permanently empty.
    /// Idempotent and cheap: a `HEAD` that already resolves is a no-op.
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
