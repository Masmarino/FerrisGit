use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::milestone::MilestoneStorePort;
use uuid::Uuid;

pub struct DeleteMilestoneUseCase {
    milestones: Arc<dyn MilestoneStorePort>,
}

impl DeleteMilestoneUseCase {
    pub fn new(milestones: Arc<dyn MilestoneStorePort>) -> Self {
        Self { milestones }
    }

    pub async fn execute(&self, milestone_id: Uuid) -> Result<(), DomainError> {
        self.milestones.delete(milestone_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeMilestones;
    use crate::use_cases::fixtures::milestone;

    #[tokio::test]
    async fn deleting_a_milestone_removes_it() {
        let seed = milestone();
        let store = Arc::new(FakeMilestones::new(vec![seed.clone()]));
        let use_case = DeleteMilestoneUseCase::new(store.clone());

        use_case.execute(seed.id).await.unwrap();

        assert!(store.get(seed.id).is_none());
    }

    #[tokio::test]
    async fn deleting_an_unknown_milestone_id_succeeds_without_error() {
        let use_case = DeleteMilestoneUseCase::new(Arc::new(FakeMilestones::empty()));

        use_case.execute(Uuid::new_v4()).await.unwrap();
    }
}
