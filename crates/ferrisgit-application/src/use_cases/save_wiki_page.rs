use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::repository::RepositoryStorePort;
use ferrisgit_domain::wiki::{NewWiki, WikiStorePort, wiki_disk_path_for};
use ferrisgit_domain::wiki_page::{WikiRevision, WikiWriterPort, is_valid_wiki_slug};
use uuid::Uuid;

pub struct SaveWikiPageUseCase {
    repositories: Arc<dyn RepositoryStorePort>,
    wikis: Arc<dyn WikiStorePort>,
    wiki_writer: Arc<dyn WikiWriterPort>,
}

impl SaveWikiPageUseCase {
    pub fn new(
        repositories: Arc<dyn RepositoryStorePort>,
        wikis: Arc<dyn WikiStorePort>,
        wiki_writer: Arc<dyn WikiWriterPort>,
    ) -> Self {
        Self {
            repositories,
            wikis,
            wiki_writer,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        &self,
        repository_id: Uuid,
        slug: &str,
        content: &str,
        base_sha: Option<&str>,
        author_name: &str,
        author_email: &str,
        message: &str,
    ) -> Result<WikiRevision, DomainError> {
        if !is_valid_wiki_slug(slug) {
            return Err(DomainError::Validation(format!(
                "'{slug}' is not a valid wiki page name"
            )));
        }
        let repository = self
            .repositories
            .find_by_id(repository_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("repository".to_string()))?;
        let wiki_disk_path = wiki_disk_path_for(&repository.disk_path);

        self.wiki_writer
            .ensure_wiki_repo_exists(&wiki_disk_path)
            .await?;
        self.wikis
            .find_or_create(NewWiki {
                repository_id,
                disk_path: wiki_disk_path.clone(),
            })
            .await?;

        self.wiki_writer
            .save_page(
                &wiki_disk_path,
                slug,
                content,
                base_sha,
                author_name,
                author_email,
                message,
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use ferrisgit_domain::repository::{Repository, RepositoryVisibility};

    use super::*;
    use crate::test_support::{FakeRepositories, FakeWikiWriter, FakeWikis};

    fn repository() -> Repository {
        Repository {
            id: Uuid::new_v4(),
            owner_id: Uuid::new_v4(),
            name: "hello".to_string(),
            group_id: None,
            description: String::new(),
            disk_path: "alice/hello.git".to_string(),
            visibility: RepositoryVisibility::Private,
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn rejects_an_invalid_slug_before_touching_any_port() {
        let repo = repository();
        let use_case = SaveWikiPageUseCase::new(
            Arc::new(FakeRepositories::new(vec![repo.clone()])),
            Arc::new(FakeWikis::empty()),
            Arc::new(FakeWikiWriter::new()),
        );
        let result = use_case
            .execute(
                repo.id,
                "has/slash",
                "content",
                None,
                "Ada",
                "ada@example.com",
                "msg",
            )
            .await;
        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn derives_the_wiki_disk_path_from_the_repositorys_own_disk_path() {
        let repo = repository();
        let writer = Arc::new(FakeWikiWriter::new());
        let use_case = SaveWikiPageUseCase::new(
            Arc::new(FakeRepositories::new(vec![repo.clone()])),
            Arc::new(FakeWikis::empty()),
            writer.clone(),
        );
        use_case
            .execute(
                repo.id,
                "Home",
                "content",
                None,
                "Ada",
                "ada@example.com",
                "msg",
            )
            .await
            .unwrap();
        assert_eq!(
            writer.ensure_calls(),
            vec!["alice/hello.wiki.git".to_string()]
        );
        assert_eq!(writer.save_calls()[0].0, "alice/hello.wiki.git");
    }

    #[tokio::test]
    async fn creates_the_wikis_row_only_once_across_two_saves() {
        let repo = repository();
        let wikis = Arc::new(FakeWikis::empty());
        let use_case = SaveWikiPageUseCase::new(
            Arc::new(FakeRepositories::new(vec![repo.clone()])),
            wikis.clone(),
            Arc::new(FakeWikiWriter::new()),
        );
        let first_revision = use_case
            .execute(
                repo.id,
                "Home",
                "content",
                None,
                "Ada",
                "ada@example.com",
                "msg",
            )
            .await
            .unwrap();
        let first_id = wikis
            .find_by_repository_id(repo.id)
            .await
            .unwrap()
            .unwrap()
            .id;
        // `base_sha` must be the sha the first save returned: the fake enforces the same compare-and-swap as
        // `GitWikiWriter`.
        use_case
            .execute(
                repo.id,
                "About",
                "content",
                Some(first_revision.commit_sha.as_str()),
                "Ada",
                "ada@example.com",
                "msg",
            )
            .await
            .unwrap();
        let second_id = wikis
            .find_by_repository_id(repo.id)
            .await
            .unwrap()
            .unwrap()
            .id;
        assert_eq!(first_id, second_id);
    }

    #[tokio::test]
    async fn forwards_base_sha_unchanged_to_the_writer() {
        let repo = repository();
        let writer = Arc::new(FakeWikiWriter::new());
        let use_case = SaveWikiPageUseCase::new(
            Arc::new(FakeRepositories::new(vec![repo.clone()])),
            Arc::new(FakeWikis::empty()),
            writer.clone(),
        );
        // A real first save so the second save's `base_sha` is accepted by the fake's compare-and-swap.
        let first = use_case
            .execute(
                repo.id,
                "Home",
                "content",
                None,
                "Ada",
                "ada@example.com",
                "msg",
            )
            .await
            .unwrap();
        use_case
            .execute(
                repo.id,
                "Home",
                "updated content",
                Some(first.commit_sha.as_str()),
                "Ada",
                "ada@example.com",
                "msg",
            )
            .await
            .unwrap();
        assert_eq!(
            writer.save_calls().last().unwrap().2,
            Some(first.commit_sha.clone())
        );
    }
}
