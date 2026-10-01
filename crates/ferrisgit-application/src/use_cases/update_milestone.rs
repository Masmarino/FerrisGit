use std::sync::Arc;

use chrono::{DateTime, Utc};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::milestone::{Milestone, MilestoneState, MilestoneStorePort};
use uuid::Uuid;

pub struct UpdateMilestoneUseCase {
    milestones: Arc<dyn MilestoneStorePort>,
}

impl UpdateMilestoneUseCase {
    pub fn new(milestones: Arc<dyn MilestoneStorePort>) -> Self {
        Self { milestones }
    }

    pub async fn execute(
        &self,
        milestone_id: Uuid,
        title: String,
        description: String,
        due_date: Option<DateTime<Utc>>,
        state: MilestoneState,
    ) -> Result<Milestone, DomainError> {
        self.milestones
            .update(milestone_id, title, description, due_date, state)
            .await?;
        self.milestones
            .find_by_id(milestone_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("milestone".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeMilestones;

    fn milestone() -> Milestone {
        Milestone {
            id: Uuid::new_v4(),
            title: "v1.0".to_string(),
            description: String::new(),
            due_date: None,
            state: MilestoneState::Open,
            repository_id: Some(Uuid::new_v4()),
            group_id: None,
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn updating_a_milestone_changes_its_fields_and_closes_it() {
        let seed = milestone();
        let due_date = Utc::now();
        let use_case =
            UpdateMilestoneUseCase::new(Arc::new(FakeMilestones::new(vec![seed.clone()])));

        let updated = use_case
            .execute(
                seed.id,
                "v1.0-final".to_string(),
                "Final release".to_string(),
                Some(due_date),
                MilestoneState::Closed,
            )
            .await
            .unwrap();

        assert_eq!(updated.title, "v1.0-final");
        assert_eq!(updated.description, "Final release");
        assert_eq!(updated.due_date, Some(due_date));
        assert_eq!(updated.state, MilestoneState::Closed);
    }

    #[tokio::test]
    async fn updating_an_unknown_milestone_id_is_a_not_found_error() {
        let use_case = UpdateMilestoneUseCase::new(Arc::new(FakeMilestones::empty()));

        let result = use_case
            .execute(
                Uuid::new_v4(),
                "x".to_string(),
                String::new(),
                None,
                MilestoneState::Open,
            )
            .await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }
}
