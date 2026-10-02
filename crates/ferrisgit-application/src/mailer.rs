use std::sync::Arc;

use ferrisgit_domain::email::EmailPort;
use ferrisgit_domain::error::DomainError;

use crate::email_templates::EmailContent;

/// Sends the notification mails. User-facing actions use `send_in_background`, which delivers on its own task so a slow
/// or unreachable SMTP server never holds up the action. A mail that can't be delivered, with no SMTP configured or the
/// server down, is logged and never fails anything. `send_best_effort` does the same without the extra task.
#[derive(Clone)]
pub struct Mailer {
    email: Arc<dyn EmailPort>,
}

impl Mailer {
    pub fn new(email: Arc<dyn EmailPort>) -> Self {
        Self { email }
    }

    pub async fn send(&self, to: &str, content: EmailContent) -> Result<(), DomainError> {
        self.email
            .send(to, &content.subject, &content.text, &content.html)
            .await
    }

    pub async fn send_best_effort(&self, to: &str, content: EmailContent) {
        let subject = content.subject.clone();
        if let Err(error) = self.send(to, content).await {
            tracing::warn!(%error, subject = %subject, "could not send notification e-mail");
        }
    }

    /// Returns at once, delivery and logging happen on a spawned task. Callers drop the handle, tests await it.
    pub fn send_in_background(
        &self,
        to: String,
        content: EmailContent,
    ) -> tokio::task::JoinHandle<()> {
        let mailer = self.clone();
        tokio::spawn(async move { mailer.send_best_effort(&to, content).await })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;

    struct FakeEmail {
        sent: Mutex<Vec<(String, String, String, String)>>,
        attempts: AtomicUsize,
        fail: bool,
    }

    impl FakeEmail {
        fn new(fail: bool) -> Arc<Self> {
            Arc::new(Self {
                sent: Mutex::new(Vec::new()),
                attempts: AtomicUsize::new(0),
                fail,
            })
        }
    }

    #[async_trait]
    impl EmailPort for FakeEmail {
        async fn send(
            &self,
            to: &str,
            subject: &str,
            text_body: &str,
            html_body: &str,
        ) -> Result<(), DomainError> {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                return Err(DomainError::Infrastructure("smtp down".to_string()));
            }
            self.sent.lock().unwrap().push((
                to.to_string(),
                subject.to_string(),
                text_body.to_string(),
                html_body.to_string(),
            ));
            Ok(())
        }
    }

    fn content() -> EmailContent {
        EmailContent {
            subject: "Sujet".to_string(),
            text: "texte".to_string(),
            html: "<p>html</p>".to_string(),
        }
    }

    #[tokio::test]
    async fn send_forwards_recipient_subject_and_both_bodies() {
        let fake = FakeEmail::new(false);
        Mailer::new(fake.clone())
            .send("a@example.com", content())
            .await
            .unwrap();
        let sent = fake.sent.lock().unwrap();
        assert_eq!(
            sent.as_slice(),
            &[(
                "a@example.com".to_string(),
                "Sujet".to_string(),
                "texte".to_string(),
                "<p>html</p>".to_string()
            )]
        );
    }

    #[tokio::test]
    async fn send_surfaces_a_delivery_failure() {
        let fake = FakeEmail::new(true);
        let result = Mailer::new(fake).send("a@example.com", content()).await;
        assert!(matches!(result, Err(DomainError::Infrastructure(_))));
    }

    #[tokio::test]
    async fn send_best_effort_swallows_a_delivery_failure() {
        let fake = FakeEmail::new(true);
        Mailer::new(fake)
            .send_best_effort("a@example.com", content())
            .await;
    }

    #[tokio::test]
    async fn send_best_effort_still_delivers_on_success() {
        let fake = FakeEmail::new(false);
        Mailer::new(fake.clone())
            .send_best_effort("a@example.com", content())
            .await;
        assert_eq!(fake.sent.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn send_in_background_delivers_without_the_caller_waiting() {
        let fake = FakeEmail::new(false);
        let handle =
            Mailer::new(fake.clone()).send_in_background("a@example.com".to_string(), content());
        handle.await.unwrap();
        assert_eq!(fake.attempts.load(Ordering::SeqCst), 1);
        assert_eq!(fake.sent.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn send_in_background_swallows_a_delivery_failure() {
        let fake = FakeEmail::new(true);
        let handle =
            Mailer::new(fake.clone()).send_in_background("a@example.com".to_string(), content());
        assert!(
            handle.await.is_ok(),
            "the task must not panic on a failed delivery"
        );
        assert_eq!(fake.attempts.load(Ordering::SeqCst), 1);
        assert!(fake.sent.lock().unwrap().is_empty());
    }
}
