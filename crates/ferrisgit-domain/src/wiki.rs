use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::DomainError;

#[derive(Debug, Clone, PartialEq)]
pub struct Wiki {
    pub id: Uuid,
    pub repository_id: Uuid,
    pub disk_path: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewWiki {
    pub repository_id: Uuid,
    pub disk_path: String,
}

/// Always derived from the parent repository's disk_path, never recomputed from owner/group, so it cannot drift.
/// Shared by `SaveWikiPageUseCase` and the git-http lazy-create-on-push.
pub fn wiki_disk_path_for(repository_disk_path: &str) -> String {
    format!(
        "{}.wiki.git",
        repository_disk_path
            .strip_suffix(".git")
            .unwrap_or(repository_disk_path)
    )
}

#[async_trait]
pub trait WikiStorePort: Send + Sync {
    /// Plain lookup, never creates: read paths must have no write side effect (a repository with no wiki is a valid
    /// state).
    async fn find_by_repository_id(&self, repository_id: Uuid)
    -> Result<Option<Wiki>, DomainError>;

    /// Idempotent upsert: returns the existing row or inserts one. The row's `disk_path` never changes, so
    /// `new_wiki.disk_path` is only used the first time. Two concurrent callers get the same row, and no error.
    async fn find_or_create(&self, new_wiki: NewWiki) -> Result<Wiki, DomainError>;
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct FakeWikis(Mutex<Vec<Wiki>>);

    #[async_trait]
    impl WikiStorePort for FakeWikis {
        async fn find_by_repository_id(
            &self,
            repository_id: Uuid,
        ) -> Result<Option<Wiki>, DomainError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .find(|w| w.repository_id == repository_id)
                .cloned())
        }

        async fn find_or_create(&self, new_wiki: NewWiki) -> Result<Wiki, DomainError> {
            let mut wikis = self.0.lock().unwrap();
            if let Some(existing) = wikis
                .iter()
                .find(|w| w.repository_id == new_wiki.repository_id)
            {
                return Ok(existing.clone());
            }
            let wiki = Wiki {
                id: Uuid::new_v4(),
                repository_id: new_wiki.repository_id,
                disk_path: new_wiki.disk_path,
                created_at: Utc::now(),
            };
            wikis.push(wiki.clone());
            Ok(wiki)
        }
    }

    #[tokio::test]
    async fn find_or_create_is_idempotent_through_a_trait_object() {
        let store: std::sync::Arc<dyn WikiStorePort> = std::sync::Arc::new(FakeWikis::default());
        let repository_id = Uuid::new_v4();
        let first = store
            .find_or_create(NewWiki {
                repository_id,
                disk_path: "a/b.wiki.git".to_string(),
            })
            .await
            .unwrap();
        let second = store
            .find_or_create(NewWiki {
                repository_id,
                disk_path: "ignored.wiki.git".to_string(),
            })
            .await
            .unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(
            second.disk_path, "a/b.wiki.git",
            "the disk_path from the FIRST call wins, not a later one"
        );
        assert_eq!(
            store
                .find_by_repository_id(repository_id)
                .await
                .unwrap()
                .unwrap()
                .id,
            first.id
        );
    }

    #[test]
    fn wiki_disk_path_is_derived_from_the_parent_repositorys_own_path() {
        assert_eq!(
            wiki_disk_path_for("alice/hello.git"),
            "alice/hello.wiki.git"
        );
        assert_eq!(
            wiki_disk_path_for("groups/acme/backend.git"),
            "groups/acme/backend.wiki.git"
        );
    }
}
