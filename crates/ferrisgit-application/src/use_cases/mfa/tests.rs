use super::*;
use crate::mfa_crypto::{TOTP_STEP_SECS, generate_code_at, hash_backup_code, verify_backup_code};
use crate::passkey_ceremonies::PasskeyCeremonies;
use crate::test_support::{FakeBackupCodes, FakeHasher, FakePasskeys, FakeTotp, FakeUsers};
use crate::use_cases::fixtures;
use crate::use_cases::passkeys::build_webauthn;
use async_trait::async_trait;
use ferrisgit_domain::user::User;
use ferrisgit_domain::webauthn::{StoredPasskey, WebauthnCredentialPort};
use std::sync::Mutex;
use url::Url;
use webauthn_authenticator_rs::WebauthnAuthenticator;
use webauthn_authenticator_rs::softpasskey::SoftPasskey;

struct Fixture {
    service: Arc<MfaService>,
    totp: Arc<FakeTotp>,
    backup: Arc<FakeBackupCodes>,
    passkeys: Arc<FakePasskeys>,
    users: Arc<FakeUsers>,
    user_id: Uuid,
}

/// Wraps a `FakeTotp` to script the races a real database can produce between two of the service's calls.
#[derive(Default)]
struct SpyTotp {
    inner: Arc<FakeTotp>,
    /// `Some(snapshot)`: `get` returns that stale view while the inner store is already ahead.
    stale_get: Mutex<Option<Option<TotpCredential>>>,
    /// `Some(credential)`: right after a successful `set_last_used_step`, that credential is upserted
    /// (a concurrent `enroll_totp` landing between the CAS and the confirm).
    enroll_after_cas: Mutex<Option<TotpCredential>>,
    fail_delete: bool,
}

impl SpyTotp {
    fn over(inner: Arc<FakeTotp>) -> Self {
        Self {
            inner,
            ..Self::default()
        }
    }
}

#[async_trait]
impl TotpCredentialPort for SpyTotp {
    async fn get(&self, user_id: Uuid) -> Result<Option<TotpCredential>, DomainError> {
        let stale = self.stale_get.lock().unwrap().clone();
        match stale {
            Some(snapshot) => Ok(snapshot),
            None => self.inner.get(user_id).await,
        }
    }
    async fn upsert(&self, credential: &TotpCredential) -> Result<bool, DomainError> {
        self.inner.upsert(credential).await
    }
    async fn set_last_used_step(&self, user_id: Uuid, step: i64) -> Result<bool, DomainError> {
        let won = self.inner.set_last_used_step(user_id, step).await?;
        let racing_enrol = self.enroll_after_cas.lock().unwrap().take();
        if let (true, Some(credential)) = (won, racing_enrol) {
            self.inner.upsert(&credential).await?;
        }
        Ok(won)
    }
    async fn confirm(&self, user_id: Uuid, expected_step: i64) -> Result<bool, DomainError> {
        self.inner.confirm(user_id, expected_step).await
    }
    async fn delete(&self, user_id: Uuid) -> Result<(), DomainError> {
        if self.fail_delete {
            return Err(DomainError::Infrastructure("boom".to_string()));
        }
        self.inner.delete(user_id).await
    }
    async fn confirmed_user_ids(&self, user_ids: &[Uuid]) -> Result<Vec<Uuid>, DomainError> {
        self.inner.confirmed_user_ids(user_ids).await
    }
}

/// Wraps `FakePasskeys` to script what a real database could do between two of the service's calls.
#[derive(Default)]
struct SpyPasskeys {
    inner: Arc<FakePasskeys>,
    fail_delete: bool,
    /// The first `count_for_user` answers are taken from here (a stale count), then the real one.
    scripted_counts: Mutex<Vec<i64>>,
}

#[async_trait]
impl WebauthnCredentialPort for SpyPasskeys {
    async fn list_for_user(&self, user_id: Uuid) -> Result<Vec<StoredPasskey>, DomainError> {
        self.inner.list_for_user(user_id).await
    }
    async fn count_for_user(&self, user_id: Uuid) -> Result<i64, DomainError> {
        let scripted = {
            let mut counts = self.scripted_counts.lock().unwrap();
            if counts.is_empty() {
                None
            } else {
                Some(counts.remove(0))
            }
        };
        match scripted {
            Some(count) => Ok(count),
            None => self.inner.count_for_user(user_id).await,
        }
    }
    async fn insert(&self, passkey: &StoredPasskey) -> Result<bool, DomainError> {
        self.inner.insert(passkey).await
    }
    async fn update_after_authentication(
        &self,
        id: Uuid,
        passkey_json: &str,
    ) -> Result<(), DomainError> {
        self.inner
            .update_after_authentication(id, passkey_json)
            .await
    }
    async fn delete(&self, id: Uuid, user_id: Uuid) -> Result<bool, DomainError> {
        if self.fail_delete {
            return Err(DomainError::Infrastructure("boom".to_string()));
        }
        self.inner.delete(id, user_id).await
    }
    async fn delete_all_for_user(&self, user_id: Uuid) -> Result<(), DomainError> {
        self.inner.delete_all_for_user(user_id).await
    }
    async fn user_ids_with_passkeys(&self, user_ids: &[Uuid]) -> Result<Vec<Uuid>, DomainError> {
        self.inner.user_ids_with_passkeys(user_ids).await
    }
}

/// A TOTP that is confirmed for the first read only, as if it were removed by a concurrent request right after.
struct FlickerTotp {
    inner: Arc<FakeTotp>,
    reads: std::sync::atomic::AtomicUsize,
}

#[async_trait]
impl TotpCredentialPort for FlickerTotp {
    async fn get(&self, user_id: Uuid) -> Result<Option<TotpCredential>, DomainError> {
        if self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            self.inner.get(user_id).await
        } else {
            Ok(None)
        }
    }
    async fn upsert(&self, credential: &TotpCredential) -> Result<bool, DomainError> {
        self.inner.upsert(credential).await
    }
    async fn set_last_used_step(&self, user_id: Uuid, step: i64) -> Result<bool, DomainError> {
        self.inner.set_last_used_step(user_id, step).await
    }
    async fn confirm(&self, user_id: Uuid, expected_step: i64) -> Result<bool, DomainError> {
        self.inner.confirm(user_id, expected_step).await
    }
    async fn delete(&self, user_id: Uuid) -> Result<(), DomainError> {
        self.inner.delete(user_id).await
    }
    async fn confirmed_user_ids(&self, user_ids: &[Uuid]) -> Result<Vec<Uuid>, DomainError> {
        self.inner.confirmed_user_ids(user_ids).await
    }
}

struct FailingBackup(Arc<FakeBackupCodes>);

#[async_trait]
impl BackupCodePort for FailingBackup {
    async fn replace_all(
        &self,
        _user_id: Uuid,
        _code_hashes: &[String],
    ) -> Result<(), DomainError> {
        Err(DomainError::Infrastructure("boom".to_string()))
    }
    async fn try_consume(&self, user_id: Uuid, plaintext_code: &str) -> Result<bool, DomainError> {
        self.0.try_consume(user_id, plaintext_code).await
    }
    async fn count_unused(&self, user_id: Uuid) -> Result<i64, DomainError> {
        self.0.count_unused(user_id).await
    }
    async fn delete_all(&self, user_id: Uuid) -> Result<(), DomainError> {
        self.0.delete_all(user_id).await
    }
}

fn service_with_passkeys(f: &Fixture, passkeys: Arc<dyn WebauthnCredentialPort>) -> MfaService {
    MfaService::new(
        f.totp.clone(),
        f.backup.clone(),
        f.users.clone(),
        Arc::new(FakeHasher),
        passkeys,
    )
}

fn passkey_service(f: &Fixture) -> PasskeyService {
    PasskeyService::new(
        build_webauthn("http://localhost:4200").map(Arc::new),
        f.passkeys.clone(),
        Arc::new(PasskeyCeremonies::new()),
        f.users.clone(),
    )
}

