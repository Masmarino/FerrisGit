use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::notification::NotificationStorePort;
use uuid::Uuid;

pub struct MarkNotificationReadUseCase {
    notifications: Arc<dyn NotificationStorePort>,
}

impl MarkNotificationReadUseCase {
    pub fn new(notifications: Arc<dyn NotificationStorePort>) -> Self {
        Self { notifications }
    }

    pub async fn execute(
        &self,
        notification_id: Uuid,
        recipient_id: Uuid,
    ) -> Result<(), DomainError> {
        self.notifications
            .mark_read(notification_id, recipient_id)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeNotifications;
    use chrono::Utc;
    use ferrisgit_domain::notification::{Notification, NotificationKind};

    fn notification(recipient_id: Uuid) -> Notification {
        Notification {
            id: Uuid::new_v4(),
            recipient_id,
            kind: NotificationKind::CollaboratorAdded,
            repository_owner: "owner".to_string(),
            repository_name: "hello".to_string(),
            actor_username: Some("actor".to_string()),
            merge_request_id: None,
            merge_request_title: None,
            pipeline_id: None,
            commit_sha: None,
            issue_id: None,
            issue_number: None,
            issue_title: None,
            role: None,
            read_at: None,
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn marking_a_notification_read_clears_it_from_the_unread_count() {
        let recipient_id = Uuid::new_v4();
        let seed = notification(recipient_id);
        let store = Arc::new(FakeNotifications::new(vec![seed.clone()]));
        let use_case = MarkNotificationReadUseCase::new(store.clone());

        use_case.execute(seed.id, recipient_id).await.unwrap();

        assert_eq!(store.unread_count(recipient_id).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn marking_someone_elses_notification_read_has_no_effect() {
        let recipient_id = Uuid::new_v4();
        let stranger_id = Uuid::new_v4();
        let seed = notification(recipient_id);
        let store = Arc::new(FakeNotifications::new(vec![seed.clone()]));
        let use_case = MarkNotificationReadUseCase::new(store.clone());

        use_case.execute(seed.id, stranger_id).await.unwrap();

        assert_eq!(
            store.unread_count(recipient_id).await.unwrap(),
            1,
            "a notification must not be markable read by anyone other than its own recipient"
        );
    }

    #[tokio::test]
    async fn marking_an_unknown_notification_id_succeeds_without_error() {
        let use_case = MarkNotificationReadUseCase::new(Arc::new(FakeNotifications::empty()));

        use_case
            .execute(Uuid::new_v4(), Uuid::new_v4())
            .await
            .unwrap();
    }
}
