//! A mailer that records what the server tries to send, and can be told to fail like an unreachable SMTP server.

use async_trait::async_trait;
use ferrisgit_domain::email::EmailPort;
use ferrisgit_domain::error::DomainError;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct Mail {
    pub to: String,
    pub subject: String,
    pub text: String,
    pub html: String,
}

#[derive(Default)]
pub struct RecordingEmail {
    attempted: Mutex<Vec<Mail>>,
    delivered: Mutex<Vec<Mail>>,
    failure: Mutex<Option<String>>,
    attempts: AtomicUsize,
}

impl RecordingEmail {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// A mailer whose every delivery fails with `message`.
    pub fn failing(message: &str) -> Arc<Self> {
        let mailer = Self::default();
        mailer.fail_with(message);
        Arc::new(mailer)
    }

    /// Makes every later delivery fail with `message`.
    pub fn fail_with(&self, message: &str) {
        *self.failure.lock().unwrap() = Some(message.to_string());
    }

    pub fn attempts(&self) -> usize {
        self.attempts.load(Ordering::SeqCst)
    }

    pub fn attempted(&self) -> Vec<Mail> {
        self.attempted.lock().unwrap().clone()
    }

    pub fn delivered(&self) -> Vec<Mail> {
        self.delivered.lock().unwrap().clone()
    }

    pub fn attempted_with_subject(&self, subject: &str) -> Vec<Mail> {
        self.attempted()
            .into_iter()
            .filter(|mail| mail.subject == subject)
            .collect()
    }

    /// `(recipient, subject)` of what was delivered.
    pub fn sent(&self) -> Vec<(String, String)> {
        self.delivered()
            .into_iter()
            .map(|mail| (mail.to, mail.subject))
            .collect()
    }

    /// The plain-text bodies of what was delivered.
    pub fn texts(&self) -> Vec<String> {
        self.delivered().into_iter().map(|mail| mail.text).collect()
    }

    pub async fn wait_for_subject(&self, subject: &str, count: usize) -> Vec<Mail> {
        for _ in 0..100 {
            let mails = self.attempted_with_subject(subject);
            if mails.len() >= count {
                return mails;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        panic!(
            "expected {count} mail(s) with subject {subject:?}, saw {:?}",
            self.attempted_with_subject(subject)
        );
    }

    /// Gives a background task the time it would need to (wrongly) send something, then reads the mails.
    pub async fn settled_with_subject(&self, subject: &str) -> Vec<Mail> {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        self.attempted_with_subject(subject)
    }
}

#[async_trait]
impl EmailPort for RecordingEmail {
    async fn send(
        &self,
        to: &str,
        subject: &str,
        text_body: &str,
        html_body: &str,
    ) -> Result<(), DomainError> {
        let mail = Mail {
            to: to.to_string(),
            subject: subject.to_string(),
            text: text_body.to_string(),
            html: html_body.to_string(),
        };
        self.attempted.lock().unwrap().push(mail.clone());
        self.attempts.fetch_add(1, Ordering::SeqCst);
        if let Some(message) = self.failure.lock().unwrap().clone() {
            return Err(DomainError::Infrastructure(message));
        }
        self.delivered.lock().unwrap().push(mail);
        Ok(())
    }
}

pub async fn wait_for_attempts(mailer: &RecordingEmail, count: usize) {
    for _ in 0..100 {
        if mailer.attempts() >= count {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!(
        "expected {count} delivery attempt(s), saw {}",
        mailer.attempts()
    );
}

/// Gives a background task the time it would need to (wrongly) send something, then reads the counter.
pub async fn settled_attempts(mailer: &RecordingEmail) -> usize {
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    mailer.attempts()
}
