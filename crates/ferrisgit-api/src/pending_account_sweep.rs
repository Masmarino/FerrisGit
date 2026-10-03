use std::sync::Arc;
use std::time::Duration;

use ferrisgit_application::use_cases::sweep_pending_accounts::SweepPendingAccountsUseCase;
use tokio::sync::watch;
use tokio::task::JoinHandle;

/// How often the unactivated accounts are looked at. A reminder goes out once the previous link has expired (24 hours
/// after it was sent), so this is also how late a reminder can be.
pub const SWEEP_EVERY: Duration = Duration::from_secs(60 * 60);

/// Sweeps at once, then every `every`, one sweep at a time. Stops when `shutdown` turns true, even mid-sweep: an account
/// is deleted by one statement and a reminder only counts once it is stored, so nothing is left half done. A failed
/// sweep is logged and retried on the next tick.
pub fn spawn(
    sweep: Arc<SweepPendingAccountsUseCase>,
    every: Duration,
    mut shutdown: watch::Receiver<bool>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(every);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = interval.tick() => {}
                _ = shutdown.wait_for(|stopped| *stopped) => return,
            }
            tokio::select! {
                result = sweep.execute(chrono::Utc::now()) => match result {
                    Ok(report) if report.deleted == 0 && report.reminded == 0 => {}
                    Ok(report) => tracing::info!(
                        deleted = report.deleted,
                        reminded = report.reminded,
                        "cleaned up the accounts nobody activated"
                    ),
                    Err(e) => tracing::warn!("failed to clean up unactivated accounts: {e}"),
                },
                _ = shutdown.wait_for(|stopped| *stopped) => return,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use ferrisgit_application::mailer::Mailer;
    use ferrisgit_domain::email::{EmailPort, SmtpSettings, SmtpSettingsPort};
    use ferrisgit_domain::error::DomainError;
    use ferrisgit_domain::invitation::{
        Invitation, PendingAccount, PendingAccountPort, UserInvitationPort,
    };
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use uuid::Uuid;

    /// Counts the sweeps through the one call each of them makes, fails the first `failures`, and can keep one open.
    struct CountingPending {
        sweeps: AtomicUsize,
        failures: usize,
        hold: Mutex<Option<tokio::sync::oneshot::Receiver<()>>>,
    }

    impl CountingPending {
        fn new(failures: usize) -> Arc<Self> {
            Arc::new(Self {
                sweeps: AtomicUsize::new(0),
                failures,
                hold: Mutex::new(None),
            })
        }

        fn sweeps(&self) -> usize {
            self.sweeps.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl PendingAccountPort for CountingPending {
        async fn list(&self) -> Result<Vec<PendingAccount>, DomainError> {
            let n = self.sweeps.fetch_add(1, Ordering::SeqCst);
            let hold = self.hold.lock().unwrap().take();
            if let Some(hold) = hold {
                let _ = hold.await;
            }
            if n < self.failures {
                return Err(DomainError::Infrastructure("db down".to_string()));
            }
            Ok(Vec::new())
        }

        async fn delete(&self, _user_id: Uuid) -> Result<bool, DomainError> {
            unimplemented!()
        }
    }

    struct NoInvitations;

    #[async_trait]
    impl UserInvitationPort for NoInvitations {
        async fn replace(&self, _: Uuid, _: &str, _: DateTime<Utc>) -> Result<(), DomainError> {
            unimplemented!()
        }
        async fn renew(&self, _: Uuid, _: &str, _: DateTime<Utc>) -> Result<bool, DomainError> {
            unimplemented!()
        }
        async fn consume(&self, _: &str) -> Result<Option<Invitation>, DomainError> {
            unimplemented!()
        }
        async fn expiries(&self, _: &[Uuid]) -> Result<Vec<(Uuid, DateTime<Utc>)>, DomainError> {
            unimplemented!()
        }
    }

    struct NoMail;

    #[async_trait]
    impl EmailPort for NoMail {
        async fn send(&self, _: &str, _: &str, _: &str, _: &str) -> Result<(), DomainError> {
            unimplemented!()
        }
    }

    struct NoSmtp;

    #[async_trait]
    impl SmtpSettingsPort for NoSmtp {
        async fn get(&self) -> Result<Option<SmtpSettings>, DomainError> {
            Ok(None)
        }
        async fn save(&self, _: &SmtpSettings) -> Result<(), DomainError> {
            unimplemented!()
        }
    }

    fn use_case(pending: Arc<CountingPending>) -> Arc<SweepPendingAccountsUseCase> {
        Arc::new(SweepPendingAccountsUseCase::new(
            pending,
            Arc::new(NoInvitations),
            Arc::new(Mailer::new(Arc::new(NoMail))),
            Arc::new(NoSmtp),
            "https://git.example.com".to_string(),
        ))
    }

    async fn until(condition: impl Fn() -> bool) {
        for _ in 0..200 {
            if condition() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("condition not met in time");
    }

    #[tokio::test]
    async fn it_sweeps_at_startup_and_then_on_every_tick() {
        let pending = CountingPending::new(0);
        let (shutdown, rx) = watch::channel(false);

        let task = spawn(use_case(pending.clone()), Duration::from_millis(60), rx);

        until(|| pending.sweeps() >= 1).await;
        let after_startup = pending.sweeps();
        until(|| pending.sweeps() > after_startup).await;
        shutdown.send_replace(true);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn it_waits_for_the_next_tick_instead_of_sweeping_in_a_loop() {
        let pending = CountingPending::new(0);
        let (shutdown, rx) = watch::channel(false);

        let task = spawn(use_case(pending.clone()), Duration::from_secs(3600), rx);

        until(|| pending.sweeps() == 1).await;
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(pending.sweeps(), 1);
        shutdown.send_replace(true);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn a_failed_sweep_does_not_stop_the_task() {
        let pending = CountingPending::new(1);
        let (shutdown, rx) = watch::channel(false);

        let task = spawn(use_case(pending.clone()), Duration::from_millis(40), rx);

        until(|| pending.sweeps() >= 3).await;
        assert!(!task.is_finished(), "the loop survives an error");
        shutdown.send_replace(true);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn it_stops_when_the_server_shuts_down_while_idle() {
        let pending = CountingPending::new(0);
        let (shutdown, rx) = watch::channel(false);
        let task = spawn(use_case(pending.clone()), Duration::from_secs(3600), rx);
        until(|| pending.sweeps() == 1).await;

        shutdown.send_replace(true);

        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .expect("the task ends promptly")
            .unwrap();
    }

    #[tokio::test]
    async fn it_stops_even_in_the_middle_of_a_sweep() {
        let pending = CountingPending::new(0);
        let (release, held) = tokio::sync::oneshot::channel();
        *pending.hold.lock().unwrap() = Some(held);
        let (shutdown, rx) = watch::channel(false);
        let task = spawn(use_case(pending.clone()), Duration::from_secs(3600), rx);
        until(|| pending.sweeps() == 1).await;

        shutdown.send_replace(true);

        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .expect("a stuck sweep does not block the shutdown")
            .unwrap();
        drop(release);
    }
}
