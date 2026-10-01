use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::notification::NotificationStorePort;
use uuid::Uuid;

pub struct MarkAllNotificationsReadUseCase {
    notifications: Arc<dyn NotificationStorePort>,
}

impl MarkAllNotificationsReadUseCase {
    pub fn new(notifications: Arc<dyn NotificationStorePort>) -> Self {
        Self { notifications }
    }

    pub async fn execute(&self, recipient_id: Uuid) -> Result<(), DomainError> {
        self.notifications.mark_all_read(recipient_id).await
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
    async fn marking_all_read_clears_every_unread_notification_for_that_recipient() {
        let recipient_id = Uuid::new_v4();
        let store = Arc::new(FakeNotifications::new(vec![
            notification(recipient_id),
            notification(recipient_id),
        ]));
        let use_case = MarkAllNotificationsReadUseCase::new(store.clone());

        use_case.execute(recipient_id).await.unwrap();

        assert_eq!(store.unread_count(recipient_id).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn marking_all_read_does_not_affect_another_recipients_notifications() {
        let recipient_id = Uuid::new_v4();
        let other_id = Uuid::new_v4();
        let store = Arc::new(FakeNotifications::new(vec![
            notification(recipient_id),
            notification(other_id),
        ]));
        let use_case = MarkAllNotificationsReadUseCase::new(store.clone());

        use_case.execute(recipient_id).await.unwrap();

        assert_eq!(store.unread_count(other_id).await.unwrap(), 1);
    }
}
