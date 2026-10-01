use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::label::LabelStorePort;
use uuid::Uuid;

pub struct DeleteLabelUseCase {
    labels: Arc<dyn LabelStorePort>,
}

impl DeleteLabelUseCase {
    pub fn new(labels: Arc<dyn LabelStorePort>) -> Self {
        Self { labels }
    }

    pub async fn execute(&self, label_id: Uuid) -> Result<(), DomainError> {
        self.labels.delete(label_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeLabels;
    use chrono::Utc;
    use ferrisgit_domain::label::Label;

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
    async fn deleting_a_label_removes_it() {
        let seed = label();
        let store = Arc::new(FakeLabels::new(vec![seed.clone()]));
        let use_case = DeleteLabelUseCase::new(store.clone());

        use_case.execute(seed.id).await.unwrap();

        assert!(store.find_by_id(seed.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn deleting_an_unknown_label_id_succeeds_without_error() {
        // Mirrors the Postgres adapter: a DELETE matching zero rows still succeeds, since
        // deleting something already gone is not a failure worth surfacing.
        let use_case = DeleteLabelUseCase::new(Arc::new(FakeLabels::empty()));

        use_case.execute(Uuid::new_v4()).await.unwrap();
    }
}
