use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::label::{Label, LabelStorePort, NewLabel};
use uuid::Uuid;

pub struct CreateLabelUseCase {
    labels: Arc<dyn LabelStorePort>,
}

impl CreateLabelUseCase {
    pub fn new(labels: Arc<dyn LabelStorePort>) -> Self {
        Self { labels }
    }

    pub async fn execute(
        &self,
        name: String,
        color: String,
        repository_id: Option<Uuid>,
        group_id: Option<Uuid>,
    ) -> Result<Label, DomainError> {
        self.labels
            .create(NewLabel {
                name,
                color,
                repository_id,
                group_id,
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeLabels;

    #[tokio::test]
    async fn creating_a_repository_scoped_label_stores_it() {
        let repository_id = Uuid::new_v4();
        let use_case = CreateLabelUseCase::new(Arc::new(FakeLabels::empty()));

        let label = use_case
            .execute(
                "Bug".to_string(),
                "#dc2626".to_string(),
                Some(repository_id),
                None,
            )
            .await
            .unwrap();

        assert_eq!(label.name, "Bug");
        assert_eq!(label.color, "#dc2626");
        assert_eq!(label.repository_id, Some(repository_id));
        assert_eq!(label.group_id, None);
    }

    #[tokio::test]
    async fn creating_a_group_scoped_label_stores_it() {
        let group_id = Uuid::new_v4();
        let use_case = CreateLabelUseCase::new(Arc::new(FakeLabels::empty()));

        let label = use_case
            .execute(
                "Feature".to_string(),
                "#16a34a".to_string(),
                None,
                Some(group_id),
            )
            .await
            .unwrap();

        assert_eq!(label.repository_id, None);
        assert_eq!(label.group_id, Some(group_id));
    }
}
