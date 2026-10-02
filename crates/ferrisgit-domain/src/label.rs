use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::DomainError;

#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    pub id: Uuid,
    pub name: String,
    pub color: String,
    pub repository_id: Option<Uuid>,
    pub group_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

pub struct NewLabel {
    pub name: String,
    pub color: String,
    pub repository_id: Option<Uuid>,
    pub group_id: Option<Uuid>,
}

/// Scoped to exactly one of `repository_id`/`group_id` (the store rejects anything else). Assignment-time scope
/// validation lives in the application use cases.
#[async_trait]
pub trait LabelStorePort: Send + Sync {
    async fn create(&self, new_label: NewLabel) -> Result<Label, DomainError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Label>, DomainError>;
    async fn update(&self, id: Uuid, name: String, color: String) -> Result<(), DomainError>;
    async fn delete(&self, id: Uuid) -> Result<(), DomainError>;
    async fn list_for_repository(&self, repository_id: Uuid) -> Result<Vec<Label>, DomainError>;
    async fn list_for_group(&self, group_id: Uuid) -> Result<Vec<Label>, DomainError>;

    async fn set_labels_for_issue(
        &self,
        issue_id: Uuid,
        label_ids: &[Uuid],
    ) -> Result<(), DomainError>;
    async fn list_for_issue(&self, issue_id: Uuid) -> Result<Vec<Label>, DomainError>;
    /// Batched variant of `list_for_issue`, for rendering label chips on a list of
    /// issues without one query per row. Each returned pair is `(issue_id, label)`.
    async fn list_for_issues(&self, issue_ids: &[Uuid]) -> Result<Vec<(Uuid, Label)>, DomainError>;

    async fn set_labels_for_merge_request(
        &self,
        merge_request_id: Uuid,
        label_ids: &[Uuid],
    ) -> Result<(), DomainError>;
    async fn list_for_merge_request(
        &self,
        merge_request_id: Uuid,
    ) -> Result<Vec<Label>, DomainError>;
    async fn list_for_merge_requests(
        &self,
        merge_request_ids: &[Uuid],
    ) -> Result<Vec<(Uuid, Label)>, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::Mutex;

    struct FakeLabels {
        labels: Mutex<Vec<Label>>,
        issue_links: Mutex<Vec<(Uuid, Uuid)>>,
    }