async fn registration_ceremony(
    passkeys: &PasskeyService,
    user_id: Uuid,
) -> (Uuid, RegisterPublicKeyCredential) {
    let (challenge_id, challenge) = passkeys.start_registration(user_id).await.unwrap();
    let credential = WebauthnAuthenticator::new(SoftPasskey::new(true))
        .do_registration(Url::parse("http://localhost:4200").unwrap(), challenge)
        .unwrap();
    (challenge_id, credential)
}

fn service_over(f: &Fixture, totp: Arc<dyn TotpCredentialPort>) -> MfaService {
    MfaService::new(
        totp,
        f.backup.clone(),
        f.users.clone(),
        Arc::new(FakeHasher),
        f.passkeys.clone(),
    )
}

/// The password is the one `FakeHasher` accepts for `"s3cret!"`.
fn user(name: &str) -> User {
    User {
        password_hash: "hashed:s3cret!".to_string(),
        ..fixtures::user(name)
    }
}

fn fixture() -> Fixture {
    let florian = user("florian");
    let totp = Arc::new(FakeTotp::new());
    let backup = Arc::new(FakeBackupCodes::new());
    let passkeys = Arc::new(FakePasskeys::new());
    let users = Arc::new(FakeUsers::new(vec![florian.clone(), user("marie")]));
    let service = Arc::new(MfaService::new(
        totp.clone(),
        backup.clone(),
        users.clone(),
        Arc::new(FakeHasher),
        passkeys.clone(),
    ));
    Fixture {
        service,
        totp,
        backup,
        passkeys,
        users,
        user_id: florian.id,
    }
}

fn stored_secret(f: &Fixture) -> String {
    f.totp
        .credential_of(f.user_id)
        .expect("a credential")
        .secret
}

/// Enrols and confirms with the current code. Confirm consumes the current step, so the first
/// challenge must use the next step's code (`code_for_next_step`).
async fn enrolled(f: &Fixture) -> Vec<String> {
    let enrollment = f.service.enroll_totp(f.user_id).await.unwrap();
    let code = generate_code_at(&enrollment.secret_base32, now_unix());
    f.service.confirm_totp(f.user_id, &code).await.unwrap()
}

fn code_for_next_step(f: &Fixture) -> String {
    generate_code_at(&stored_secret(f), now_unix() + TOTP_STEP_SECS)
}

fn is_unauthorized<T: std::fmt::Debug>(result: Result<T, DomainError>) -> bool {
    matches!(result, Err(DomainError::Unauthorized(_)))
}

fn is_validation<T: std::fmt::Debug>(result: Result<T, DomainError>) -> bool {
    matches!(result, Err(DomainError::Validation(_)))
}

#[tokio::test]
async fn enrolling_creates_an_unconfirmed_credential_and_returns_a_secret_and_url() {
    let f = fixture();

    let enrollment = f.service.enroll_totp(f.user_id).await.unwrap();

    assert_eq!(enrollment.secret_base32.len(), 32);
    assert!(enrollment.otpauth_url.starts_with("otpauth://totp/"));
    assert!(enrollment.otpauth_url.contains("florian"));
    assert!(enrollment.otpauth_url.contains(&enrollment.secret_base32));
    let credential = f.totp.credential_of(f.user_id).unwrap();
    assert!(!credential.confirmed);
    assert_eq!(credential.last_used_step, None);
    assert_eq!(credential.secret, enrollment.secret_base32);
}

#[tokio::test]
async fn enrolling_twice_before_confirming_replaces_the_secret() {
    let f = fixture();

    let first = f.service.enroll_totp(f.user_id).await.unwrap();
    let second = f.service.enroll_totp(f.user_id).await.unwrap();

    assert_ne!(first.secret_base32, second.secret_base32);
    assert_eq!(stored_secret(&f), second.secret_base32);
}

#[tokio::test]
async fn enrolling_an_unknown_user_is_not_found() {
    let f = fixture();

    assert!(matches!(
        f.service.enroll_totp(Uuid::new_v4()).await,
        Err(DomainError::NotFound(_))
    ));
}

#[tokio::test]
async fn enrolling_when_already_confirmed_is_a_validation_error_and_keeps_the_credential() {
    let f = fixture();
    enrolled(&f).await;
    let secret = stored_secret(&f);

    assert!(is_validation(f.service.enroll_totp(f.user_id).await));
    assert_eq!(stored_secret(&f), secret);
    assert!(f.totp.credential_of(f.user_id).unwrap().confirmed);
}

#[tokio::test]
async fn confirming_with_a_valid_code_confirms_stores_ten_hashes_and_returns_verifiable_codes() {
    let f = fixture();

    let codes = enrolled(&f).await;

    assert_eq!(codes.len(), 10);
    let credential = f.totp.credential_of(f.user_id).unwrap();
    assert!(credential.confirmed);
    assert!(
        credential.last_used_step.is_some(),
        "the confirming step is spent"
    );
    let hashes = f.backup.hashes_of(f.user_id);
    assert_eq!(hashes.len(), 10);
    for code in &codes {
        assert!(
            hashes.iter().any(|hash| verify_backup_code(code, hash)),
            "{code} must verify against a stored hash"
        );
    }
    assert!(
        hashes
            .iter()
            .all(|hash| !codes.iter().any(|code| hash.contains(code.as_str()))),
        "no plaintext is stored"
    );
}

#[tokio::test]
async fn confirming_with_a_wrong_code_is_a_validation_error_and_stays_unconfirmed() {
    let f = fixture();
    let enrollment = f.service.enroll_totp(f.user_id).await.unwrap();
    // Everything the service could accept, even across a step boundary: now-1 ..= now+1, plus now+2.
    let accepted: Vec<String> = [
        now_unix() - 30,
        now_unix(),
        now_unix() + 30,
        now_unix() + 60,
    ]
    .iter()
    .map(|t| generate_code_at(&enrollment.secret_base32, *t))
    .collect();
    let wrong = (0..1000u32)
        .map(|i| format!("{:06}", i * 997 % 1_000_000))
        .find(|c| !accepted.contains(c))
        .unwrap();

    assert!(is_validation(
        f.service.confirm_totp(f.user_id, &wrong).await
    ));

    assert!(!f.totp.credential_of(f.user_id).unwrap().confirmed);
    assert_eq!(f.backup.hashes_of(f.user_id).len(), 0);
}

#[tokio::test]
async fn confirming_without_a_pending_credential_is_a_validation_error() {
    let f = fixture();

    assert!(is_validation(
        f.service.confirm_totp(f.user_id, "123456").await
    ));
}

#[tokio::test]
async fn confirming_again_when_already_confirmed_is_refused_and_keeps_the_backup_codes() {
    let f = fixture();
    let codes = enrolled(&f).await;
    let next = code_for_next_step(&f);

    assert!(is_validation(
        f.service.confirm_totp(f.user_id, &next).await
    ));

    assert!(
        f.service
            .verify_challenge(f.user_id, None, Some(&codes[0]))
            .await
            .is_ok(),
        "the original backup codes survive"
    );
}

#[tokio::test]
async fn the_code_accepted_by_confirm_cannot_be_replayed_at_the_first_challenge() {
    let f = fixture();
    let enrollment = f.service.enroll_totp(f.user_id).await.unwrap();
    let confirming = generate_code_at(&enrollment.secret_base32, now_unix());
    f.service
        .confirm_totp(f.user_id, &confirming)
        .await
        .unwrap();

    assert!(
        is_unauthorized(
            f.service
                .verify_challenge(f.user_id, Some(&confirming), None)
                .await
        ),
        "same step as confirm"
    );

    let next = generate_code_at(&enrollment.secret_base32, now_unix() + TOTP_STEP_SECS);
    assert!(
        f.service
            .verify_challenge(f.user_id, Some(&next), None)
            .await
            .is_ok(),
        "the next step passes"
    );
}

