use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::label::{Label, LabelStorePort};
use uuid::Uuid;

pub struct UpdateLabelUseCase {
    labels: Arc<dyn LabelStorePort>,
}

impl UpdateLabelUseCase {
    pub fn new(labels: Arc<dyn LabelStorePort>) -> Self {
        Self { labels }
    }

    pub async fn execute(
        &self,
        label_id: Uuid,
        name: String,
        color: String,
    ) -> Result<Label, DomainError> {
        self.labels.update(label_id, name, color).await?;
        self.labels
            .find_by_id(label_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("label".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeLabels;
    use chrono::Utc;

    fn label() -> Label {
        Label {
            id: Uuid::new_v4(),
            name: "Bug".to_string(),
            color: "#dc2626".to_string(),
            repository_id: Some(Uuid::new_v4()),
            group_id: None,
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn updating_a_label_changes_its_name_and_color() {
        let seed = label();
        let use_case = UpdateLabelUseCase::new(Arc::new(FakeLabels::new(vec![seed.clone()])));

        let updated = use_case
            .execute(seed.id, "Critical Bug".to_string(), "#7f1d1d".to_string())
            .await
            .unwrap();

        assert_eq!(updated.name, "Critical Bug");
        assert_eq!(updated.color, "#7f1d1d");
    }

    #[tokio::test]
    async fn updating_an_unknown_label_id_is_a_not_found_error() {
        let use_case = UpdateLabelUseCase::new(Arc::new(FakeLabels::empty()));

        let result = use_case
            .execute(Uuid::new_v4(), "x".to_string(), "#000000".to_string())
            .await;

        assert!(matches!(result, Err(DomainError::NotFound(_))));
    }
}
