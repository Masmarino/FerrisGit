use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::DomainError;

/// The instance switches behind the anonymous pages of public repositories, and behind their anonymous git reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublicPagesSettings {
    pub public_pages_enabled: bool,
    pub seo_indexing_enabled: bool,
}

impl Default for PublicPagesSettings {
    fn default() -> Self {
        Self {
            public_pages_enabled: true,
            seo_indexing_enabled: false,
        }
    }
}

impl PublicPagesSettings {
    /// Indexing a site whose public pages are off would only index the sign-in page.
    pub fn allows_indexing(&self) -> bool {
        self.public_pages_enabled && self.seo_indexing_enabled
    }
}

/// `None` leaves a switch as it is.
#[derive(Debug, Clone, Copy, Default)]
pub struct PublicPagesSettingsUpdate {
    pub public_pages_enabled: Option<bool>,
    pub seo_indexing_enabled: Option<bool>,
}

#[async_trait]
pub trait PublicPagesSettingsPort: Send + Sync {
    async fn get(&self) -> Result<PublicPagesSettings, DomainError>;
    async fn update(
        &self,
        update: PublicPagesSettingsUpdate,
    ) -> Result<PublicPagesSettings, DomainError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PublicCatalogSort {
    /// Most starred first.
    #[default]
    Stars,
    Name,
    /// Newest first.
    Created,
}

impl PublicCatalogSort {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "stars" => Ok(Self::Stars),
            "name" => Ok(Self::Name),
            "created" => Ok(Self::Created),
            other => Err(DomainError::Validation(format!(
                "unknown sort: {other} (expected stars, name or created)"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicCatalogQuery {
    /// Case-insensitive substring of the name, path or description. `None` lists everything.
    pub text: Option<String>,
    pub sort: PublicCatalogSort,
    /// 1-based.
    pub page: u32,
    pub per_page: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PublicCatalogEntry {
    pub id: Uuid,
    pub name: String,
    /// Owner or group chain, then the repository name.
    pub path: Vec<String>,
    pub owner: String,
    pub description: String,
    pub stars: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PublicCatalogPage {
    pub items: Vec<PublicCatalogEntry>,
    pub total: i64,
}

/// Lists `Public` repositories only, whatever the query.
#[async_trait]
pub trait PublicCatalogPort: Send + Sync {
    async fn search(&self, query: &PublicCatalogQuery) -> Result<PublicCatalogPage, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_pages_are_on_and_indexing_off_by_default() {
        let settings = PublicPagesSettings::default();
        assert!(settings.public_pages_enabled);
        assert!(!settings.seo_indexing_enabled);
        assert!(!settings.allows_indexing());
    }

    #[test]
    fn indexing_needs_both_switches() {
        let both = PublicPagesSettings {
            public_pages_enabled: true,
            seo_indexing_enabled: true,
        };
        let pages_off = PublicPagesSettings {
            public_pages_enabled: false,
            seo_indexing_enabled: true,
        };
        assert!(both.allows_indexing());
        assert!(!pages_off.allows_indexing());
    }

    #[test]
    fn sorts_parse_from_their_query_form() {
        assert_eq!(
            PublicCatalogSort::parse("stars").unwrap(),
            PublicCatalogSort::Stars
        );
        assert_eq!(
            PublicCatalogSort::parse("name").unwrap(),
            PublicCatalogSort::Name
        );
        assert_eq!(
            PublicCatalogSort::parse("created").unwrap(),
            PublicCatalogSort::Created
        );
        assert!(matches!(
            PublicCatalogSort::parse("downloads"),
            Err(DomainError::Validation(_))
        ));
    }
}