#[tokio::test]
async fn a_valid_totp_challenge_succeeds_once_and_its_replay_fails() {
    let f = fixture();
    enrolled(&f).await;
    let code = code_for_next_step(&f);

    assert!(
        f.service
            .verify_challenge(f.user_id, Some(&code), None)
            .await
            .is_ok()
    );
    assert!(is_unauthorized(
        f.service
            .verify_challenge(f.user_id, Some(&code), None)
            .await
    ));
}

#[tokio::test]
async fn a_code_from_an_older_step_than_the_last_used_one_is_refused() {
    let f = fixture();
    enrolled(&f).await;
    let next = code_for_next_step(&f);
    f.service
        .verify_challenge(f.user_id, Some(&next), None)
        .await
        .unwrap();

    // The current step is now behind `last_used_step` (= current + 1): inside the skew window, still refused.
    let current = generate_code_at(&stored_secret(&f), now_unix());
    assert!(is_unauthorized(
        f.service
            .verify_challenge(f.user_id, Some(&current), None)
            .await
    ));
}

#[tokio::test]
async fn a_previous_step_code_is_accepted_only_when_its_step_is_newer_than_the_last_used_one() {
    // The "previous step" must still be the previous one when the service reads the clock.
    if now_unix() % TOTP_STEP_SECS >= TOTP_STEP_SECS - 3 {
        tokio::time::sleep(std::time::Duration::from_secs(4)).await;
    }
    let f = fixture();
    let enrollment = f.service.enroll_totp(f.user_id).await.unwrap();
    let previous = generate_code_at(&enrollment.secret_base32, now_unix() - TOTP_STEP_SECS);
    f.service.confirm_totp(f.user_id, &previous).await.unwrap();

    assert!(is_unauthorized(
        f.service
            .verify_challenge(f.user_id, Some(&previous), None)
            .await
    ));
    let current = generate_code_at(&enrollment.secret_base32, now_unix());
    assert!(
        f.service
            .verify_challenge(f.user_id, Some(&current), None)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn a_wrong_totp_code_is_unauthorized() {
    let f = fixture();
    enrolled(&f).await;

    assert!(is_unauthorized(
        f.service
            .verify_challenge(f.user_id, Some("abcdef"), None)
            .await
    ));
    assert!(is_unauthorized(
        f.service.verify_challenge(f.user_id, Some(""), None).await
    ));
}

#[tokio::test]
async fn an_unconfirmed_credential_is_ignored_by_the_challenge() {
    let f = fixture();
    let enrollment = f.service.enroll_totp(f.user_id).await.unwrap();
    let code = generate_code_at(&enrollment.secret_base32, now_unix());

    assert!(is_unauthorized(
        f.service
            .verify_challenge(f.user_id, Some(&code), None)
            .await
    ));
}

#[tokio::test]
async fn a_user_with_no_factor_fails_the_challenge() {
    let f = fixture();

    assert!(is_unauthorized(
        f.service
            .verify_challenge(f.user_id, Some("123456"), None)
            .await
    ));
    assert!(is_unauthorized(
        f.service
            .verify_challenge(f.user_id, None, Some("0123456789abcdef0123456789abcdef"))
            .await
    ));
}

#[tokio::test]
async fn a_backup_code_works_once_then_fails() {
    let f = fixture();
    let codes = enrolled(&f).await;

    assert!(
        f.service
            .verify_challenge(f.user_id, None, Some(&codes[3]))
            .await
            .is_ok()
    );
    assert!(is_unauthorized(
        f.service
            .verify_challenge(f.user_id, None, Some(&codes[3]))
            .await
    ));
    assert!(
        f.service
            .verify_challenge(f.user_id, None, Some(&codes[4]))
            .await
            .is_ok(),
        "the others are unaffected"
    );
    assert_eq!(
        f.service
            .status(f.user_id)
            .await
            .unwrap()
            .backup_codes_remaining,
        8
    );
}

#[tokio::test]
async fn a_backup_code_typed_with_padding_or_capitals_is_normalised() {
    let f = fixture();
    let codes = enrolled(&f).await;

    assert!(
        f.service
            .verify_challenge(
                f.user_id,
                None,
                Some(&format!(" {}\n", codes[0].to_uppercase()))
            )
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn a_wrong_backup_code_is_unauthorized() {
    let f = fixture();
    enrolled(&f).await;

    assert!(is_unauthorized(
        f.service
            .verify_challenge(f.user_id, None, Some("00000000000000000000000000000000"))
            .await
    ));
}

#[tokio::test]
async fn providing_both_factors_or_neither_is_unauthorized_and_consumes_nothing() {
    let f = fixture();
    let codes = enrolled(&f).await;
    let next = code_for_next_step(&f);

    assert!(is_unauthorized(
        f.service
            .verify_challenge(f.user_id, Some(&next), Some(&codes[0]))
            .await
    ));
    assert!(is_unauthorized(
        f.service.verify_challenge(f.user_id, None, None).await
    ));

    assert_eq!(
        f.service
            .status(f.user_id)
            .await
            .unwrap()
            .backup_codes_remaining,
        10
    );
    assert!(
        f.service
            .verify_challenge(f.user_id, Some(&next), None)
            .await
            .is_ok(),
        "the TOTP step was not spent either"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_use_of_the_same_totp_step_yields_exactly_one_success() {
    let f = fixture();
    enrolled(&f).await;
    let code = code_for_next_step(&f);

    let attempts: Vec<_> = (0..16)
        .map(|_| {
            let service = f.service.clone();
            let user_id = f.user_id;
            let code = code.clone();
            tokio::spawn(async move {
                service
                    .verify_challenge(user_id, Some(&code), None)
                    .await
                    .is_ok()
            })
        })
        .collect();
    let mut winners = 0;
    for attempt in attempts {
        if attempt.await.unwrap() {
            winners += 1;
        }
    }

    assert_eq!(winners, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_use_of_the_same_backup_code_yields_exactly_one_success() {
    let f = fixture();
    let codes = enrolled(&f).await;

    let attempts: Vec<_> = (0..16)
        .map(|_| {
            let service = f.service.clone();
            let user_id = f.user_id;
            let code = codes[0].clone();
            tokio::spawn(async move {
                service
                    .verify_challenge(user_id, None, Some(&code))
                    .await
                    .is_ok()
            })
        })
        .collect();
    let mut winners = 0;
    for attempt in attempts {
        if attempt.await.unwrap() {
            winners += 1;
        }
    }

    assert_eq!(winners, 1);
}

#[tokio::test]
async fn regenerating_backup_codes_invalidates_the_old_set() {
    let f = fixture();
    let old = enrolled(&f).await;

    let new = f
        .service
        .regenerate_backup_codes(f.user_id, "s3cret!")
        .await
        .unwrap();

    assert_eq!(new.len(), 10);
    assert!(old.iter().all(|code| !new.contains(code)));
    assert!(is_unauthorized(
        f.service
            .verify_challenge(f.user_id, None, Some(&old[0]))
            .await
    ));
    assert!(
        f.service
            .verify_challenge(f.user_id, None, Some(&new[0]))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn regenerating_needs_the_right_password_and_a_confirmed_totp() {
    let f = fixture();
    assert!(
        is_validation(
            f.service
                .regenerate_backup_codes(f.user_id, "s3cret!")
                .await
        ),
        "no TOTP at all"
    );

    f.service.enroll_totp(f.user_id).await.unwrap();
    assert!(
        is_validation(
            f.service
                .regenerate_backup_codes(f.user_id, "s3cret!")
                .await
        ),
        "TOTP not confirmed yet"
    );
    assert_eq!(f.backup.hashes_of(f.user_id).len(), 0);

    let old = enrolled_after_enrol(&f).await;
    match f.service.regenerate_backup_codes(f.user_id, "wrong").await {
        Err(DomainError::Validation(message)) => {
            assert_eq!(message, "current password is incorrect")
        }
        other => panic!("expected a validation error, got {other:?}"),
    }
    assert!(
        f.service
            .verify_challenge(f.user_id, None, Some(&old[0]))
            .await
            .is_ok(),
        "the old set is untouched after a refusal"
    );
}

async fn enrolled_after_enrol(f: &Fixture) -> Vec<String> {
    let code = generate_code_at(&stored_secret(f), now_unix());
    f.service.confirm_totp(f.user_id, &code).await.unwrap()
}

#[tokio::test]
async fn check_password_accepts_the_right_one_and_reports_the_wrong_one_as_a_validation_error() {
    let f = fixture();

    f.service
        .check_password(f.user_id, "s3cret!")
        .await
        .unwrap();
    match f.service.check_password(f.user_id, "nope").await {
        Err(DomainError::Validation(message)) => {
            assert_eq!(message, "current password is incorrect")
        }
        other => panic!("expected a validation error, got {other:?}"),
    }
    assert!(matches!(
        f.service.check_password(Uuid::new_v4(), "s3cret!").await,
        Err(DomainError::NotFound(_))
    ));
}

#[tokio::test]
async fn reset_clears_everything_for_that_user_only() {
    let f = fixture();
    enrolled(&f).await;
    let other = Uuid::new_v4();
    f.totp
        .upsert(&TotpCredential {
            user_id: other,
            secret: mfa_crypto::generate_secret_base32(),
            confirmed: true,
            last_used_step: None,
            created_at: Utc::now(),
        })
        .await
        .unwrap();
    f.backup
        .replace_all(other, &[hash_backup_code("keep-me")])
        .await
        .unwrap();

    f.service.reset(f.user_id).await.unwrap();

    assert!(f.totp.credential_of(f.user_id).is_none());
    assert_eq!(f.backup.hashes_of(f.user_id).len(), 0);
    assert!(f.totp.credential_of(other).is_some());
    assert_eq!(f.backup.hashes_of(other).len(), 1);
    f.service.reset(f.user_id).await.unwrap();
}

async fn backdate_pending(f: &Fixture, age: chrono::Duration) {
    let mut credential = f
        .totp
        .credential_of(f.user_id)
        .expect("a pending credential");
    credential.created_at = Utc::now() - age;
    f.totp.upsert(&credential).await.unwrap();
}

#[tokio::test]
async fn an_abandoned_pending_enrolment_can_no_longer_be_confirmed_and_mints_no_codes() {
    // The secret was shown once, the user cancelled, and later someone who recorded it confirms with a session
    // alone.
    let f = fixture();
    let enrollment = f.service.enroll_totp(f.user_id).await.unwrap();
    backdate_pending(&f, chrono::Duration::minutes(16)).await;
    f.backup
        .replace_all(f.user_id, &[hash_backup_code("the user's own")])
        .await
        .unwrap();
    let own_codes = f.backup.hashes_of(f.user_id);

    let code = generate_code_at(&enrollment.secret_base32, now_unix());
    let result = f.service.confirm_totp(f.user_id, &code).await;

    assert!(
        matches!(&result, Err(DomainError::Validation(m)) if m == "invalid code"),
        "{result:?}"
    );
    assert!(
        f.totp
            .credential_of(f.user_id)
            .is_some_and(|c| !c.confirmed),
        "not confirmed"
    );
    assert_eq!(
        f.backup.hashes_of(f.user_id),
        own_codes,
        "the user's own codes were not replaced"
    );
    assert!(!f.service.factors(f.user_id).await.unwrap().any());
}

#[tokio::test]
async fn a_pending_enrolment_just_inside_the_lifetime_still_confirms() {
    let f = fixture();
    let enrollment = f.service.enroll_totp(f.user_id).await.unwrap();
    backdate_pending(&f, chrono::Duration::minutes(14)).await;

    let codes = f
        .service
        .confirm_totp(
            f.user_id,
            &generate_code_at(&enrollment.secret_base32, now_unix()),
        )
        .await
        .unwrap();

    assert_eq!(codes.len(), 10);
}

#[tokio::test]
async fn enrolling_again_replaces_a_stale_enrolment_with_a_fresh_secret_that_confirms() {
    let f = fixture();
    let stale = f.service.enroll_totp(f.user_id).await.unwrap();
    backdate_pending(&f, chrono::Duration::hours(3)).await;

    let fresh = f.service.enroll_totp(f.user_id).await.unwrap();

    assert_ne!(fresh.secret_base32, stale.secret_base32);
    let stored = f.totp.credential_of(f.user_id).unwrap();
    assert_eq!(stored.secret, fresh.secret_base32);
    assert!(
        Utc::now() - stored.created_at < chrono::Duration::minutes(1),
        "the clock restarted"
    );
    assert!(
        f.service
            .confirm_totp(
                f.user_id,
                &generate_code_at(&stale.secret_base32, now_unix())
            )
            .await
            .is_err()
    );
    assert_eq!(
        f.service
            .confirm_totp(
                f.user_id,
                &generate_code_at(&fresh.secret_base32, now_unix())
            )
            .await
            .unwrap()
            .len(),
        10
    );
}

#[tokio::test]
async fn the_pending_lifetime_can_be_changed() {
    let f = fixture();
    let short = MfaService::new(
        f.totp.clone(),
        f.backup.clone(),
        f.users.clone(),
        Arc::new(FakeHasher),
        f.passkeys.clone(),
    )
    .with_pending_ttl(chrono::Duration::zero());
    let enrollment = short.enroll_totp(f.user_id).await.unwrap();
    backdate_pending(&f, chrono::Duration::seconds(1)).await;

    assert!(
        short
            .confirm_totp(
                f.user_id,
                &generate_code_at(&enrollment.secret_base32, now_unix())
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn status_counts_unused_codes_and_is_zero_when_not_enrolled() {
    let f = fixture();
    assert_eq!(
        f.service.status(f.user_id).await.unwrap(),
        MfaStatus {
            totp_enabled: false,
            backup_codes_remaining: 0
        }
    );

    f.service.enroll_totp(f.user_id).await.unwrap();
    assert_eq!(
        f.service.status(f.user_id).await.unwrap(),
        MfaStatus {
            totp_enabled: false,
            backup_codes_remaining: 0
        },
        "unconfirmed"
    );

    let codes = enrolled_after_enrol(&f).await;
    assert_eq!(
        f.service.status(f.user_id).await.unwrap(),
        MfaStatus {
            totp_enabled: true,
            backup_codes_remaining: 10
        }
    );

    f.service
        .verify_challenge(f.user_id, None, Some(&codes[0]))
        .await
        .unwrap();
    assert_eq!(
        f.service.status(f.user_id).await.unwrap(),
        MfaStatus {
            totp_enabled: true,
            backup_codes_remaining: 9
        }
    );
}

#[tokio::test]
async fn status_reports_the_codes_of_a_passkey_only_user() {
    let f = fixture();
    f.passkeys.seed(f.user_id, "MacBook");
    f.passkeys.seed(f.user_id, "iPhone");
    f.passkeys.seed(Uuid::new_v4(), "someone else's");
    assert_eq!(
        f.service.status(f.user_id).await.unwrap(),
        MfaStatus {
            totp_enabled: false,
            backup_codes_remaining: 0
        }
    );

    let codes = f.service.issue_backup_codes(f.user_id).await.unwrap();

    assert_eq!(
        f.service.status(f.user_id).await.unwrap(),
        MfaStatus {
            totp_enabled: false,
            backup_codes_remaining: 10
        }
    );
    f.service
        .verify_challenge(f.user_id, None, Some(&codes[0]))
        .await
        .unwrap();
    assert_eq!(
        f.service
            .status(f.user_id)
            .await
            .unwrap()
            .backup_codes_remaining,
        9
    );
}

#[tokio::test]
async fn factors_report_has_totp_only_for_a_confirmed_credential() {
    let f = fixture();
    assert!(!f.service.factors(f.user_id).await.unwrap().has_totp);

    f.service.enroll_totp(f.user_id).await.unwrap();
    assert!(!f.service.factors(f.user_id).await.unwrap().has_totp);

    enrolled_after_enrol(&f).await;
    assert!(f.service.factors(f.user_id).await.unwrap().has_totp);
}

#[tokio::test]
async fn factors_combine_totp_and_passkeys() {
    let f = fixture();
    assert_eq!(
        f.service.factors(f.user_id).await.unwrap(),
        MfaFactors {
            has_totp: false,
            has_passkey: false
        }
    );
    assert!(
        !f.service.factors(f.user_id).await.unwrap().any(),
        "no factor"
    );

    f.service.enroll_totp(f.user_id).await.unwrap();
    assert_eq!(
        f.service.factors(f.user_id).await.unwrap(),
        MfaFactors {
            has_totp: false,
            has_passkey: false
        }
    );
    assert!(
        !f.service.factors(f.user_id).await.unwrap().any(),
        "unconfirmed TOTP only"
    );

    enrolled_after_enrol(&f).await;
    let totp_only = f.service.factors(f.user_id).await.unwrap();
    assert_eq!(
        totp_only,
        MfaFactors {
            has_totp: true,
            has_passkey: false
        }
    );
    assert!(totp_only.any());

    f.passkeys.seed(f.user_id, "MacBook");
    let both = f.service.factors(f.user_id).await.unwrap();
    assert_eq!(
        both,
        MfaFactors {
            has_totp: true,
            has_passkey: true
        }
    );
    assert!(both.any());

    f.totp.delete(f.user_id).await.unwrap();
    let passkey_only = f.service.factors(f.user_id).await.unwrap();
    assert_eq!(
        passkey_only,
        MfaFactors {
            has_totp: false,
            has_passkey: true
        }
    );
    assert!(passkey_only.any());
}

#[tokio::test]
async fn another_users_passkey_is_not_my_factor() {
    let f = fixture();
    f.passkeys.seed(Uuid::new_v4(), "someone else's");

    assert!(!f.service.factors(f.user_id).await.unwrap().any());
}

#[tokio::test]
async fn a_backup_code_is_accepted_for_a_passkey_only_user_once() {
    let f = fixture();
    f.passkeys.seed(f.user_id, "MacBook");
    let codes = f.service.issue_backup_codes(f.user_id).await.unwrap();

    assert!(
        f.service
            .verify_challenge(f.user_id, None, Some(&codes[2]))
            .await
            .is_ok()
    );
    assert!(
        is_unauthorized(
            f.service
                .verify_challenge(f.user_id, None, Some(&codes[2]))
                .await
        ),
        "single use"
    );
    assert!(
        f.service
            .verify_challenge(f.user_id, None, Some(&codes[3]))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn a_totp_code_is_refused_for_a_passkey_only_user() {
    let f = fixture();
    f.passkeys.seed(f.user_id, "MacBook");

    assert!(is_unauthorized(
        f.service
            .verify_challenge(f.user_id, Some("123456"), None)
            .await
    ));
}

#[tokio::test]
async fn issuing_backup_codes_needs_a_factor_and_replaces_the_old_set() {
    let f = fixture();
    assert!(
        is_validation(f.service.issue_backup_codes(f.user_id).await),
        "no factor at all"
    );
    assert_eq!(f.backup.hashes_of(f.user_id).len(), 0);

    // Dormant codes left behind by a removed factor are wiped by the refusal, never revived.
    f.backup
        .replace_all(f.user_id, &[hash_backup_code("dormant")])
        .await
        .unwrap();
    assert!(is_validation(f.service.issue_backup_codes(f.user_id).await));
    assert_eq!(
        f.backup.hashes_of(f.user_id).len(),
        0,
        "the dormant codes are gone"
    );

    f.service.enroll_totp(f.user_id).await.unwrap();
    assert!(
        is_validation(f.service.issue_backup_codes(f.user_id).await),
        "an unconfirmed TOTP is no factor"
    );

    f.passkeys.seed(f.user_id, "MacBook");
    let first = f.service.issue_backup_codes(f.user_id).await.unwrap();
    let second = f.service.issue_backup_codes(f.user_id).await.unwrap();

    assert_eq!((first.len(), second.len()), (10, 10));
    assert!(
        is_unauthorized(
            f.service
                .verify_challenge(f.user_id, None, Some(&first[0]))
                .await
        ),
        "the first set is dead"
    );
    assert!(
        f.service
            .verify_challenge(f.user_id, None, Some(&second[0]))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn regenerating_works_for_a_passkey_only_user_and_needs_the_password() {
    let f = fixture();
    f.passkeys.seed(f.user_id, "MacBook");
    let old = f.service.issue_backup_codes(f.user_id).await.unwrap();

    assert!(is_validation(
        f.service.regenerate_backup_codes(f.user_id, "wrong").await
    ));
    assert!(
        f.service
            .verify_challenge(f.user_id, None, Some(&old[0]))
            .await
            .is_ok(),
        "a refusal leaves the set alone"
    );

    let new = f
        .service
        .regenerate_backup_codes(f.user_id, "s3cret!")
        .await
        .unwrap();
    assert_eq!(new.len(), 10);
    assert!(is_unauthorized(
        f.service
            .verify_challenge(f.user_id, None, Some(&old[1]))
            .await
    ));
    assert!(
        f.service
            .verify_challenge(f.user_id, None, Some(&new[0]))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn removing_the_totp_while_a_passkey_remains_keeps_the_backup_codes_and_the_passkey() {
    let f = fixture();
    let codes = enrolled(&f).await;
    f.passkeys.seed(f.user_id, "MacBook");

    f.service.remove_totp(f.user_id).await.unwrap();

    assert!(f.totp.credential_of(f.user_id).is_none());
    assert_eq!(f.passkeys.of(f.user_id).len(), 1);
    assert_eq!(f.backup.hashes_of(f.user_id).len(), 10);
    assert!(
        f.service
            .verify_challenge(f.user_id, None, Some(&codes[0]))
            .await
            .is_ok(),
        "the passkey keeps the codes usable"
    );
    assert_eq!(
        f.service.factors(f.user_id).await.unwrap(),
        MfaFactors {
            has_totp: false,
            has_passkey: true
        }
    );
}

#[tokio::test]
async fn removing_the_totp_when_nothing_else_remains_deletes_the_backup_codes_too() {
    let f = fixture();
    enrolled(&f).await;

    f.service.remove_totp(f.user_id).await.unwrap();

    assert!(f.totp.credential_of(f.user_id).is_none());
    assert_eq!(f.backup.hashes_of(f.user_id).len(), 0);
    assert!(!f.service.factors(f.user_id).await.unwrap().any());
}

#[tokio::test]
async fn removing_the_totp_only_touches_that_user_and_is_idempotent() {
    let f = fixture();
    enrolled(&f).await;
    let other = Uuid::new_v4();
    f.totp
        .upsert(&TotpCredential {
            user_id: other,
            secret: mfa_crypto::generate_secret_base32(),
            confirmed: true,
            last_used_step: None,
            created_at: Utc::now(),
        })
        .await
        .unwrap();
    f.backup
        .replace_all(other, &[hash_backup_code("keep-me")])
        .await
        .unwrap();

    f.service.remove_totp(f.user_id).await.unwrap();
    f.service.remove_totp(f.user_id).await.unwrap();

    assert!(f.totp.credential_of(other).is_some());
    assert_eq!(f.backup.hashes_of(other).len(), 1);
}

#[tokio::test]
async fn removing_the_last_factors_codes_go_before_the_credential() {
    let f = fixture();
    enrolled(&f).await;
    let spy = SpyTotp {
        inner: f.totp.clone(),
        fail_delete: true,
        ..SpyTotp::default()
    };

    let result = service_over(&f, Arc::new(spy)).remove_totp(f.user_id).await;

    assert!(result.is_err(), "the credential delete failed");
    assert_eq!(
        f.backup.hashes_of(f.user_id).len(),
        0,
        "no live codes are left behind a half-done removal"
    );
}

#[tokio::test]
async fn reset_removes_the_passkeys_of_that_user_too() {
    let f = fixture();
    enrolled(&f).await;
    f.passkeys.seed(f.user_id, "MacBook");
    f.passkeys.seed(f.user_id, "iPhone");
    let other = Uuid::new_v4();
    f.passkeys.seed(other, "keep-me");

    f.service.reset(f.user_id).await.unwrap();

    assert!(f.passkeys.of(f.user_id).is_empty());
    assert_eq!(
        f.passkeys.of(other).len(),
        1,
        "another user's passkey survives"
    );
    assert!(f.totp.credential_of(f.user_id).is_none());
    assert_eq!(f.backup.hashes_of(f.user_id).len(), 0);
    assert!(!f.service.factors(f.user_id).await.unwrap().any());
}

#[tokio::test]
async fn reset_of_a_passkey_only_user_works() {
    let f = fixture();
    f.passkeys.seed(f.user_id, "MacBook");
    f.service.issue_backup_codes(f.user_id).await.unwrap();

    f.service.reset(f.user_id).await.unwrap();

    assert!(f.passkeys.of(f.user_id).is_empty());
    assert_eq!(f.backup.hashes_of(f.user_id).len(), 0);
}

#[test]
fn debug_output_of_an_enrolment_never_contains_the_secret_or_the_url() {
    let enrollment = TotpEnrollment {
        secret_base32: "TOTPSEEDVALUE".to_string(),
        otpauth_url: "otpauth://totp/FerrisGit:x?secret=TOTPSEEDVALUE".to_string(),
    };

    let debug = format!("{enrollment:?}");
    let wrapped = format!("{:?}", Ok::<_, DomainError>(enrollment));

    for text in [&debug, &wrapped] {
        assert!(!text.contains("TOTPSEEDVALUE"), "{text}");
        assert!(!text.contains("secret="), "{text}");
        assert!(text.contains("[redacted]"), "{text}");
    }
}

#[tokio::test]
async fn removing_the_last_passkey_of_a_passkey_only_user_deletes_the_backup_codes() {
    let f = fixture();
    let passkey = f.passkeys.seed(f.user_id, "MacBook");
    let codes = f.service.issue_backup_codes(f.user_id).await.unwrap();

    f.service
        .remove_passkey(f.user_id, passkey.id)
        .await
        .unwrap();

    assert!(f.passkeys.of(f.user_id).is_empty());
    assert_eq!(
        f.backup.hashes_of(f.user_id).len(),
        0,
        "no codes are left to be revived by the next passkey"
    );
    assert!(!f.service.factors(f.user_id).await.unwrap().any());
    f.passkeys.seed(f.user_id, "New key");
    assert!(is_unauthorized(
        f.service
            .verify_challenge(f.user_id, None, Some(&codes[0]))
            .await
    ));
    assert_eq!(
        f.service
            .status(f.user_id)
            .await
            .unwrap()
            .backup_codes_remaining,
        0
    );
}

#[tokio::test]
async fn removing_one_of_two_passkeys_keeps_the_other_and_the_backup_codes() {
    let f = fixture();
    let first = f.passkeys.seed(f.user_id, "MacBook");
    let second = f.passkeys.seed(f.user_id, "YubiKey");
    let codes = f.service.issue_backup_codes(f.user_id).await.unwrap();

    f.service.remove_passkey(f.user_id, first.id).await.unwrap();

    assert_eq!(f.passkeys.of(f.user_id), vec![second]);
    assert_eq!(f.backup.hashes_of(f.user_id).len(), 10);
    assert!(
        f.service
            .verify_challenge(f.user_id, None, Some(&codes[0]))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn removing_the_last_passkey_while_a_totp_remains_keeps_the_backup_codes() {
    let f = fixture();
    let codes = enrolled(&f).await;
    let passkey = f.passkeys.seed(f.user_id, "MacBook");

    f.service
        .remove_passkey(f.user_id, passkey.id)
        .await
        .unwrap();

    assert!(f.passkeys.of(f.user_id).is_empty());
    assert_eq!(f.backup.hashes_of(f.user_id).len(), 10);
    assert!(
        f.service
            .verify_challenge(f.user_id, None, Some(&codes[0]))
            .await
            .is_ok(),
        "the TOTP keeps the codes meaningful"
    );
    assert_eq!(
        f.service.factors(f.user_id).await.unwrap(),
        MfaFactors {
            has_totp: true,
            has_passkey: false
        }
    );
}

#[tokio::test]
async fn an_unconfirmed_totp_does_not_keep_the_codes_alive() {
    let f = fixture();
    f.service.enroll_totp(f.user_id).await.unwrap();
    let passkey = f.passkeys.seed(f.user_id, "MacBook");
    f.service.issue_backup_codes(f.user_id).await.unwrap();

    f.service
        .remove_passkey(f.user_id, passkey.id)
        .await
        .unwrap();

    assert_eq!(f.backup.hashes_of(f.user_id).len(), 0);
}

#[tokio::test]
async fn removing_a_foreign_or_unknown_passkey_is_not_found_and_touches_nothing() {
    let f = fixture();
    let mine = f.passkeys.seed(f.user_id, "MacBook");
    f.service.issue_backup_codes(f.user_id).await.unwrap();
    let maries = f.passkeys.seed(Uuid::new_v4(), "Marie's key");
    f.backup
        .replace_all(maries.user_id, &[hash_backup_code("marie")])
        .await
        .unwrap();

    assert!(matches!(
        f.service.remove_passkey(f.user_id, maries.id).await,
        Err(DomainError::NotFound(_))
    ));
    assert!(matches!(
        f.service.remove_passkey(f.user_id, Uuid::new_v4()).await,
        Err(DomainError::NotFound(_))
    ));
    assert!(matches!(
        f.service.remove_passkey(maries.user_id, mine.id).await,
        Err(DomainError::NotFound(_))
    ));

    assert_eq!(f.passkeys.of(f.user_id), vec![mine], "mine is still there");
    assert_eq!(f.passkeys.of(maries.user_id), vec![maries.clone()]);
    assert_eq!(
        f.backup.hashes_of(f.user_id).len(),
        10,
        "my codes were not touched by a refused removal"
    );
    assert_eq!(f.backup.hashes_of(maries.user_id).len(), 1, "nor were hers");
}

#[tokio::test]
async fn removing_the_last_passkey_deletes_the_codes_before_the_passkey() {
    let f = fixture();
    let passkey = f.passkeys.seed(f.user_id, "MacBook");
    f.service.issue_backup_codes(f.user_id).await.unwrap();
    let spy = Arc::new(SpyPasskeys {
        inner: f.passkeys.clone(),
        fail_delete: true,
        ..SpyPasskeys::default()
    });

    let result = service_with_passkeys(&f, spy)
        .remove_passkey(f.user_id, passkey.id)
        .await;

    assert!(result.is_err(), "the passkey delete failed");
    assert_eq!(
        f.backup.hashes_of(f.user_id).len(),
        0,
        "a partial failure never leaves live codes behind a removed factor"
    );
}

#[tokio::test]
async fn a_totp_removed_concurrently_does_not_leave_codes_behind_the_last_passkey() {
    let f = fixture();
    enrolled(&f).await;
    let passkey = f.passkeys.seed(f.user_id, "MacBook");
    // The passkey removal reads the TOTP as still there (codes kept); a parallel removal deletes it right after.
    let flicker = Arc::new(FlickerTotp {
        inner: f.totp.clone(),
        reads: std::sync::atomic::AtomicUsize::new(0),
    });
    let service = MfaService::new(
        flicker,
        f.backup.clone(),
        f.users.clone(),
        Arc::new(FakeHasher),
        f.passkeys.clone(),
    );

    service.remove_passkey(f.user_id, passkey.id).await.unwrap();

    assert_eq!(
        f.backup.hashes_of(f.user_id).len(),
        0,
        "the re-check after the delete found no factor left"
    );
}

#[tokio::test]
async fn a_passkey_removed_concurrently_does_not_leave_codes_behind_the_totp() {
    let f = fixture();
    enrolled(&f).await;
    // remove_totp counts the passkeys: a stale "1" (kept the codes), then the truth: none.
    let spy = Arc::new(SpyPasskeys {
        inner: f.passkeys.clone(),
        scripted_counts: Mutex::new(vec![1]),
        ..SpyPasskeys::default()
    });

    service_with_passkeys(&f, spy)
        .remove_totp(f.user_id)
        .await
        .unwrap();

    assert!(f.totp.credential_of(f.user_id).is_none());
    assert_eq!(f.backup.hashes_of(f.user_id).len(), 0);
}

#[tokio::test]
async fn the_first_passkey_setup_registers_the_passkey_and_issues_ten_working_codes() {
    let f = fixture();
    let passkeys = passkey_service(&f);
    let (challenge_id, credential) = registration_ceremony(&passkeys, f.user_id).await;

    let (stored, codes) = f
        .service
        .finish_first_passkey_setup(&passkeys, f.user_id, challenge_id, &credential, "MacBook")
        .await
        .unwrap();

    assert_eq!(
        (stored.user_id, stored.name.as_str()),
        (f.user_id, "MacBook")
    );
    assert_eq!(f.passkeys.of(f.user_id), vec![stored]);
    assert_eq!(codes.len(), 10);
    assert_eq!(
        f.service.factors(f.user_id).await.unwrap(),
        MfaFactors {
            has_totp: false,
            has_passkey: true
        }
    );
    assert!(
        f.service
            .verify_challenge(f.user_id, None, Some(&codes[0]))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn the_first_passkey_setup_is_refused_when_a_totp_already_exists_and_changes_nothing() {
    let f = fixture();
    let totp_codes = enrolled(&f).await;
    let passkeys = passkey_service(&f);
    let (challenge_id, credential) = registration_ceremony(&passkeys, f.user_id).await;

    let result = f
        .service
        .finish_first_passkey_setup(&passkeys, f.user_id, challenge_id, &credential, "MacBook")
        .await;

    assert!(is_validation(result));
    assert!(
        f.passkeys.of(f.user_id).is_empty(),
        "nothing was registered"
    );
    assert_eq!(
        f.backup.hashes_of(f.user_id).len(),
        10,
        "the existing codes were neither replaced nor deleted"
    );
    assert!(
        f.service
            .verify_challenge(f.user_id, None, Some(&totp_codes[0]))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn the_first_passkey_setup_is_refused_when_a_passkey_already_exists_and_burns_no_ceremony() {
    let f = fixture();
    let existing = f.passkeys.seed(f.user_id, "MacBook");
    let old_codes = f.service.issue_backup_codes(f.user_id).await.unwrap();
    let passkeys = passkey_service(&f);
    let (challenge_id, credential) = registration_ceremony(&passkeys, f.user_id).await;

    let result = f
        .service
        .finish_first_passkey_setup(&passkeys, f.user_id, challenge_id, &credential, "Second")
        .await;

    assert!(is_validation(result));
    assert_eq!(f.passkeys.of(f.user_id), vec![existing.clone()]);
    assert!(
        f.service
            .verify_challenge(f.user_id, None, Some(&old_codes[0]))
            .await
            .is_ok(),
        "no fresh codes were minted, the old set is intact"
    );
    // The refusal came before the ceremony was taken: it is still there for its owner.
    f.passkeys.delete(existing.id, f.user_id).await.unwrap();
    assert!(
        passkeys
            .finish_registration(f.user_id, challenge_id, &credential, "Second")
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn a_failed_first_passkey_ceremony_issues_no_codes() {
    let f = fixture();
    let passkeys = passkey_service(&f);
    let (_, credential) = registration_ceremony(&passkeys, f.user_id).await;

    let result = f
        .service
        .finish_first_passkey_setup(&passkeys, f.user_id, Uuid::new_v4(), &credential, "MacBook")
        .await;

    assert!(is_validation(result));
    assert!(f.passkeys.of(f.user_id).is_empty());
    assert_eq!(f.backup.hashes_of(f.user_id).len(), 0);
    assert!(!f.service.factors(f.user_id).await.unwrap().any());
}

#[tokio::test]
async fn the_first_passkey_setup_deletes_a_pending_totp_enrolment_so_that_nobody_can_confirm_it_later()
 {
    let f = fixture();
    let enrollment = f.service.enroll_totp(f.user_id).await.unwrap();
    let passkeys = passkey_service(&f);
    let (challenge_id, credential) = registration_ceremony(&passkeys, f.user_id).await;

    f.service
        .finish_first_passkey_setup(&passkeys, f.user_id, challenge_id, &credential, "MacBook")
        .await
        .unwrap();

    assert!(
        f.totp.credential_of(f.user_id).is_none(),
        "the unconfirmed row is gone"
    );
    let code = mfa_crypto::generate_code_at(&enrollment.secret_base32, now_unix());
    let confirm = f.service.confirm_totp(f.user_id, &code).await;
    assert!(
        matches!(confirm, Err(DomainError::Validation(_))),
        "the old secret can no longer be confirmed: {confirm:?}"
    );
    assert_eq!(
        f.service.factors(f.user_id).await.unwrap(),
        MfaFactors {
            has_totp: false,
            has_passkey: true
        }
    );
}

#[tokio::test]
async fn a_refused_first_passkey_setup_keeps_a_confirmed_totp_untouched() {
    let f = fixture();
    enrolled(&f).await;
    let passkeys = passkey_service(&f);
    let (challenge_id, credential) = registration_ceremony(&passkeys, f.user_id).await;

    assert!(is_validation(
        f.service
            .finish_first_passkey_setup(&passkeys, f.user_id, challenge_id, &credential, "MacBook")
            .await
    ));

    assert!(
        f.totp.credential_of(f.user_id).is_some_and(|c| c.confirmed),
        "the confirmed TOTP was not deleted by the refusal"
    );
}

#[tokio::test]
async fn the_first_passkey_setup_kills_dormant_codes_of_a_removed_factor() {
    let f = fixture();
    f.backup
        .replace_all(f.user_id, &[hash_backup_code("dormant")])
        .await
        .unwrap();
    let passkeys = passkey_service(&f);
    let (challenge_id, credential) = registration_ceremony(&passkeys, f.user_id).await;

    let (_, codes) = f
        .service
        .finish_first_passkey_setup(&passkeys, f.user_id, challenge_id, &credential, "MacBook")
        .await
        .unwrap();

    assert_eq!(
        f.backup.hashes_of(f.user_id).len(),
        10,
        "exactly the new set"
    );
    assert!(codes.iter().all(|code| code != "dormant"));
}

#[tokio::test]
async fn if_issuing_the_codes_fails_the_new_passkey_is_rolled_back() {
    let f = fixture();
    let passkeys = passkey_service(&f);
    let (challenge_id, credential) = registration_ceremony(&passkeys, f.user_id).await;
    let service = MfaService::new(
        f.totp.clone(),
        Arc::new(FailingBackup(f.backup.clone())),
        f.users.clone(),
        Arc::new(FakeHasher),
        f.passkeys.clone(),
    );

    let result = service
        .finish_first_passkey_setup(&passkeys, f.user_id, challenge_id, &credential, "MacBook")
        .await;

    assert!(matches!(result, Err(DomainError::Infrastructure(_))));
    assert!(
        f.passkeys.of(f.user_id).is_empty(),
        "the user is still in setup, not stranded with a passkey and no codes"
    );
}

#[tokio::test]
async fn the_first_passkey_setup_of_an_unavailable_client_is_service_unavailable() {
    let f = fixture();
    let unavailable = PasskeyService::new(
        None,
        f.passkeys.clone(),
        Arc::new(PasskeyCeremonies::new()),
        f.users.clone(),
    );
    let (_, credential) = registration_ceremony(&passkey_service(&f), f.user_id).await;

    let result = f
        .service
        .finish_first_passkey_setup(
            &unavailable,
            f.user_id,
            Uuid::new_v4(),
            &credential,
            "MacBook",
        )
        .await;

    assert!(matches!(result, Err(DomainError::ServiceUnavailable(_))));
    assert_eq!(f.backup.hashes_of(f.user_id).len(), 0);
}

#[tokio::test]
async fn enrolling_is_refused_when_the_upsert_finds_a_credential_confirmed_in_between() {
    let f = fixture();
    enrolled(&f).await;
    let confirmed = f.totp.credential_of(f.user_id).unwrap();
    // The has-confirmed check reads a stale "nothing there"; only the upsert's own refusal can catch it.
    let spy = SpyTotp::over(f.totp.clone());
    *spy.stale_get.lock().unwrap() = Some(None);

    let result = service_over(&f, Arc::new(spy)).enroll_totp(f.user_id).await;

    assert!(is_validation(result));
    assert_eq!(
        f.totp.credential_of(f.user_id).unwrap(),
        confirmed,
        "the confirmed credential is untouched"
    );
}

#[tokio::test]
async fn a_concurrent_enrol_between_verify_and_confirm_cannot_get_an_unscanned_secret_confirmed() {
    let f = fixture();
    let scanned = f.service.enroll_totp(f.user_id).await.unwrap();
    let code = generate_code_at(&scanned.secret_base32, now_unix());
    let unscanned = TotpCredential {
        user_id: f.user_id,
        secret: mfa_crypto::generate_secret_base32(),
        confirmed: false,
        last_used_step: None,
        created_at: Utc::now(),
    };
    let spy = SpyTotp::over(f.totp.clone());
    *spy.enroll_after_cas.lock().unwrap() = Some(unscanned.clone());

    let result = service_over(&f, Arc::new(spy))
        .confirm_totp(f.user_id, &code)
        .await;

    assert!(is_validation(result), "no codes are handed out");
    assert_eq!(
        f.totp.credential_of(f.user_id).unwrap(),
        unscanned,
        "the racing enrolment stays pending, unconfirmed"
    );
    assert_eq!(
        f.backup.hashes_of(f.user_id).len(),
        0,
        "and no backup codes were written"
    );
}

#[tokio::test]
async fn a_second_confirm_on_a_stale_view_never_gets_codes_and_leaves_the_first_set_intact() {
    let f = fixture();
    let enrollment = f.service.enroll_totp(f.user_id).await.unwrap();
    let pending = f.totp.credential_of(f.user_id).unwrap();
    let first = generate_code_at(&enrollment.secret_base32, now_unix());
    let first_codes = f.service.confirm_totp(f.user_id, &first).await.unwrap();
    let spy = SpyTotp::over(f.totp.clone());
    *spy.stale_get.lock().unwrap() = Some(Some(pending));
    let second = generate_code_at(&enrollment.secret_base32, now_unix() + TOTP_STEP_SECS);

    let result = service_over(&f, Arc::new(spy))
        .confirm_totp(f.user_id, &second)
        .await;

    assert!(is_validation(result));
    assert!(
        f.service
            .verify_challenge(f.user_id, None, Some(&first_codes[0]))
            .await
            .is_ok(),
        "the first response's codes are still the stored ones"
    );
    assert_eq!(
        f.service
            .status(f.user_id)
            .await
            .unwrap()
            .backup_codes_remaining,
        9
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_confirms_with_different_steps_never_both_return_codes() {
    let f = fixture();
    let enrollment = f.service.enroll_totp(f.user_id).await.unwrap();
    let codes = [
        generate_code_at(&enrollment.secret_base32, now_unix() - TOTP_STEP_SECS),
        generate_code_at(&enrollment.secret_base32, now_unix()),
    ];

    let attempts: Vec<_> = (0..16)
        .map(|i| {
            let service = f.service.clone();
            let user_id = f.user_id;
            let code = codes[i % 2].clone();
            tokio::spawn(async move { service.confirm_totp(user_id, &code).await })
        })
        .collect();
    let mut winners = Vec::new();
    for attempt in attempts {
        if let Ok(issued) = attempt.await.unwrap() {
            winners.push(issued);
        }
    }

    assert_eq!(winners.len(), 1, "exactly one confirm hands out codes");
    let hashes = f.backup.hashes_of(f.user_id);
    assert_eq!(hashes.len(), 10);
    assert!(
        winners[0]
            .iter()
            .all(|code| hashes.iter().any(|hash| verify_backup_code(code, hash))),
        "the returned codes are the stored ones"
    );
}

#[tokio::test]
async fn verify_challenge_honours_a_lost_cas_even_when_the_read_was_stale() {
    let f = fixture();
    enrolled(&f).await;
    let stale = f.totp.credential_of(f.user_id).unwrap();
    assert!(stale.confirmed);
    let code = code_for_next_step(&f);
    f.service
        .verify_challenge(f.user_id, Some(&code), None)
        .await
        .unwrap();
    // The second attempt read the credential before the first advanced the step: only the CAS can refuse it.
    let spy = SpyTotp::over(f.totp.clone());
    *spy.stale_get.lock().unwrap() = Some(Some(stale));

    let result = service_over(&f, Arc::new(spy))
        .verify_challenge(f.user_id, Some(&code), None)
        .await;

    assert!(is_unauthorized(result));
}

#[tokio::test]
async fn confirm_totp_honours_a_lost_cas_even_when_the_read_was_stale() {
    let f = fixture();
    let enrollment = f.service.enroll_totp(f.user_id).await.unwrap();
    let pending = f.totp.credential_of(f.user_id).unwrap();
    let code = generate_code_at(&enrollment.secret_base32, now_unix());
    assert!(
        f.totp
            .set_last_used_step(f.user_id, (now_unix() / TOTP_STEP_SECS) as i64)
            .await
            .unwrap()
    );
    let spy = SpyTotp::over(f.totp.clone());
    *spy.stale_get.lock().unwrap() = Some(Some(pending));

    let result = service_over(&f, Arc::new(spy))
        .confirm_totp(f.user_id, &code)
        .await;

    assert!(is_validation(result));
    assert!(!f.totp.credential_of(f.user_id).unwrap().confirmed);
    assert_eq!(f.backup.hashes_of(f.user_id).len(), 0);
}

#[tokio::test]
async fn a_backup_code_is_refused_when_the_credential_is_gone_but_codes_remain() {
    let f = fixture();
    let codes = enrolled(&f).await;
    f.totp.delete(f.user_id).await.unwrap();
    assert_eq!(
        f.backup.hashes_of(f.user_id).len(),
        10,
        "the codes survived"
    );

    assert!(is_unauthorized(
        f.service
            .verify_challenge(f.user_id, None, Some(&codes[0]))
            .await
    ));
    assert_eq!(
        f.backup.count_unused(f.user_id).await.unwrap(),
        10,
        "and none was consumed"
    );
}

#[tokio::test]
async fn a_backup_code_is_refused_while_the_credential_is_only_pending() {
    let f = fixture();
    let codes = enrolled(&f).await;
    // The store refuses to overwrite a confirmed row, so recreate a pending one from scratch.
    f.totp.delete(f.user_id).await.unwrap();
    f.totp
        .upsert(&TotpCredential {
            user_id: f.user_id,
            secret: mfa_crypto::generate_secret_base32(),
            confirmed: false,
            last_used_step: None,
            created_at: Utc::now(),
        })
        .await
        .unwrap();

    assert!(is_unauthorized(
        f.service
            .verify_challenge(f.user_id, None, Some(&codes[0]))
            .await
    ));
}

#[tokio::test]
async fn reset_deletes_the_backup_codes_before_the_credential() {
    let f = fixture();
    enrolled(&f).await;
    let spy = SpyTotp {
        inner: f.totp.clone(),
        fail_delete: true,
        ..SpyTotp::default()
    };

    let result = service_over(&f, Arc::new(spy)).reset(f.user_id).await;

    assert!(result.is_err(), "the credential delete failed");
    assert_eq!(
        f.backup.hashes_of(f.user_id).len(),
        0,
        "the codes were already gone: a partial failure never leaves live codes behind"
    );
}
