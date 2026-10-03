use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use ferrisgit_domain::email::SmtpSettingsPort;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::invitation::{PendingAccount, PendingAccountPort, UserInvitationPort};

use crate::account_rules::{
    INVITATION_TTL_HOURS, PENDING_ACCOUNT_RETENTION_DAYS, generate_invitation_token,
};
use crate::email_templates;
use crate::mailer::Mailer;
use crate::token_hash::hash_token;

/// What one sweep did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SweepReport {
    pub deleted: usize,
    pub reminded: usize,
    /// Accounts the sweep could not handle (mail refused, database error). They are tried again on the next sweep.
    pub failed: usize,
}

/// Cleans up the accounts nobody activated: one older than the retention is deleted, and one whose last link has
/// expired gets a reminder with a new link. The sweep runs often, so a reminder goes out about once a day. Without
/// mail configured there are no reminders (an admin hands the links over by hand), but the deletion still happens.
pub struct SweepPendingAccountsUseCase {
    pending: Arc<dyn PendingAccountPort>,
    invitations: Arc<dyn UserInvitationPort>,
    mailer: Arc<Mailer>,
    smtp: Arc<dyn SmtpSettingsPort>,
    public_url: String,
}

impl SweepPendingAccountsUseCase {
    pub fn new(
        pending: Arc<dyn PendingAccountPort>,
        invitations: Arc<dyn UserInvitationPort>,
        mailer: Arc<Mailer>,
        smtp: Arc<dyn SmtpSettingsPort>,
        public_url: String,
    ) -> Self {
        Self {
            pending,
            invitations,
            mailer,
            smtp,
            public_url,
        }
    }

    pub async fn execute(&self, now: DateTime<Utc>) -> Result<SweepReport, DomainError> {
        let mut report = SweepReport::default();
        // Read once, and only if a reminder is due.
        let mut mail_configured: Option<bool> = None;
        for account in self.pending.list().await? {
            let deletion = account.created_at + Duration::days(PENDING_ACCOUNT_RETENTION_DAYS);
            if now >= deletion {
                match self.pending.delete(account.user_id).await {
                    Ok(true) => report.deleted += 1,
                    // Activated since the list, so it is an ordinary account now.
                    Ok(false) => {}
                    Err(error) => {
                        tracing::warn!(user_id = %account.user_id, %error, "could not delete an unactivated account");
                        report.failed += 1;
                    }
                }
            } else if account.link_expires_at <= now {
                let can_mail = match mail_configured {
                    Some(configured) => configured,
                    None => {
                        let configured = self.smtp.get().await?.is_some();
                        mail_configured = Some(configured);
                        configured
                    }
                };
                if !can_mail {
                    continue;
                }
                match self.remind(&account, deletion, now).await {
                    Ok(true) => report.reminded += 1,
                    Ok(false) => {}
                    Err(error) => {
                        tracing::warn!(user_id = %account.user_id, %error, "could not send an activation reminder");
                        report.failed += 1;
                    }
                }
            }
        }
        Ok(report)
    }