    #[async_trait]
    impl LabelStorePort for FakeLabels {
        async fn create(&self, new_label: NewLabel) -> Result<Label, DomainError> {
            let label = Label {
                id: Uuid::new_v4(),
                name: new_label.name,
                color: new_label.color,
                repository_id: new_label.repository_id,
                group_id: new_label.group_id,
                created_at: Utc::now(),
            };
            self.labels.lock().unwrap().push(label.clone());
            Ok(label)
        }
        async fn find_by_id(&self, id: Uuid) -> Result<Option<Label>, DomainError> {
            Ok(self
                .labels
                .lock()
                .unwrap()
                .iter()
                .find(|l| l.id == id)
                .cloned())
        }
        async fn update(&self, id: Uuid, name: String, color: String) -> Result<(), DomainError> {
            let mut labels = self.labels.lock().unwrap();
            let Some(label) = labels.iter_mut().find(|l| l.id == id) else {
                return Err(DomainError::NotFound("label".to_string()));
            };
            label.name = name;
            label.color = color;
            Ok(())
        }
        async fn delete(&self, id: Uuid) -> Result<(), DomainError> {
            self.labels.lock().unwrap().retain(|l| l.id != id);
            self.issue_links
                .lock()
                .unwrap()
                .retain(|(_, label_id)| *label_id != id);
            Ok(())
        }
        async fn list_for_repository(
            &self,
            repository_id: Uuid,
        ) -> Result<Vec<Label>, DomainError> {
            Ok(self
                .labels
                .lock()
                .unwrap()
                .iter()
                .filter(|l| l.repository_id == Some(repository_id))
                .cloned()
                .collect())
        }
        async fn list_for_group(&self, group_id: Uuid) -> Result<Vec<Label>, DomainError> {
            Ok(self
                .labels
                .lock()
                .unwrap()
                .iter()
                .filter(|l| l.group_id == Some(group_id))
                .cloned()
                .collect())
        }
        async fn set_labels_for_issue(
            &self,
            issue_id: Uuid,
            label_ids: &[Uuid],
        ) -> Result<(), DomainError> {
            let mut links = self.issue_links.lock().unwrap();
            links.retain(|(i, _)| *i != issue_id);
            links.extend(label_ids.iter().map(|label_id| (issue_id, *label_id)));
            Ok(())
        }
        async fn list_for_issue(&self, issue_id: Uuid) -> Result<Vec<Label>, DomainError> {
            let links = self.issue_links.lock().unwrap();
            let labels = self.labels.lock().unwrap();
            Ok(links
                .iter()
                .filter(|(i, _)| *i == issue_id)
                .filter_map(|(_, label_id)| labels.iter().find(|l| l.id == *label_id).cloned())
                .collect())
        }
        async fn list_for_issues(
            &self,
            issue_ids: &[Uuid],
        ) -> Result<Vec<(Uuid, Label)>, DomainError> {
            let links = self.issue_links.lock().unwrap();
            let labels = self.labels.lock().unwrap();
            Ok(links
                .iter()
                .filter(|(i, _)| issue_ids.contains(i))
                .filter_map(|(i, label_id)| {
                    labels
                        .iter()
                        .find(|l| l.id == *label_id)
                        .map(|l| (*i, l.clone()))
                })
                .collect())
        }
        async fn set_labels_for_merge_request(
            &self,
            _merge_request_id: Uuid,
            _label_ids: &[Uuid],
        ) -> Result<(), DomainError> {
            unimplemented!("not exercised by this fake's tests")
        }
        async fn list_for_merge_request(
            &self,
            _merge_request_id: Uuid,
        ) -> Result<Vec<Label>, DomainError> {
            unimplemented!("not exercised by this fake's tests")
        }
        async fn list_for_merge_requests(
            &self,
            _merge_request_ids: &[Uuid],
        ) -> Result<Vec<(Uuid, Label)>, DomainError> {
            unimplemented!("not exercised by this fake's tests")
        }
    }

    #[tokio::test]
    async fn setting_an_issues_labels_replaces_the_previous_set() {
        let store: Arc<dyn LabelStorePort> = Arc::new(FakeLabels {
            labels: Mutex::new(vec![]),
            issue_links: Mutex::new(vec![]),
        });
        let repository_id = Uuid::new_v4();
        let bug = store
            .create(NewLabel {
                name: "Bug".to_string(),
                color: "#dc2626".to_string(),
                repository_id: Some(repository_id),
                group_id: None,
            })
            .await
            .unwrap();
        let feature = store
            .create(NewLabel {
                name: "Feature".to_string(),
                color: "#16a34a".to_string(),
                repository_id: Some(repository_id),
                group_id: None,
            })
            .await
            .unwrap();
        let issue_id = Uuid::new_v4();

        store
            .set_labels_for_issue(issue_id, &[bug.id, feature.id])
            .await
            .unwrap();
        assert_eq!(store.list_for_issue(issue_id).await.unwrap().len(), 2);

        store
            .set_labels_for_issue(issue_id, &[bug.id])
            .await
            .unwrap();
        let labels = store.list_for_issue(issue_id).await.unwrap();
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].id, bug.id);
    }

    #[tokio::test]
    async fn deleting_a_label_removes_it_from_every_issue() {
        let store: Arc<dyn LabelStorePort> = Arc::new(FakeLabels {
            labels: Mutex::new(vec![]),
            issue_links: Mutex::new(vec![]),
        });
        let repository_id = Uuid::new_v4();
        let bug = store
            .create(NewLabel {
                name: "Bug".to_string(),
                color: "#dc2626".to_string(),
                repository_id: Some(repository_id),
                group_id: None,
            })
            .await
            .unwrap();
        let issue_id = Uuid::new_v4();
        store
            .set_labels_for_issue(issue_id, &[bug.id])
            .await
            .unwrap();

        store.delete(bug.id).await.unwrap();

        assert!(store.list_for_issue(issue_id).await.unwrap().is_empty());
        assert!(store.find_by_id(bug.id).await.unwrap().is_none());
    }
}
