use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::public_pages::{
    PublicCatalogPage, PublicCatalogPort, PublicCatalogQuery, PublicCatalogSort,
    PublicPagesSettingsPort,
};
use ferrisgit_domain::repository::{Repository, RepositoryStorePort, RepositoryVisibility};
use uuid::Uuid;

pub const MAX_QUERY_CHARS: usize = 100;
pub const DEFAULT_PER_PAGE: u32 = 20;
pub const MAX_PER_PAGE: u32 = 50;
pub const MAX_PAGE: u32 = 100;

/// The single decision of whether an anonymous visitor may read a repository: it must be `Public` and the
/// instance must have its public pages on. Every refusal is the same `NotFound` as an unknown id.
pub struct RequirePublicRepositoryUseCase {
    settings: Arc<dyn PublicPagesSettingsPort>,
    repositories: Arc<dyn RepositoryStorePort>,
}

impl RequirePublicRepositoryUseCase {
    pub fn new(
        settings: Arc<dyn PublicPagesSettingsPort>,
        repositories: Arc<dyn RepositoryStorePort>,
    ) -> Self {
        Self {
            settings,
            repositories,
        }
    }

    pub async fn execute(&self, repository_id: Uuid) -> Result<Repository, DomainError> {
        // Both lookups always run, so a switched-off instance answers no faster than an unknown id.
        let (settings, repository) = tokio::join!(
            self.settings.get(),
            self.repositories.find_by_id(repository_id)
        );
        let (settings, repository) = (settings?, repository?);
        match repository {
            Some(repo)
                if settings.public_pages_enabled
                    && repo.visibility == RepositoryVisibility::Public =>
            {
                Ok(repo)
            }
            _ => Err(DomainError::NotFound("repository".to_string())),
        }
    }
}

/// The query string as received: everything is parsed and bounded here so a bad value is a 400 with a reason.
#[derive(Debug, Clone, Default)]
pub struct RawPublicCatalogSearch {
    pub q: Option<String>,
    pub sort: Option<String>,
    pub page: Option<String>,
    pub per_page: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PublicCatalogResult {
    pub page: PublicCatalogPage,
    pub page_number: u32,
    pub per_page: u32,
}

pub struct SearchPublicRepositoriesUseCase {
    settings: Arc<dyn PublicPagesSettingsPort>,
    catalog: Arc<dyn PublicCatalogPort>,
}

impl SearchPublicRepositoriesUseCase {
    pub fn new(
        settings: Arc<dyn PublicPagesSettingsPort>,
        catalog: Arc<dyn PublicCatalogPort>,
    ) -> Self {
        Self { settings, catalog }
    }