    /// Sends the new link before storing it: if the mail can't go out, the stored link is left alone and the next sweep
    /// tries again, instead of invalidating a link the person never got a replacement for. `false` means the account
    /// was activated in between.
    async fn remind(
        &self,
        account: &PendingAccount,
        deletion: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<bool, DomainError> {
        let token = generate_invitation_token();
        let activation_url = format!("{}/activate#token={}", self.public_url, token);
        let deletion_date = deletion.format("%d/%m/%Y").to_string();
        self.mailer
            .send(
                &account.email,
                email_templates::activation_reminder(
                    &account.username,
                    &activation_url,
                    &deletion_date,
                ),
            )
            .await?;
        self.invitations
            .renew(
                account.user_id,
                &hash_token(&token),
                now + Duration::hours(INVITATION_TTL_HOURS),
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeEmail, FakeInvitations, FakePendingAccounts, FakeSmtpSettings};
    use chrono::TimeZone;
    use uuid::Uuid;

    const PUBLIC_URL: &str = "https://git.example.com";

    struct Fixture {
        pending: Arc<FakePendingAccounts>,
        invitations: Arc<FakeInvitations>,
        email: Arc<FakeEmail>,
        use_case: SweepPendingAccountsUseCase,
    }

    fn day(n: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 1, 9, 0, 0).unwrap() + Duration::days(n)
    }

    /// An account created at `day(0)`, whose first link expires 24 hours later.
    fn account(username: &str) -> PendingAccount {
        PendingAccount {
            user_id: Uuid::new_v4(),
            username: username.to_string(),
            email: format!("{username}@example.com"),
            created_at: day(0),
            link_expires_at: day(0) + Duration::hours(INVITATION_TTL_HOURS),
        }
    }

    fn fixture(accounts: Vec<PendingAccount>) -> Fixture {
        let invitations = Arc::new(FakeInvitations::new());
        for a in &accounts {
            invitations.insert(a.user_id, "first-link-hash", a.link_expires_at);
        }
        let pending = Arc::new(FakePendingAccounts::new(accounts, invitations.clone()));
        let email = Arc::new(FakeEmail::new());
        let use_case = SweepPendingAccountsUseCase::new(
            pending.clone(),
            invitations.clone(),
            Arc::new(Mailer::new(email.clone())),
            Arc::new(FakeSmtpSettings::configured()),
            PUBLIC_URL.to_string(),
        );
        Fixture {
            pending,
            invitations,
            email,
            use_case,
        }
    }

    #[tokio::test]
    async fn an_account_whose_link_is_still_valid_is_left_alone() {
        let a = account("alice");
        let f = fixture(vec![a.clone()]);

        let report = f
            .use_case
            .execute(day(0) + Duration::hours(23))
            .await
            .unwrap();

        assert_eq!(report, SweepReport::default());
        assert!(f.email.sent().is_empty());
        assert!(f.pending.deleted().is_empty());
        assert_eq!(
            f.invitations.row_of(a.user_id).unwrap().0,
            "first-link-hash"
        );
    }

    #[tokio::test]
    async fn once_the_link_has_expired_a_reminder_with_a_new_link_goes_out() {
        let a = account("alice");
        let f = fixture(vec![a.clone()]);
        let now = day(1) + Duration::minutes(5);

        let report = f.use_case.execute(now).await.unwrap();

        assert_eq!(report.reminded, 1);
        let sent = f.email.sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, "alice@example.com");
        assert_eq!(sent[0].1, "Votre compte FerrisGit n'est pas encore activé");
        let link = sent[0]
            .2
            .split_whitespace()
            .find(|w| w.starts_with(PUBLIC_URL))
            .unwrap();
        let token = link.split_once("#token=").unwrap().1;
        let (stored_hash, expires_at) = f.invitations.row_of(a.user_id).unwrap();
        assert_eq!(
            stored_hash,
            hash_token(token),
            "the mailed link is the stored one"
        );
        assert_eq!(expires_at, now + Duration::hours(INVITATION_TTL_HOURS));
        assert!(
            sent[0].2.contains("08/10/2026"),
            "the account goes on day 7: {}",
            sent[0].2
        );
    }

    #[tokio::test]
    async fn a_reminder_follows_every_expired_link_until_the_account_is_deleted() {
        let a = account("alice");
        let f = fixture(vec![a.clone()]);

        // The sweep runs every hour, a little after each expiry.
        let mut reminders = 0;
        for hour in 1..(7 * 24) {
            let now = day(0) + Duration::hours(hour) + Duration::minutes(1);
            let report = f.use_case.execute(now).await.unwrap();
            reminders += report.reminded;
            assert_eq!(report.deleted, 0, "still within seven days at hour {hour}");
        }

        assert_eq!(reminders, 6, "one a day, from the first to the sixth");
        let report = f.use_case.execute(day(7)).await.unwrap();
        assert_eq!(report.deleted, 1);
        assert_eq!(f.pending.deleted(), vec![a.user_id]);
    }

    #[tokio::test]
    async fn two_sweeps_in_a_row_send_a_single_reminder() {
        let a = account("alice");
        let f = fixture(vec![a.clone()]);
        let now = day(1) + Duration::minutes(5);

        f.use_case.execute(now).await.unwrap();
        let second = f
            .use_case
            .execute(now + Duration::minutes(30))
            .await
            .unwrap();

        assert_eq!(second, SweepReport::default());
        assert_eq!(f.email.sent().len(), 1);
    }

    #[tokio::test]
    async fn an_account_is_deleted_at_seven_days_and_not_a_minute_before() {
        let a = account("alice");
        let f = fixture(vec![a.clone()]);

        let before = f
            .use_case
            .execute(day(7) - Duration::minutes(1))
            .await
            .unwrap();
        assert_eq!(before.deleted, 0);
        assert!(f.pending.deleted().is_empty());

        let at = f.use_case.execute(day(7)).await.unwrap();
        assert_eq!(at.deleted, 1);
        assert_eq!(f.pending.deleted(), vec![a.user_id]);
    }

