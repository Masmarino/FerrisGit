use std::collections::HashMap;

use crate::error::{conflict_on_duplicate, infra};
use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::release::{
    NewRelease, NewReleaseAsset, Release, ReleaseAsset, ReleaseStorePort, ReleaseUpdate,
};
use sqlx::PgPool;
use uuid::Uuid;

pub struct PostgresReleaseStore {
    pool: PgPool,
}

impl PostgresReleaseStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

struct Row {
    id: Uuid,
    repository_id: Uuid,
    tag_name: String,
    title: String,
    notes: String,
    draft: bool,
    prerelease: bool,
    author_id: Option<Uuid>,
    created_at: chrono::DateTime<chrono::Utc>,
    published_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<Row> for Release {
    fn from(row: Row) -> Self {
        Release {
            id: row.id,
            repository_id: row.repository_id,
            tag_name: row.tag_name,
            title: row.title,
            notes: row.notes,
            draft: row.draft,
            prerelease: row.prerelease,
            author_id: row.author_id,
            created_at: row.created_at,
            published_at: row.published_at,
        }
    }
}

struct AssetRow {
    id: Uuid,
    release_id: Uuid,
    filename: String,
    content_type: String,
    size_bytes: i64,
    disk_path: String,
    uploaded_by: Option<Uuid>,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl From<AssetRow> for ReleaseAsset {
    fn from(row: AssetRow) -> Self {
        ReleaseAsset {
            id: row.id,
            release_id: row.release_id,
            filename: row.filename,
            content_type: row.content_type,
            size_bytes: row.size_bytes,
            disk_path: row.disk_path,
            uploaded_by: row.uploaded_by,
            created_at: row.created_at,
        }
    }
}

#[async_trait]
impl ReleaseStorePort for PostgresReleaseStore {
    async fn create(&self, new_release: NewRelease) -> Result<Release, DomainError> {
        let row = sqlx::query_as!(
            Row,
            "INSERT INTO releases (repository_id, tag_name, title, notes, draft, prerelease, author_id, published_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, CASE WHEN $5 THEN NULL ELSE now() END) \
             RETURNING id, repository_id, tag_name, title, notes, draft, prerelease, author_id, created_at, published_at",
            new_release.repository_id,
            new_release.tag_name,
            new_release.title,
            new_release.notes,
            new_release.draft,
            new_release.prerelease,
            new_release.author_id,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(conflict_on_duplicate(|| {
            format!("a release for tag '{}' already exists", new_release.tag_name)
        }))?;
        Ok(row.into())
    }

    async fn list_for_repository(
        &self,
        repository_id: Uuid,
        include_drafts: bool,
    ) -> Result<Vec<Release>, DomainError> {
        let rows = sqlx::query_as!(
            Row,
            "SELECT id, repository_id, tag_name, title, notes, draft, prerelease, author_id, created_at, published_at FROM releases \
             WHERE repository_id = $1 AND (draft = FALSE OR $2) \
             ORDER BY COALESCE(published_at, created_at) DESC",
            repository_id,
            include_drafts,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn find_by_tag_name(
        &self,
        repository_id: Uuid,
        tag_name: &str,
    ) -> Result<Option<Release>, DomainError> {
        let row = sqlx::query_as!(
            Row,
            "SELECT id, repository_id, tag_name, title, notes, draft, prerelease, author_id, created_at, published_at FROM releases \
             WHERE repository_id = $1 AND tag_name = $2",
            repository_id,
            tag_name,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(infra)?;
        Ok(row.map(Into::into))
    }

    async fn update(
        &self,
        id: Uuid,
        repository_id: Uuid,
        update: ReleaseUpdate,
    ) -> Result<Release, DomainError> {
        let row = sqlx::query_as!(
            Row,
            "UPDATE releases SET \
             title = COALESCE($1, title), \
             notes = COALESCE($2, notes), \
             prerelease = COALESCE($3, prerelease) \
             WHERE id = $4 AND repository_id = $5 \
             RETURNING id, repository_id, tag_name, title, notes, draft, prerelease, author_id, created_at, published_at",
            update.title,
            update.notes,
            update.prerelease,
            id,
            repository_id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(infra)?
        .ok_or_else(|| DomainError::NotFound("release".to_string()))?;
        Ok(row.into())
    }

    async fn publish(&self, id: Uuid, repository_id: Uuid) -> Result<Release, DomainError> {
        let row = sqlx::query_as!(
            Row,
            "UPDATE releases SET draft = FALSE, published_at = COALESCE(published_at, now()) \
             WHERE id = $1 AND repository_id = $2 \
             RETURNING id, repository_id, tag_name, title, notes, draft, prerelease, author_id, created_at, published_at",
            id,
            repository_id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(infra)?
        .ok_or_else(|| DomainError::NotFound("release".to_string()))?;
        Ok(row.into())
    }

    async fn delete(&self, id: Uuid, repository_id: Uuid) -> Result<(), DomainError> {
        let result = sqlx::query!(
            "DELETE FROM releases WHERE id = $1 AND repository_id = $2",
            id,
            repository_id
        )
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound("release".to_string()));
        }
        Ok(())
    }

    async fn create_asset(&self, new_asset: NewReleaseAsset) -> Result<ReleaseAsset, DomainError> {
        let row = sqlx::query_as!(
            AssetRow,
            "INSERT INTO release_assets (id, release_id, filename, content_type, size_bytes, disk_path, uploaded_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             RETURNING id, release_id, filename, content_type, size_bytes, disk_path, uploaded_by, created_at",
            new_asset.id,
            new_asset.release_id,
            new_asset.filename,
            new_asset.content_type,
            new_asset.size_bytes,
            new_asset.disk_path,
            new_asset.uploaded_by,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(infra)?;
        Ok(row.into())
    }

    async fn list_assets(&self, release_id: Uuid) -> Result<Vec<ReleaseAsset>, DomainError> {
        let rows = sqlx::query_as!(
            AssetRow,
            "SELECT id, release_id, filename, content_type, size_bytes, disk_path, uploaded_by, created_at FROM release_assets \
             WHERE release_id = $1 ORDER BY created_at ASC",
            release_id,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn find_asset(
        &self,
        id: Uuid,
        release_id: Uuid,
    ) -> Result<Option<ReleaseAsset>, DomainError> {
        let row = sqlx::query_as!(
            AssetRow,
            "SELECT id, release_id, filename, content_type, size_bytes, disk_path, uploaded_by, created_at FROM release_assets \
             WHERE id = $1 AND release_id = $2",
            id,
            release_id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(infra)?;
        Ok(row.map(Into::into))
    }

    async fn delete_asset(&self, id: Uuid, release_id: Uuid) -> Result<(), DomainError> {
        let result = sqlx::query!(
            "DELETE FROM release_assets WHERE id = $1 AND release_id = $2",
            id,
            release_id
        )
        .execute(&self.pool)
        .await
        .map_err(infra)?;
        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound("release asset".to_string()));
        }
        Ok(())
    }

    async fn asset_counts(&self, release_ids: &[Uuid]) -> Result<HashMap<Uuid, i64>, DomainError> {
        if release_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = sqlx::query_as::<_, (Uuid, i64)>("SELECT release_id, COUNT(*)::bigint FROM release_assets WHERE release_id = ANY($1) GROUP BY release_id")
            .bind(release_ids)
            .fetch_all(&self.pool)
            .await
            .map_err(infra)?;
        Ok(rows.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::postgres::test_support::{seed_repository, seed_user};

    fn new_release(
        repository_id: Uuid,
        author_id: Uuid,
        tag_name: &str,
        draft: bool,
    ) -> NewRelease {
        NewRelease {
            repository_id,
            tag_name: tag_name.to_string(),
            title: "v1.0.0".to_string(),
            notes: "notes".to_string(),
            draft,
            prerelease: false,
            author_id,
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn creating_a_published_release_stamps_published_at_immediately(pool: PgPool) {
        let owner_id = seed_user(&pool, "owner").await;
        let repository_id = seed_repository(&pool, owner_id, "hello").await;
        let store = PostgresReleaseStore::new(pool);

        let created = store
            .create(new_release(repository_id, owner_id, "v1.0.0", false))
            .await
            .unwrap();

        assert!(!created.draft);
        assert!(created.published_at.is_some());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn creating_a_draft_release_leaves_published_at_unset(pool: PgPool) {
        let owner_id = seed_user(&pool, "owner").await;
        let repository_id = seed_repository(&pool, owner_id, "hello").await;
        let store = PostgresReleaseStore::new(pool);

        let created = store
            .create(new_release(repository_id, owner_id, "v1.0.0", true))
            .await
            .unwrap();

        assert!(created.draft);
        assert!(created.published_at.is_none());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn creating_two_releases_for_the_same_tag_in_one_repository_is_a_conflict(pool: PgPool) {
        let owner_id = seed_user(&pool, "owner").await;
        let repository_id = seed_repository(&pool, owner_id, "hello").await;
        let store = PostgresReleaseStore::new(pool);
        store
            .create(new_release(repository_id, owner_id, "v1.0.0", false))
            .await
            .unwrap();

        let result = store
            .create(new_release(repository_id, owner_id, "v1.0.0", false))
            .await;

        assert!(matches!(result, Err(DomainError::Conflict(_))));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn list_for_repository_excludes_drafts_unless_asked_for(pool: PgPool) {
        let owner_id = seed_user(&pool, "owner").await;
        let repository_id = seed_repository(&pool, owner_id, "hello").await;
        let store = PostgresReleaseStore::new(pool);
        let published = store
            .create(new_release(repository_id, owner_id, "v1.0.0", false))
            .await
            .unwrap();
        let draft = store
            .create(new_release(repository_id, owner_id, "v2.0.0-rc1", true))
            .await
            .unwrap();

        let visible_to_reader = store
            .list_for_repository(repository_id, false)
            .await
            .unwrap();
        assert_eq!(
            visible_to_reader.iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![published.id]
        );

        let visible_to_maintainer = store
            .list_for_repository(repository_id, true)
            .await
            .unwrap();
        let mut ids: Vec<Uuid> = visible_to_maintainer.iter().map(|r| r.id).collect();
        ids.sort();
        let mut expected = vec![published.id, draft.id];
        expected.sort();
        assert_eq!(ids, expected);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn publish_sets_draft_false_and_stamps_published_at_only_once(pool: PgPool) {
        let owner_id = seed_user(&pool, "owner").await;
        let repository_id = seed_repository(&pool, owner_id, "hello").await;
        let store = PostgresReleaseStore::new(pool);
        let created = store
            .create(new_release(repository_id, owner_id, "v1.0.0", true))
            .await
            .unwrap();
        assert!(created.published_at.is_none());

        let first_publish = store.publish(created.id, repository_id).await.unwrap();
        assert!(!first_publish.draft);
        let first_timestamp = first_publish.published_at.unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let second_publish = store.publish(created.id, repository_id).await.unwrap();

        assert_eq!(
            second_publish.published_at.unwrap(),
            first_timestamp,
            "publishing an already-published release must not move published_at"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn update_changes_only_the_given_fields_and_never_draft(pool: PgPool) {
        let owner_id = seed_user(&pool, "owner").await;
        let repository_id = seed_repository(&pool, owner_id, "hello").await;
        let store = PostgresReleaseStore::new(pool);
        let created = store
            .create(new_release(repository_id, owner_id, "v1.0.0", true))
            .await
            .unwrap();

        let updated = store
            .update(
                created.id,
                repository_id,
                ReleaseUpdate {
                    title: Some("v1.0.0 — GA".to_string()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

        assert_eq!(updated.title, "v1.0.0 — GA");
        assert_eq!(
            updated.notes, "notes",
            "notes must be unchanged when not part of the update"
        );
        assert!(
            updated.draft,
            "update must never touch draft — that's publish's job"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn delete_is_scoped_to_the_given_repository(pool: PgPool) {
        let owner_id = seed_user(&pool, "owner").await;
        let repository_id = seed_repository(&pool, owner_id, "a").await;
        let other_repository_id = seed_repository(&pool, owner_id, "b").await;
        let store = PostgresReleaseStore::new(pool);
        let created = store
            .create(new_release(repository_id, owner_id, "v1.0.0", false))
            .await
            .unwrap();

        let result = store.delete(created.id, other_repository_id).await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
        assert!(
            store
                .find_by_tag_name(repository_id, "v1.0.0")
                .await
                .unwrap()
                .is_some()
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn assets_round_trip_and_are_scoped_to_their_release(pool: PgPool) {
        let owner_id = seed_user(&pool, "owner").await;
        let repository_id = seed_repository(&pool, owner_id, "hello").await;
        let store = PostgresReleaseStore::new(pool);
        let release = store
            .create(new_release(repository_id, owner_id, "v1.0.0", false))
            .await
            .unwrap();
        let other_release = store
            .create(new_release(repository_id, owner_id, "v2.0.0", false))
            .await
            .unwrap();

        let asset = store
            .create_asset(NewReleaseAsset {
                id: Uuid::new_v4(),
                release_id: release.id,
                filename: "binary.tar.gz".to_string(),
                content_type: "application/gzip".to_string(),
                size_bytes: 1024,
                disk_path: "release-assets/x/y/z".to_string(),
                uploaded_by: owner_id,
            })
            .await
            .unwrap();

        assert_eq!(store.list_assets(release.id).await.unwrap().len(), 1);
        assert!(
            store
                .list_assets(other_release.id)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            store
                .find_asset(asset.id, other_release.id)
                .await
                .unwrap()
                .is_none(),
            "an asset must not be reachable through a release it doesn't belong to"
        );
        assert!(
            store
                .find_asset(asset.id, release.id)
                .await
                .unwrap()
                .is_some()
        );

        let delete_through_wrong_release = store.delete_asset(asset.id, other_release.id).await;
        assert!(matches!(
            delete_through_wrong_release,
            Err(DomainError::NotFound(_))
        ));

        store.delete_asset(asset.id, release.id).await.unwrap();
        assert!(store.list_assets(release.id).await.unwrap().is_empty());
    }

    async fn seed_asset(
        store: &PostgresReleaseStore,
        release_id: Uuid,
        uploaded_by: Uuid,
        filename: &str,
    ) {
        store
            .create_asset(NewReleaseAsset {
                id: Uuid::new_v4(),
                release_id,
                filename: filename.to_string(),
                content_type: "application/octet-stream".to_string(),
                size_bytes: 1,
                disk_path: format!("release-assets/{filename}"),
                uploaded_by,
            })
            .await
            .unwrap();
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn asset_counts_returns_the_number_of_assets_per_release(pool: PgPool) {
        let owner_id = seed_user(&pool, "owner").await;
        let repository_id = seed_repository(&pool, owner_id, "hello").await;
        let store = PostgresReleaseStore::new(pool);
        let with_two = store
            .create(new_release(repository_id, owner_id, "v1.0.0", false))
            .await
            .unwrap();
        let with_one = store
            .create(new_release(repository_id, owner_id, "v2.0.0", false))
            .await
            .unwrap();
        let without = store
            .create(new_release(repository_id, owner_id, "v3.0.0", false))
            .await
            .unwrap();
        seed_asset(&store, with_two.id, owner_id, "a.bin").await;
        seed_asset(&store, with_two.id, owner_id, "b.bin").await;
        seed_asset(&store, with_one.id, owner_id, "c.bin").await;

        let counts = store
            .asset_counts(&[with_two.id, with_one.id, without.id])
            .await
            .unwrap();

        assert_eq!(counts.get(&with_two.id), Some(&2));
        assert_eq!(counts.get(&with_one.id), Some(&1));
        assert_eq!(
            counts.get(&without.id).copied().unwrap_or(0),
            0,
            "a release with no asset counts 0 (absent or zero)"
        );
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn asset_counts_only_counts_the_requested_releases_and_short_circuits_on_an_empty_slice(
        pool: PgPool,
    ) {
        let owner_id = seed_user(&pool, "owner").await;
        let repository_id = seed_repository(&pool, owner_id, "hello").await;
        let store = PostgresReleaseStore::new(pool);
        let asked = store
            .create(new_release(repository_id, owner_id, "v1.0.0", false))
            .await
            .unwrap();
        let other = store
            .create(new_release(repository_id, owner_id, "v2.0.0", false))
            .await
            .unwrap();
        seed_asset(&store, asked.id, owner_id, "a.bin").await;
        seed_asset(&store, other.id, owner_id, "b.bin").await;

        let counts = store.asset_counts(&[asked.id]).await.unwrap();
        assert_eq!(counts.len(), 1);
        assert_eq!(counts.get(&asked.id), Some(&1));

        assert!(store.asset_counts(&[]).await.unwrap().is_empty());
    }
}
