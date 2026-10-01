use std::sync::Arc;

use chrono::{DateTime, Utc};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::milestone::{Milestone, MilestoneStorePort, NewMilestone};
use uuid::Uuid;

pub struct CreateMilestoneUseCase {
    milestones: Arc<dyn MilestoneStorePort>,
}

impl CreateMilestoneUseCase {
    pub fn new(milestones: Arc<dyn MilestoneStorePort>) -> Self {
        Self { milestones }
    }

    pub async fn execute(
        &self,
        title: String,
        description: String,
        due_date: Option<DateTime<Utc>>,
        repository_id: Option<Uuid>,
        group_id: Option<Uuid>,
    ) -> Result<Milestone, DomainError> {
        self.milestones
            .create(NewMilestone {
                title,
                description,
                due_date,
                repository_id,
                group_id,
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeMilestones;
    use ferrisgit_domain::milestone::MilestoneState;

    #[tokio::test]
    async fn creating_a_repository_scoped_milestone_stores_it_as_open() {
        let repository_id = Uuid::new_v4();
        let use_case = CreateMilestoneUseCase::new(Arc::new(FakeMilestones::empty()));

        let milestone = use_case
            .execute(
                "v1.0".to_string(),
                "First release".to_string(),
                None,
                Some(repository_id),
                None,
            )
            .await
            .unwrap();

        assert_eq!(milestone.title, "v1.0");
        assert_eq!(milestone.state, MilestoneState::Open);
        assert_eq!(milestone.repository_id, Some(repository_id));
    }

    #[tokio::test]
    async fn creating_a_group_scoped_milestone_with_a_due_date_stores_it() {
        let group_id = Uuid::new_v4();
        let due_date = Utc::now();
        let use_case = CreateMilestoneUseCase::new(Arc::new(FakeMilestones::empty()));

        let milestone = use_case
            .execute(
                "v2.0".to_string(),
                String::new(),
                Some(due_date),
                None,
                Some(group_id),
            )
            .await
            .unwrap();

        assert_eq!(milestone.due_date, Some(due_date));
        assert_eq!(milestone.group_id, Some(group_id));
    }
}
