use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MilestoneState {
    Open,
    Closed,
}

impl MilestoneState {
    pub fn parse(s: &str) -> Result<Self, DomainError> {
        match s {
            "open" => Ok(MilestoneState::Open),
            "closed" => Ok(MilestoneState::Closed),
            other => Err(DomainError::Validation(format!(
                "invalid milestone state: {other}"
            ))),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            MilestoneState::Open => "open",
            MilestoneState::Closed => "closed",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Milestone {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub due_date: Option<DateTime<Utc>>,
    pub state: MilestoneState,
    pub repository_id: Option<Uuid>,
    pub group_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

pub struct NewMilestone {
    pub title: String,
    pub description: String,
    pub due_date: Option<DateTime<Utc>>,
    pub repository_id: Option<Uuid>,
    pub group_id: Option<Uuid>,
}

#[async_trait]
pub trait MilestoneStorePort: Send + Sync {
    async fn create(&self, new_milestone: NewMilestone) -> Result<Milestone, DomainError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Milestone>, DomainError>;
    async fn update(
        &self,
        id: Uuid,
        title: String,
        description: String,
        due_date: Option<DateTime<Utc>>,
        state: MilestoneState,
    ) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
    async fn list_for_repository(&self, repository_id: Uuid)
    -> Result<Vec<Milestone>, DomainError>;
    async fn list_for_group(&self, group_id: Uuid) -> Result<Vec<Milestone>, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn milestone_state_parse_and_as_str_round_trip() {
        assert_eq!(MilestoneState::parse("open").unwrap(), MilestoneState::Open);
        assert_eq!(
            MilestoneState::parse("closed").unwrap(),
            MilestoneState::Closed
        );
        assert_eq!(MilestoneState::Open.as_str(), "open");
        assert_eq!(MilestoneState::Closed.as_str(), "closed");
        assert!(matches!(
            MilestoneState::parse("bogus"),
            Err(DomainError::Validation(_))
        ));
    }
}
