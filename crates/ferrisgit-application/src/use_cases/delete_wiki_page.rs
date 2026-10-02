use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::wiki::WikiStorePort;
use ferrisgit_domain::wiki_page::{WikiWriterPort, is_valid_wiki_slug};
use uuid::Uuid;

pub struct DeleteWikiPageUseCase {
    wikis: Arc<dyn WikiStorePort>,
    wiki_writer: Arc<dyn WikiWriterPort>,
}

impl DeleteWikiPageUseCase {
    pub fn new(wikis: Arc<dyn WikiStorePort>, wiki_writer: Arc<dyn WikiWriterPort>) -> Self {
        Self { wikis, wiki_writer }
    }

    pub async fn execute(
        &self,
        repository_id: Uuid,
        slug: &str,
        base_sha: &str,
        author_name: &str,
        author_email: &str,
        message: &str,
    ) -> Result<(), DomainError> {
        if !is_valid_wiki_slug(slug) {
            return Err(DomainError::Validation(format!(
                "'{slug}' is not a valid wiki page name"
            )));
        }
        let wiki = self
            .wikis
            .find_by_repository_id(repository_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("wiki".to_string()))?;
        self.wiki_writer
            .delete_page(
                &wiki.disk_path,
                slug,
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
    use ferrisgit_domain::wiki::NewWiki;

    use super::*;
    use crate::test_support::{FakeWikiWriter, FakeWikis};

    #[tokio::test]
    async fn deleting_from_a_repository_with_no_wiki_at_all_is_not_found() {
        let use_case = DeleteWikiPageUseCase::new(
            Arc::new(FakeWikis::empty()),
            Arc::new(FakeWikiWriter::new()),
        );
        let result = use_case
            .execute(
                Uuid::new_v4(),
                "Home",
                "deadbeef",
                "Ada",
                "ada@example.com",
                "msg",
            )
            .await;
        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }

    /// Validation has to come first: with an empty `FakeWikis` a missing check would show up as a plain `NotFound`.
    #[tokio::test]
    async fn rejects_an_invalid_slug_before_looking_up_the_wiki() {
        let use_case = DeleteWikiPageUseCase::new(
            Arc::new(FakeWikis::empty()),
            Arc::new(FakeWikiWriter::new()),
        );
        let result = use_case
            .execute(
                Uuid::new_v4(),
                "has/slash",
                "deadbeef",
                "Ada",
                "ada@example.com",
                "msg",
            )
            .await;
        assert!(
            matches!(result, Err(DomainError::Validation(_))),
            "expected Validation, got {result:?}"
        );
    }

    #[tokio::test]
    async fn deleting_an_existing_wikis_page_delegates_to_the_writer_with_the_right_disk_path() {
        let wikis = Arc::new(FakeWikis::empty());
        let repository_id = Uuid::new_v4();
        wikis
            .find_or_create(NewWiki {
                repository_id,
                disk_path: "alice/hello.wiki.git".to_string(),
            })
            .await
            .unwrap();
        let writer = Arc::new(FakeWikiWriter::new());
        // The fake writer does the same compare-and-swap as the real one, so the page must exist at `base_sha`.
        let saved = writer
            .save_page(
                "alice/hello.wiki.git",
                "Home",
                "# Home",
                None,
                "Ada",
                "ada@example.com",
                "Create Home",
            )
            .await
            .unwrap();
        let use_case = DeleteWikiPageUseCase::new(wikis, writer.clone());

        use_case
            .execute(
                repository_id,
                "Home",
                &saved.commit_sha,
                "Ada",
                "ada@example.com",
                "Remove Home",
            )
            .await
            .unwrap();

        assert_eq!(
            writer.delete_calls(),
            vec![(
                "alice/hello.wiki.git".to_string(),
                "Home".to_string(),
                saved.commit_sha.clone()
            )]
        );
        assert_eq!(
            writer.page_content_of("alice/hello.wiki.git", "Home"),
            None,
            "the page must actually be gone from the writer's state"
        );
    }
}
