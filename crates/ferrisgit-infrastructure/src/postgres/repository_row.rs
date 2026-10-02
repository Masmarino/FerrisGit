use chrono::{DateTime, Utc};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::repository::{Repository, RepositoryVisibility};
use uuid::Uuid;

/// The eight `repositories` columns, shared by every store that returns a `Repository`.
#[derive(sqlx::FromRow)]
pub(super) struct RepositoryRow {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub name: String,
    pub group_id: Option<Uuid>,
    pub description: String,
    pub disk_path: String,
    pub visibility: String,
    pub created_at: DateTime<Utc>,
}

impl TryFrom<RepositoryRow> for Repository {
    type Error = DomainError;

    fn try_from(row: RepositoryRow) -> Result<Self, DomainError> {
        Ok(Repository {
            id: row.id,
            owner_id: row.owner_id,
            name: row.name,
            group_id: row.group_id,
            description: row.description,
            disk_path: row.disk_path,
            visibility: RepositoryVisibility::parse(&row.visibility)?,
            created_at: row.created_at,
        })
    }
}