    pub async fn execute(
        &self,
        raw: RawPublicCatalogSearch,
    ) -> Result<PublicCatalogResult, DomainError> {
        if !self.settings.get().await?.public_pages_enabled {
            return Err(DomainError::NotFound("not found".to_string()));
        }
        let query = parse_query(raw)?;
        let page = self.catalog.search(&query).await?;
        Ok(PublicCatalogResult {
            page,
            page_number: query.page,
            per_page: query.per_page,
        })
    }
}

fn bounded_number(
    name: &str,
    raw: Option<String>,
    default: u32,
    max: u32,
) -> Result<u32, DomainError> {
    let Some(raw) = raw.filter(|r| !r.is_empty()) else {
        return Ok(default);
    };
    match raw.parse::<u32>() {
        Ok(n) if (1..=max).contains(&n) => Ok(n),
        _ => Err(DomainError::Validation(format!(
            "{name} must be a number between 1 and {max}"
        ))),
    }
}

fn parse_query(raw: RawPublicCatalogSearch) -> Result<PublicCatalogQuery, DomainError> {
    let text = raw
        .q
        .map(|q| q.trim().to_string())
        .filter(|q| !q.is_empty());
    if text
        .as_ref()
        .is_some_and(|q| q.chars().count() > MAX_QUERY_CHARS)
    {
        return Err(DomainError::Validation(format!(
            "q must not exceed {MAX_QUERY_CHARS} characters"
        )));
    }
    let sort = match raw.sort.filter(|s| !s.is_empty()) {
        Some(sort) => PublicCatalogSort::parse(&sort)?,
        None => PublicCatalogSort::default(),
    };
    Ok(PublicCatalogQuery {
        text,
        sort,
        page: bounded_number("page", raw.page, 1, MAX_PAGE)?,
        per_page: bounded_number("perPage", raw.per_page, DEFAULT_PER_PAGE, MAX_PER_PAGE)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakePublicPagesSettings, FakeRepositories};
    use async_trait::async_trait;
    use chrono::Utc;
    use ferrisgit_domain::public_pages::{PublicCatalogEntry, PublicPagesSettings};
    use std::sync::Mutex;

    fn repo(visibility: RepositoryVisibility) -> Repository {
        Repository {
            id: Uuid::new_v4(),
            owner_id: Uuid::new_v4(),
            name: "hello".to_string(),
            group_id: None,
            description: String::new(),
            disk_path: "p".to_string(),
            visibility,
            created_at: Utc::now(),
        }
    }

    fn settings(public_pages_enabled: bool) -> Arc<FakePublicPagesSettings> {
        Arc::new(FakePublicPagesSettings::new(PublicPagesSettings {
            public_pages_enabled,
            seo_indexing_enabled: false,
        }))
    }

    async fn require(
        public_pages_enabled: bool,
        repos: Vec<Repository>,
        id: Uuid,
    ) -> Result<Repository, DomainError> {
        RequirePublicRepositoryUseCase::new(
            settings(public_pages_enabled),
            Arc::new(FakeRepositories::new(repos)),
        )
        .execute(id)
        .await
    }

    fn not_found_message(result: Result<Repository, DomainError>) -> String {
        match result {
            Err(DomainError::NotFound(message)) => message,
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_public_repository_is_readable_when_public_pages_are_on() {
        let public = repo(RepositoryVisibility::Public);

        let found = require(true, vec![public.clone()], public.id)
            .await
            .unwrap();

        assert_eq!(found.id, public.id);
    }

    #[tokio::test]
    async fn private_unknown_and_switched_off_are_the_same_not_found() {
        let private = repo(RepositoryVisibility::Private);
        let public = repo(RepositoryVisibility::Public);

        let unknown = not_found_message(require(true, vec![], Uuid::new_v4()).await);
        let hidden = not_found_message(require(true, vec![private.clone()], private.id).await);
        let switched_off = not_found_message(require(false, vec![public.clone()], public.id).await);

        assert_eq!(unknown, hidden);
        assert_eq!(unknown, switched_off);
    }

    #[derive(Default)]
    struct RecordingCatalog(Mutex<Vec<PublicCatalogQuery>>);

    #[async_trait]
    impl PublicCatalogPort for RecordingCatalog {
        async fn search(
            &self,
            query: &PublicCatalogQuery,
        ) -> Result<PublicCatalogPage, DomainError> {
            self.0.lock().unwrap().push(query.clone());
            Ok(PublicCatalogPage {
                items: vec![PublicCatalogEntry {
                    id: Uuid::new_v4(),
                    name: "hello".to_string(),
                    path: vec!["alice".to_string(), "hello".to_string()],
                    owner: "alice".to_string(),
                    description: String::new(),
                    stars: 3,
                    created_at: Utc::now(),
                }],
                total: 1,
            })
        }
    }

    async fn search(
        enabled: bool,
        raw: RawPublicCatalogSearch,
    ) -> (
        Result<PublicCatalogResult, DomainError>,
        Vec<PublicCatalogQuery>,
    ) {
        let catalog = Arc::new(RecordingCatalog::default());
        let result = SearchPublicRepositoriesUseCase::new(settings(enabled), catalog.clone())
            .execute(raw)
            .await;
        let queries = catalog.0.lock().unwrap().clone();
        (result, queries)
    }

    #[tokio::test]
    async fn defaults_to_the_first_page_of_twenty_sorted_by_stars() {
        let (result, queries) = search(true, RawPublicCatalogSearch::default()).await;

        let result = result.unwrap();
        assert_eq!((result.page_number, result.per_page), (1, 20));
        assert_eq!(result.page.total, 1);
        assert_eq!(
            queries,
            vec![PublicCatalogQuery {
                text: None,
                sort: PublicCatalogSort::Stars,
                page: 1,
                per_page: 20,
            }]
        );
    }

    #[tokio::test]
    async fn passes_a_trimmed_query_the_sort_and_the_paging_through() {
        let (result, queries) = search(
            true,
            RawPublicCatalogSearch {
                q: Some("  Hello ".to_string()),
                sort: Some("created".to_string()),
                page: Some("3".to_string()),
                per_page: Some("50".to_string()),
            },
        )
        .await;

        assert_eq!(result.unwrap().page_number, 3);
        assert_eq!(
            queries,
            vec![PublicCatalogQuery {
                text: Some("Hello".to_string()),
                sort: PublicCatalogSort::Created,
                page: 3,
                per_page: 50,
            }]
        );
    }

    #[tokio::test]
    async fn a_blank_query_lists_everything() {
        let (_, queries) = search(
            true,
            RawPublicCatalogSearch {
                q: Some("   ".to_string()),
                ..Default::default()
            },
        )
        .await;

        assert_eq!(queries[0].text, None);
    }

    #[tokio::test]
    async fn out_of_range_or_malformed_input_is_rejected_before_any_lookup() {
        let too_long = "a".repeat(MAX_QUERY_CHARS + 1);
        let invalid = [
            RawPublicCatalogSearch {
                q: Some(too_long),
                ..Default::default()
            },
            RawPublicCatalogSearch {
                sort: Some("downloads".to_string()),
                ..Default::default()
            },
            RawPublicCatalogSearch {
                page: Some("0".to_string()),
                ..Default::default()
            },
            RawPublicCatalogSearch {
                page: Some("101".to_string()),
                ..Default::default()
            },
            RawPublicCatalogSearch {
                page: Some("two".to_string()),
                ..Default::default()
            },
            RawPublicCatalogSearch {
                per_page: Some("51".to_string()),
                ..Default::default()
            },
            RawPublicCatalogSearch {
                per_page: Some("-1".to_string()),
                ..Default::default()
            },
        ];
        for raw in invalid {
            let (result, queries) = search(true, raw.clone()).await;
            assert!(
                matches!(result, Err(DomainError::Validation(_))),
                "{raw:?} must be rejected"
            );
            assert!(queries.is_empty());
        }
    }

    #[tokio::test]
    async fn a_query_of_exactly_the_maximum_length_is_accepted() {
        let (result, _) = search(
            true,
            RawPublicCatalogSearch {
                q: Some("é".repeat(MAX_QUERY_CHARS)),
                ..Default::default()
            },
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn the_catalog_is_not_found_when_public_pages_are_off() {
        let (result, queries) = search(false, RawPublicCatalogSearch::default()).await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
        assert!(queries.is_empty());
    }
}
