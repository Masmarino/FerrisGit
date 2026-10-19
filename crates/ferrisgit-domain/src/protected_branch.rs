use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::error::DomainError;
use crate::repository_collaborator::CollaboratorRole;

/// A rule protecting every branch whose name matches `pattern`: nobody pushes to such a branch, changes arrive through a
/// merge request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProtectedBranch {
    pub id: Uuid,
    pub repository_id: Uuid,
    pub pattern: String,
    pub created_at: DateTime<Utc>,
}

pub struct NewProtectedBranch {
    pub repository_id: Uuid,
    pub pattern: String,
}

const MAX_PATTERN_LENGTH: usize = 255;

/// Trims `pattern` and checks it can name branches: a branch name, where `*` stands for any run of characters, `/`
/// included (`release/*`, `*-stable`, `*`). Returns the pattern to store.
pub fn normalize_pattern(pattern: &str) -> Result<String, DomainError> {
    let pattern = pattern.trim();
    let invalid = |reason: &str| {
        Err(DomainError::Validation(format!(
            "invalid branch pattern: {reason}"
        )))
    };
    if pattern.is_empty() {
        return invalid("it is empty");
    }
    if pattern.chars().count() > MAX_PATTERN_LENGTH {
        return invalid("it is longer than 255 characters");
    }
    if pattern.starts_with("refs/") {
        return invalid("write the branch name without refs/heads/");
    }
    if pattern.starts_with('-') || pattern.starts_with('/') || pattern.ends_with('/') {
        return invalid("it cannot start with - or /, nor end with /");
    }
    if pattern.contains("..") || pattern.contains("//") || pattern.ends_with(".lock") {
        return invalid("it cannot contain .. or //, nor end with .lock");
    }
    if pattern
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || "~^:?[\\".contains(c))
    {
        return invalid("it cannot contain spaces nor any of ~ ^ : ? [ \\");
    }
    Ok(pattern.to_string())
}

/// Whether `branch` (a short name such as `main`) is covered by `pattern`. `*` matches any run of characters, `/`
/// included; everything else matches itself.
pub fn pattern_matches(pattern: &str, branch: &str) -> bool {
    let mut parts = pattern.split('*');
    let first = parts.next().unwrap_or_default();
    let Some(mut rest) = branch.strip_prefix(first) else {
        return false;
    };
    let parts: Vec<&str> = parts.collect();
    let Some((last, middle)) = parts.split_last() else {
        // No `*` at all: the whole name has to be the pattern.
        return rest.is_empty();
    };
    for part in middle {
        match rest.find(part) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    rest.ends_with(last)
}

/// Whether any of `protections` covers `branch`.
pub fn is_protected(protections: &[ProtectedBranch], branch: &str) -> bool {
    protections
        .iter()
        .any(|protection| pattern_matches(&protection.pattern, branch))
}

/// One `<old> <new> <ref>` command of a push.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefUpdate {
    pub old_sha: String,
    pub new_sha: String,
    pub ref_name: String,
}

fn is_null_sha(sha: &str) -> bool {
    !sha.is_empty() && sha.bytes().all(|b| b == b'0')
}

impl RefUpdate {
    pub fn is_creation(&self) -> bool {
        is_null_sha(&self.old_sha)
    }

    pub fn is_deletion(&self) -> bool {
        is_null_sha(&self.new_sha)
    }

    /// The branch this command moves, `None` for a tag or any other ref.
    pub fn branch(&self) -> Option<&str> {
        self.ref_name.strip_prefix("refs/heads/")
    }
}

/// A command of a push that is not allowed, with the reason the person pushing gets to read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefRefusal {
    pub ref_name: String,
    pub reason: String,
}

/// The commands of a push that touch a protected branch and are refused. A protected branch is never pushed to, nor
/// force-pushed, nor deleted: its changes arrive through a merge request. The one exception is creating a branch that
/// does not exist yet, which a Maintainer may do, so that the first push to a new repository (or the creation of a
/// release branch) can happen with its protection already in place. `pusher_role` is the role of whoever pushes.
pub fn refused_ref_updates(
    protections: &[ProtectedBranch],
    updates: &[RefUpdate],
    pusher_role: Option<CollaboratorRole>,
) -> Vec<RefRefusal> {
    updates
        .iter()
        .filter_map(|update| {
            let branch = update.branch()?;
            if !is_protected(protections, branch) {
                return None;
            }
            let reason = if update.is_deletion() {
                format!("branch '{branch}' is protected and cannot be deleted")
            } else if update.is_creation() {
                if pusher_role >= Some(CollaboratorRole::Maintainer) {
                    return None;
                }
                format!("branch '{branch}' is protected: only a Maintainer can create it")
            } else {
                format!(
                    "branch '{branch}' is protected: push to another branch and open a merge request"
                )
            };
            Some(RefRefusal {
                ref_name: update.ref_name.clone(),
                reason,
            })
        })
        .collect()
}

#[async_trait]
pub trait ProtectedBranchStorePort: Send + Sync {
    /// Oldest first.
    async fn list_for_repository(
        &self,
        repository_id: Uuid,
    ) -> Result<Vec<ProtectedBranch>, DomainError>;
    /// `Conflict` if the repository already protects that pattern.
    async fn create(&self, new: NewProtectedBranch) -> Result<ProtectedBranch, DomainError>;
    /// `NotFound` unless the rule belongs to that repository.
    async fn delete(&self, repository_id: Uuid, id: Uuid) -> Result<(), DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZERO: &str = "0000000000000000000000000000000000000000";
    const A: &str = "1111111111111111111111111111111111111111";
    const B: &str = "2222222222222222222222222222222222222222";

