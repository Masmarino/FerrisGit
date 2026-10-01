use async_trait::async_trait;
use uuid::Uuid;

use crate::error::DomainError;

/// A star is a preference marker with no access implications (unlike `RepositoryCollaboratorStorePort`).
/// `add`/`remove` are idempotent.
#[async_trait]
pub trait RepositoryStarStorePort: Send + Sync {
    async fn add(&self, repository_id: Uuid, user_id: Uuid) -> Result<(), DomainError>;
    async fn remove(&self, repository_id: Uuid, user_id: Uuid) -> Result<(), DomainError>;
    async fn count_for_repository(&self, repository_id: Uuid) -> Result<i64, DomainError>;
    async fn is_starred(&self, repository_id: Uuid, user_id: Uuid) -> Result<bool, DomainError>;

    /// Backs the `starred=true` filter with one query instead of an `is_starred` call per repository.
    async fn list_starred_for_user(&self, user_id: Uuid) -> Result<Vec<Uuid>, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::Mutex;

    struct FakeStars(Mutex<Vec<(Uuid, Uuid)>>);

    #[async_trait]
    impl RepositoryStarStorePort for FakeStars {
        async fn add(&self, repository_id: Uuid, user_id: Uuid) -> Result<(), DomainError> {
            let mut rows = self.0.lock().unwrap();
            if !rows
                .iter()
                .any(|(r, u)| *r == repository_id && *u == user_id)
            {
                rows.push((repository_id, user_id));
            }
            Ok(())
        }
        async fn remove(&self, repository_id: Uuid, user_id: Uuid) -> Result<(), DomainError> {
            self.0
                .lock()
                .unwrap()
                .retain(|(r, u)| !(*r == repository_id && *u == user_id));
            Ok(())
        }
        async fn count_for_repository(&self, repository_id: Uuid) -> Result<i64, DomainError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .filter(|(r, _)| *r == repository_id)
                .count() as i64)
        }
        async fn is_starred(
            &self,
            repository_id: Uuid,
            user_id: Uuid,
        ) -> Result<bool, DomainError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .any(|(r, u)| *r == repository_id && *u == user_id))
        }
        async fn list_starred_for_user(&self, user_id: Uuid) -> Result<Vec<Uuid>, DomainError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .filter(|(_, u)| *u == user_id)
                .map(|(r, _)| *r)
                .collect())
        }
    }

    #[tokio::test]
    async fn adding_then_removing_a_star_is_reflected_in_count_and_is_starred() {
        let store: Arc<dyn RepositoryStarStorePort> = Arc::new(FakeStars(Mutex::new(vec![])));
        let repo_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        assert_eq!(store.count_for_repository(repo_id).await.unwrap(), 0);
        assert!(!store.is_starred(repo_id, user_id).await.unwrap());

        store.add(repo_id, user_id).await.unwrap();
        assert_eq!(store.count_for_repository(repo_id).await.unwrap(), 1);
        assert!(store.is_starred(repo_id, user_id).await.unwrap());

        store.remove(repo_id, user_id).await.unwrap();
        assert_eq!(store.count_for_repository(repo_id).await.unwrap(), 0);
        assert!(!store.is_starred(repo_id, user_id).await.unwrap());
    }

    #[tokio::test]
    async fn adding_a_star_twice_is_not_an_error_and_counts_once() {
        let store: Arc<dyn RepositoryStarStorePort> = Arc::new(FakeStars(Mutex::new(vec![])));
        let repo_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        store.add(repo_id, user_id).await.unwrap();
        store.add(repo_id, user_id).await.unwrap();

        assert_eq!(store.count_for_repository(repo_id).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn removing_a_star_that_was_never_added_is_not_an_error() {
        let store: Arc<dyn RepositoryStarStorePort> = Arc::new(FakeStars(Mutex::new(vec![])));
        store.remove(Uuid::new_v4(), Uuid::new_v4()).await.unwrap();
    }

    #[tokio::test]
    async fn list_starred_for_user_returns_only_that_users_stars() {
        let store: Arc<dyn RepositoryStarStorePort> = Arc::new(FakeStars(Mutex::new(vec![])));
        let repo_a = Uuid::new_v4();
        let repo_b = Uuid::new_v4();
        let user = Uuid::new_v4();
        let other_user = Uuid::new_v4();

        store.add(repo_a, user).await.unwrap();
        store.add(repo_b, other_user).await.unwrap();

        let starred = store.list_starred_for_user(user).await.unwrap();
        assert_eq!(starred, vec![repo_a]);
    }
}
