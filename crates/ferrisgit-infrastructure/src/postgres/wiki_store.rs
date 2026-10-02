use crate::error::infra;
use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::wiki::{NewWiki, Wiki, WikiStorePort};
use sqlx::PgPool;
use uuid::Uuid;

pub struct PostgresWikiStore {
    pool: PgPool,
}

impl PostgresWikiStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl WikiStorePort for PostgresWikiStore {
    async fn find_by_repository_id(
        &self,
        repository_id: Uuid,
    ) -> Result<Option<Wiki>, DomainError> {
        let row = sqlx::query!(
            "SELECT id, repository_id, disk_path, created_at FROM wikis WHERE repository_id = $1",
            repository_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(infra)?;
        Ok(row.map(|r| Wiki {
            id: r.id,
            repository_id: r.repository_id,
            disk_path: r.disk_path,
            created_at: r.created_at,
        }))
    }

    async fn find_or_create(&self, new_wiki: NewWiki) -> Result<Wiki, DomainError> {
        let row = sqlx::query!(
            "INSERT INTO wikis (repository_id, disk_path) VALUES ($1, $2) \
             ON CONFLICT (repository_id) DO UPDATE SET repository_id = wikis.repository_id \
             RETURNING id, repository_id, disk_path, created_at",
            new_wiki.repository_id,
            new_wiki.disk_path
        )
        .fetch_one(&self.pool)
        .await
        .map_err(infra)?;
        Ok(Wiki {
            id: row.id,
            repository_id: row.repository_id,
            disk_path: row.disk_path,
            created_at: row.created_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::postgres::test_support::{seed_repository, seed_user};

    #[sqlx::test(migrations = "../../migrations")]
    async fn find_by_repository_id_returns_none_when_no_wiki_exists(pool: PgPool) {
        let store = PostgresWikiStore::new(pool.clone());
        let owner_id = seed_user(&pool, "owner").await;
        let repository_id = seed_repository(&pool, owner_id, "hello").await;
        assert!(
            store
                .find_by_repository_id(repository_id)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn find_or_create_creates_a_new_row(pool: PgPool) {
        let store = PostgresWikiStore::new(pool.clone());
        let owner_id = seed_user(&pool, "owner").await;
        let repository_id = seed_repository(&pool, owner_id, "hello").await;
        let wiki = store
            .find_or_create(NewWiki {
                repository_id,
                disk_path: "a/hello.wiki.git".to_string(),
            })
            .await
            .unwrap();
        assert_eq!(wiki.repository_id, repository_id);
        assert_eq!(wiki.disk_path, "a/hello.wiki.git");
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn find_or_create_is_idempotent_against_a_real_database(pool: PgPool) {
        let store = PostgresWikiStore::new(pool.clone());
        let owner_id = seed_user(&pool, "owner").await;
        let repository_id = seed_repository(&pool, owner_id, "hello").await;
        let first = store
            .find_or_create(NewWiki {
                repository_id,
                disk_path: "a/hello.wiki.git".to_string(),
            })
            .await
            .unwrap();
        let second = store
            .find_or_create(NewWiki {
                repository_id,
                disk_path: "different.wiki.git".to_string(),
            })
            .await
            .unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(
            second.disk_path, "a/hello.wiki.git",
            "the ON CONFLICT branch must not overwrite the original disk_path"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn find_by_repository_id_finds_what_find_or_create_made(pool: PgPool) {
        let store = PostgresWikiStore::new(pool.clone());
        let owner_id = seed_user(&pool, "owner").await;
        let repository_id = seed_repository(&pool, owner_id, "hello").await;
        let created = store
            .find_or_create(NewWiki {
                repository_id,
                disk_path: "a/hello.wiki.git".to_string(),
            })
            .await
            .unwrap();
        let found = store
            .find_by_repository_id(repository_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.id, created.id);
    }
}