    fn protection(pattern: &str) -> ProtectedBranch {
        ProtectedBranch {
            id: Uuid::new_v4(),
            repository_id: Uuid::new_v4(),
            pattern: pattern.to_string(),
            created_at: Utc::now(),
        }
    }

    fn update(old: &str, new: &str, ref_name: &str) -> RefUpdate {
        RefUpdate {
            old_sha: old.to_string(),
            new_sha: new.to_string(),
            ref_name: ref_name.to_string(),
        }
    }

    #[test]
    fn a_pattern_without_a_wildcard_matches_only_that_branch() {
        assert!(pattern_matches("main", "main"));
        assert!(!pattern_matches("main", "main2"));
        assert!(!pattern_matches("main", "xmain"));
        assert!(!pattern_matches("main", "feature/main"));
    }

    #[test]
    fn a_wildcard_matches_any_run_of_characters_slashes_included() {
        assert!(pattern_matches("release/*", "release/1.0"));
        assert!(pattern_matches("release/*", "release/1.0/hotfix"));
        assert!(pattern_matches("release/*", "release/"));
        assert!(!pattern_matches("release/*", "releases/1.0"));
        assert!(pattern_matches("*-stable", "2-stable"));
        assert!(!pattern_matches("*-stable", "2-stable-x"));
        assert!(pattern_matches("*", "anything/at/all"));
        assert!(pattern_matches("a*b*c", "a-b-c"));
        assert!(pattern_matches("a*b*c", "abc"));
        assert!(!pattern_matches("a*b*c", "acb"));
        assert!(!pattern_matches("ab*ba", "aba"));
    }

    #[test]
    fn a_pattern_is_trimmed_and_checked() {
        assert_eq!(normalize_pattern("  main ").unwrap(), "main");
        assert_eq!(normalize_pattern("release/*").unwrap(), "release/*");
        for bad in [
            "",
            "   ",
            "refs/heads/main",
            "-x",
            "/x",
            "x/",
            "a..b",
            "a//b",
            "x.lock",
            "a b",
            "a~1",
            "a^",
            "a:b",
            "a?",
            "a[b",
            "a\\b",
        ] {
            assert!(
                matches!(normalize_pattern(bad), Err(DomainError::Validation(_))),
                "{bad:?}"
            );
        }
        assert!(normalize_pattern(&"a".repeat(256)).is_err());
        assert!(normalize_pattern(&"a".repeat(255)).is_ok());
    }

    #[test]
    fn nothing_is_refused_when_no_branch_is_protected() {
        let updates = [update(A, B, "refs/heads/main")];
        assert!(refused_ref_updates(&[], &updates, Some(CollaboratorRole::Contributor)).is_empty());
    }

    #[test]
    fn an_update_of_a_protected_branch_is_refused_whoever_pushes() {
        let protections = [protection("main")];
        for role in [
            None,
            Some(CollaboratorRole::Contributor),
            Some(CollaboratorRole::Maintainer),
        ] {
            let refused =
                refused_ref_updates(&protections, &[update(A, B, "refs/heads/main")], role);
            assert_eq!(refused.len(), 1, "{role:?}");
            assert_eq!(refused[0].ref_name, "refs/heads/main");
            assert!(refused[0].reason.contains("merge request"));
        }
    }

    #[test]
    fn deleting_a_protected_branch_is_refused_even_to_a_maintainer() {
        let refused = refused_ref_updates(
            &[protection("main")],
            &[update(A, ZERO, "refs/heads/main")],
            Some(CollaboratorRole::Maintainer),
        );
        assert_eq!(refused.len(), 1);
        assert!(refused[0].reason.contains("cannot be deleted"));
    }

    #[test]
    fn only_a_maintainer_creates_a_protected_branch() {
        let protections = [protection("release/*")];
        let create = [update(ZERO, A, "refs/heads/release/1.0")];
        assert!(
            refused_ref_updates(&protections, &create, Some(CollaboratorRole::Maintainer))
                .is_empty()
        );
        let refused =
            refused_ref_updates(&protections, &create, Some(CollaboratorRole::Contributor));
        assert_eq!(refused.len(), 1);
        assert!(refused[0].reason.contains("only a Maintainer"));
    }

    #[test]
    fn only_the_commands_on_protected_branches_are_refused_and_tags_are_left_alone() {
        let protections = [protection("main")];
        let updates = [
            update(A, B, "refs/heads/feature"),
            update(A, B, "refs/heads/main"),
            update(A, B, "refs/tags/main"),
        ];
        let refused =
            refused_ref_updates(&protections, &updates, Some(CollaboratorRole::Contributor));
        assert_eq!(
            refused
                .iter()
                .map(|r| r.ref_name.as_str())
                .collect::<Vec<_>>(),
            vec!["refs/heads/main"]
        );
    }

    #[test]
    fn a_sha256_null_id_is_recognised_too() {
        let zero = "0".repeat(64);
        assert!(update(&zero, A, "refs/heads/x").is_creation());
        assert!(update(A, &zero, "refs/heads/x").is_deletion());
        assert!(!update(A, B, "refs/heads/x").is_creation());
    }
}
