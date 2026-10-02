use crate::error::infra;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::public_pages::{
    PublicCatalogEntry, PublicCatalogPage, PublicCatalogPort, PublicCatalogQuery, PublicCatalogSort,
};
use sqlx::PgPool;
use uuid::Uuid;

/// Prefixes `$tail` with the `filtered` CTE: public repositories with their full path (the owner, or the group chain,
/// then the name) and star count. `$1` is the optional search text, matched as a plain substring (`strpos`, so `%`
/// and `_` are not wildcards). A macro because sqlx only takes literal SQL.
macro_rules! filtered_catalog {
    ($tail:literal) => {
        concat!("\
WITH RECURSIVE group_paths (id, path) AS ( \
    SELECT id, ARRAY[name] FROM groups WHERE parent_group_id IS NULL \
    UNION ALL \
    SELECT g.id, gp.path || g.name FROM groups g JOIN group_paths gp ON g.parent_group_id = gp.id \
), \
catalog AS ( \
    SELECT r.id, r.name, r.description, r.created_at, u.username AS owner, \
           COALESCE(gp.path, ARRAY[u.username]) || r.name AS path, \
           (SELECT count(*) FROM repository_stars s WHERE s.repository_id = r.id) AS stars \
    FROM repositories r \
    JOIN users u ON u.id = r.owner_id \
    LEFT JOIN group_paths gp ON gp.id = r.group_id \
    WHERE r.visibility = 'public' \
), \
filtered AS ( \
    SELECT * FROM catalog \
    WHERE $1::text IS NULL \
       OR strpos(lower(array_to_string(path, '/')), lower($1::text)) > 0 \
       OR strpos(lower(description), lower($1::text)) > 0 \
) ", $tail)
    };
}

type CatalogRow = (
    Uuid,
    String,
    String,
    DateTime<Utc>,
    String,
    Vec<String>,
    i64,
);

pub struct PostgresPublicCatalogStore {
    pool: PgPool,
}

impl PostgresPublicCatalogStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn sort_key(sort: PublicCatalogSort) -> &'static str {
    match sort {
        PublicCatalogSort::Stars => "stars",
        PublicCatalogSort::Name => "name",
        PublicCatalogSort::Created => "created",
    }
}