    #[tokio::test]
    async fn an_account_past_the_limit_is_deleted_without_a_last_reminder() {
        let a = account("alice");
        let f = fixture(vec![a.clone()]);

        let report = f.use_case.execute(day(30)).await.unwrap();

        assert_eq!(report.deleted, 1);
        assert_eq!(report.reminded, 0);
        assert!(f.email.sent().is_empty());
    }

    #[tokio::test]
    async fn a_refused_reminder_keeps_the_stored_link_and_is_retried() {
        let a = account("alice");
        let f = fixture(vec![a.clone()]);
        let now = day(1) + Duration::minutes(5);
        f.email.refuse_everything();

        let report = f.use_case.execute(now).await.unwrap();

        assert_eq!(report.failed, 1);
        assert_eq!(report.reminded, 0);
        assert_eq!(
            f.invitations.row_of(a.user_id).unwrap().0,
            "first-link-hash",
            "nothing was invalidated"
        );

        f.email.recover();
        let retry = f.use_case.execute(now + Duration::hours(1)).await.unwrap();
        assert_eq!(retry.reminded, 1);
        assert_eq!(f.email.sent().len(), 1);
    }

    #[tokio::test]
    async fn one_failure_does_not_stop_the_rest_of_the_sweep() {
        let stuck = account("stuck");
        let other = account("other");
        let f = fixture(vec![stuck.clone(), other.clone()]);
        f.pending.refuse_to_delete(stuck.user_id);

        let report = f.use_case.execute(day(8)).await.unwrap();

        assert_eq!(report.failed, 1);
        assert_eq!(report.deleted, 1);
        assert_eq!(f.pending.deleted(), vec![other.user_id]);
    }

    #[tokio::test]
    async fn an_account_activated_before_its_deletion_is_not_counted_or_touched() {
        let a = account("alice");
        let f = fixture(vec![a.clone()]);
        f.pending.activate_before_delete(a.user_id);

        let report = f.use_case.execute(day(8)).await.unwrap();

        assert_eq!(report, SweepReport::default());
        assert!(f.pending.deleted().is_empty());
    }

    #[tokio::test]
    async fn an_account_activated_before_the_sweep_is_not_listed_and_gets_nothing() {
        let a = account("alice");
        let f = fixture(vec![a.clone()]);
        f.invitations.remove(a.user_id);

        let report = f
            .use_case
            .execute(day(1) + Duration::minutes(5))
            .await
            .unwrap();

        assert_eq!(report, SweepReport::default());
        assert!(
            f.invitations.row_of(a.user_id).is_none(),
            "no invitation was revived"
        );
    }

    #[tokio::test]
    async fn without_mail_configured_nobody_is_reminded_but_old_accounts_are_still_deleted() {
        let due = account("due");
        let old = PendingAccount {
            created_at: day(-10),
            link_expires_at: day(-9),
            ..account("old")
        };
        let invitations = Arc::new(FakeInvitations::new());
        for a in [&due, &old] {
            invitations.insert(a.user_id, "first-link-hash", a.link_expires_at);
        }
        let pending = Arc::new(FakePendingAccounts::new(
            vec![due.clone(), old.clone()],
            invitations.clone(),
        ));
        let email = Arc::new(FakeEmail::new());
        let use_case = SweepPendingAccountsUseCase::new(
            pending.clone(),
            invitations.clone(),
            Arc::new(Mailer::new(email.clone())),
            Arc::new(FakeSmtpSettings::unconfigured()),
            PUBLIC_URL.to_string(),
        );

        let report = use_case
            .execute(day(1) + Duration::minutes(5))
            .await
            .unwrap();

        assert_eq!(report.deleted, 1);
        assert_eq!(report.reminded, 0);
        assert_eq!(report.failed, 0, "not trying is not failing");
        assert!(email.sent().is_empty());
        assert_eq!(pending.deleted(), vec![old.user_id]);
        assert_eq!(
            invitations.row_of(due.user_id).unwrap().0,
            "first-link-hash",
            "the link in place is untouched"
        );
    }

    #[tokio::test]
    async fn accounts_are_handled_independently() {
        let due = account("due");
        let young = PendingAccount {
            created_at: day(1),
            link_expires_at: day(2),
            ..account("young")
        };
        let old = PendingAccount {
            created_at: day(-10),
            link_expires_at: day(-9),
            ..account("old")
        };
        let f = fixture(vec![due, young, old.clone()]);

        let report = f
            .use_case
            .execute(day(1) + Duration::minutes(5))
            .await
            .unwrap();

        assert_eq!(report.deleted, 1);
        assert_eq!(report.reminded, 1);
        assert_eq!(f.pending.deleted(), vec![old.user_id]);
        assert_eq!(f.email.sent()[0].0, "due@example.com");
    }
}