#[async_trait]
impl PublicCatalogPort for PostgresPublicCatalogStore {
    async fn search(&self, query: &PublicCatalogQuery) -> Result<PublicCatalogPage, DomainError> {
        let total: i64 = sqlx::query_scalar(filtered_catalog!("SELECT count(*) FROM filtered"))
            .bind(query.text.as_deref())
            .fetch_one(&self.pool)
            .await
            .map_err(infra)?;

        let offset = i64::from(query.page.saturating_sub(1)) * i64::from(query.per_page);
        // Every sort ends on name, path and id so pages never overlap or skip a row.
        let rows: Vec<CatalogRow> = sqlx::query_as(filtered_catalog!(
            "SELECT id, name, description, created_at, owner, path, stars FROM filtered \
             ORDER BY \
                 CASE WHEN $2 = 'stars' THEN stars END DESC NULLS LAST, \
                 CASE WHEN $2 = 'created' THEN created_at END DESC NULLS LAST, \
                 lower(name), array_to_string(path, '/'), id \
             LIMIT $3 OFFSET $4"
        ))
        .bind(query.text.as_deref())
        .bind(sort_key(query.sort))
        .bind(i64::from(query.per_page))
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(infra)?;

        Ok(PublicCatalogPage {
            items: rows
                .into_iter()
                .map(
                    |(id, name, description, created_at, owner, path, stars)| PublicCatalogEntry {
                        id,
                        name,
                        path,
                        owner,
                        description,
                        stars,
                        created_at,
                    },
                )
                .collect(),
            total,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::postgres::test_support::seed_user;

    async fn seed_group(pool: &PgPool, parent: Option<Uuid>, name: &str, creator: Uuid) -> Uuid {
        sqlx::query_scalar(
            "INSERT INTO groups (parent_group_id, name, created_by) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(parent)
        .bind(name)
        .bind(creator)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    struct Seed<'a> {
        owner: Uuid,
        group: Option<Uuid>,
        name: &'a str,
        description: &'a str,
        public: bool,
        age_days: i32,
    }

    async fn seed_repo(pool: &PgPool, seed: Seed<'_>) -> Uuid {
        sqlx::query_scalar(
            "INSERT INTO repositories (owner_id, group_id, name, description, disk_path, visibility, created_at) \
             VALUES ($1, $2, $3, $4, $3 || '-' || gen_random_uuid()::text, $5, now() - make_interval(days => $6)) \
             RETURNING id",
        )
        .bind(seed.owner)
        .bind(seed.group)
        .bind(seed.name)
        .bind(seed.description)
        .bind(if seed.public { "public" } else { "private" })
        .bind(seed.age_days)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn star(pool: &PgPool, repository_id: Uuid, user_id: Uuid) {
        sqlx::query("INSERT INTO repository_stars (repository_id, user_id) VALUES ($1, $2)")
            .bind(repository_id)
            .bind(user_id)
            .execute(pool)
            .await
            .unwrap();
    }

    fn query(
        text: Option<&str>,
        sort: PublicCatalogSort,
        page: u32,
        per_page: u32,
    ) -> PublicCatalogQuery {
        PublicCatalogQuery {
            text: text.map(str::to_string),
            sort,
            page,
            per_page,
        }
    }

    fn names(page: &PublicCatalogPage) -> Vec<&str> {
        page.items.iter().map(|e| e.name.as_str()).collect()
    }

    /// alice owns `zeta` (2 stars, 1 day old) and the private `secret`; bob owns `alpha` (no star, 30 days old); the
    /// group `acme/backend` holds `middle` (1 star, 10 days old), created by bob.
    async fn seed_catalog(pool: &PgPool) {
        let alice = seed_user(pool, "alice").await;
        let bob = seed_user(pool, "bob").await;
        let acme = seed_group(pool, None, "acme", bob).await;
        let backend = seed_group(pool, Some(acme), "backend", bob).await;
        let zeta = seed_repo(
            pool,
            Seed {
                owner: alice,
                group: None,
                name: "zeta",
                description: "Rust tooling",
                public: true,
                age_days: 1,
            },
        )
        .await;
        seed_repo(
            pool,
            Seed {
                owner: alice,
                group: None,
                name: "secret",
                description: "Rust secrets",
                public: false,
                age_days: 0,
            },
        )
        .await;
        seed_repo(
            pool,
            Seed {
                owner: bob,
                group: None,
                name: "alpha",
                description: "100%_literal",
                public: true,
                age_days: 30,
            },
        )
        .await;
        let middle = seed_repo(
            pool,
            Seed {
                owner: bob,
                group: Some(backend),
                name: "middle",
                description: "",
                public: true,
                age_days: 10,
            },
        )
        .await;
        star(pool, zeta, alice).await;
        star(pool, zeta, bob).await;
        star(pool, middle, alice).await;
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn lists_only_public_repositories_with_their_path_owner_and_stars(pool: PgPool) {
        seed_catalog(&pool).await;
        let store = PostgresPublicCatalogStore::new(pool);

        let page = store
            .search(&query(None, PublicCatalogSort::Stars, 1, 20))
            .await
            .unwrap();

        assert_eq!(page.total, 3);
        assert_eq!(names(&page), vec!["zeta", "middle", "alpha"]);
        let middle = &page.items[1];
        assert_eq!(middle.path, vec!["acme", "backend", "middle"]);
        assert_eq!(middle.owner, "bob");
        assert_eq!(middle.stars, 1);
        let zeta = &page.items[0];
        assert_eq!(zeta.path, vec!["alice", "zeta"]);
        assert_eq!(zeta.stars, 2);
        assert_eq!(zeta.description, "Rust tooling");
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn sorts_by_name_or_newest_first(pool: PgPool) {
        seed_catalog(&pool).await;
        let store = PostgresPublicCatalogStore::new(pool);

        let by_name = store
            .search(&query(None, PublicCatalogSort::Name, 1, 20))
            .await
            .unwrap();
        let by_date = store
            .search(&query(None, PublicCatalogSort::Created, 1, 20))
            .await
            .unwrap();

        assert_eq!(names(&by_name), vec!["alpha", "middle", "zeta"]);
        assert_eq!(names(&by_date), vec!["zeta", "middle", "alpha"]);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn matches_a_case_insensitive_substring_of_name_path_or_description(pool: PgPool) {
        seed_catalog(&pool).await;
        let store = PostgresPublicCatalogStore::new(pool);
        let search = |text: &'static str| {
            let store = &store;
            async move {
                store
                    .search(&query(Some(text), PublicCatalogSort::Name, 1, 20))
                    .await
                    .unwrap()
            }
        };

        assert_eq!(names(&search("ZET").await), vec!["zeta"]);
        assert_eq!(names(&search("acme/back").await), vec!["middle"]);
        assert_eq!(names(&search("alice/").await), vec!["zeta"]);
        let rust = search("rust").await;
        assert_eq!(names(&rust), vec!["zeta"], "the private match stays hidden");
        assert_eq!(rust.total, 1);
        assert_eq!(names(&search("%_lit").await), vec!["alpha"]);
        assert_eq!(names(&search("_").await), vec!["alpha"], "no wildcard");
        assert_eq!(search("nothing-like-this").await.total, 0);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn pages_through_the_results_with_the_total_of_all_matches(pool: PgPool) {
        seed_catalog(&pool).await;
        let store = PostgresPublicCatalogStore::new(pool);

        let first = store
            .search(&query(None, PublicCatalogSort::Name, 1, 2))
            .await
            .unwrap();
        let second = store
            .search(&query(None, PublicCatalogSort::Name, 2, 2))
            .await
            .unwrap();
        let beyond = store
            .search(&query(None, PublicCatalogSort::Name, 5, 2))
            .await
            .unwrap();

        assert_eq!(names(&first), vec!["alpha", "middle"]);
        assert_eq!(names(&second), vec!["zeta"]);
        assert!(beyond.items.is_empty());
        assert_eq!((first.total, second.total, beyond.total), (3, 3, 3));
    }
}
