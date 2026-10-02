use std::sync::Arc;

use chrono::{Duration, Utc};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::webauthn::{StoredPasskey, WebauthnCredentialPort};
use url::Url;
use uuid::Uuid;
use webauthn_authenticator_rs::WebauthnAuthenticator;
use webauthn_authenticator_rs::softpasskey::SoftPasskey;

use super::*;
use crate::passkey_ceremonies::PasskeyCeremonies;
use crate::test_support::{FakePasskeys, FakeUsers};
use crate::use_cases::fixtures::user;

const ORIGIN: &str = "http://localhost:4200";

struct Fixture {
    service: Arc<PasskeyService>,
    users: Arc<FakeUsers>,
    passkeys: Arc<FakePasskeys>,
    ceremonies: Arc<PasskeyCeremonies>,
    florian: Uuid,
    marie: Uuid,
}

fn fixture_with(webauthn: Option<Arc<Webauthn>>, ceremonies: PasskeyCeremonies) -> Fixture {
    let florian = user("florian");
    let marie = user("marie");
    let (florian_id, marie_id) = (florian.id, marie.id);
    let passkeys = Arc::new(FakePasskeys::new());
    let ceremonies = Arc::new(ceremonies);
    let users = Arc::new(FakeUsers::new(vec![florian, marie]));
    let service = Arc::new(PasskeyService::new(
        webauthn,
        passkeys.clone(),
        ceremonies.clone(),
        users.clone(),
    ));
    Fixture {
        service,
        users,
        passkeys,
        ceremonies,
        florian: florian_id,
        marie: marie_id,
    }
}

fn expiring(f: &Fixture) -> PasskeyService {
    PasskeyService::new(
        build_webauthn(ORIGIN).map(Arc::new),
        f.passkeys.clone(),
        Arc::new(PasskeyCeremonies::with_limits(Duration::zero(), 100, 5)),
        f.users.clone(),
    )
}

fn fixture() -> Fixture {
    fixture_with(
        build_webauthn(ORIGIN).map(Arc::new),
        PasskeyCeremonies::new(),
    )
}

fn origin() -> Url {
    Url::parse(ORIGIN).unwrap()
}

fn authenticator() -> WebauthnAuthenticator<SoftPasskey> {
    WebauthnAuthenticator::new(SoftPasskey::new(true))
}

async fn register(
    f: &Fixture,
    user_id: Uuid,
    authenticator: &mut WebauthnAuthenticator<SoftPasskey>,
    name: &str,
) -> StoredPasskey {
    let (challenge_id, challenge) = f.service.start_registration(user_id).await.unwrap();
    let credential = authenticator.do_registration(origin(), challenge).unwrap();
    f.service
        .finish_registration(user_id, challenge_id, &credential, name)
        .await
        .unwrap()
}

async fn authenticate(
    f: &Fixture,
    user_id: Uuid,
    authenticator: &mut WebauthnAuthenticator<SoftPasskey>,
) -> Result<(), DomainError> {
    let (challenge_id, challenge) = f.service.start_authentication(user_id).await.unwrap();
    let credential = authenticator
        .do_authentication(origin(), challenge)
        .unwrap();
    f.service
        .finish_authentication(user_id, challenge_id, &credential)
        .await
}

fn is_invalid_code<T: std::fmt::Debug>(result: Result<T, DomainError>) -> bool {
    matches!(&result, Err(DomainError::Unauthorized(message)) if message == "invalid code")
}

fn is_invalid_passkey<T: std::fmt::Debug>(result: Result<T, DomainError>) -> bool {
    matches!(&result, Err(DomainError::Validation(message)) if message == "invalid passkey")
}

fn counter_of(stored: &StoredPasskey) -> u64 {
    let json: serde_json::Value = serde_json::from_str(&stored.passkey_json).unwrap();
    json["cred"]["counter"]
        .as_u64()
        .expect("the serialized passkey carries its signature counter")
}

#[test]
fn a_public_https_origin_gives_a_relying_party_and_a_client() {
    let relying_party = relying_party("https://git.example.com").unwrap();

    assert_eq!(relying_party.id, "git.example.com");
    assert_eq!(relying_party.origin.as_str(), "https://git.example.com/");
    assert!(build_webauthn("https://git.example.com").is_some());
}

#[test]
fn localhost_over_http_with_a_port_is_usable_and_keeps_its_port() {
    let relying_party = relying_party("http://localhost:4200").unwrap();

    assert_eq!(relying_party.id, "localhost");
    assert_eq!(relying_party.origin.as_str(), "http://localhost:4200/");
    assert!(build_webauthn("http://localhost:4200").is_some());
}

#[test]
fn a_non_default_port_on_a_public_host_is_kept_and_a_default_one_is_dropped() {
    assert_eq!(
        relying_party("https://git.example.com:8443")
            .unwrap()
            .origin
            .as_str(),
        "https://git.example.com:8443/"
    );
    assert_eq!(
        relying_party("https://git.example.com:443")
            .unwrap()
            .origin
            .as_str(),
        "https://git.example.com/"
    );
}

#[test]
fn only_the_origin_of_the_public_url_counts() {
    let relying_party = relying_party("https://git.example.com/some/path?x=1").unwrap();

    assert_eq!(relying_party.id, "git.example.com");
    assert_eq!(relying_party.origin.as_str(), "https://git.example.com/");
}

#[test]
fn plain_http_is_only_usable_on_localhost_because_browsers_refuse_webauthn_elsewhere() {
    assert!(relying_party("http://localhost:4200").is_some());
    assert!(relying_party("http://localhost").is_some());
    assert!(relying_party("http://dev.localhost:4200").is_some());
    assert!(relying_party("https://git.example.com").is_some());
    assert!(relying_party("http://git.example.com").is_none());
    assert!(relying_party("http://git.example.com:8080").is_none());
    assert!(relying_party("http://notlocalhost").is_none());
    assert!(relying_party("http://localhost.example.com").is_none());
    assert!(build_webauthn("http://git.example.com").is_none());
    assert!(build_webauthn("https://localhost:4200").is_some());
}

#[test]
fn an_ip_literal_host_makes_webauthn_unavailable() {
    assert!(build_webauthn("http://127.0.0.1:8080").is_none());
    assert!(build_webauthn("https://192.168.1.10").is_none());
    assert!(build_webauthn("https://[::1]").is_none());
    assert!(build_webauthn("http://[2001:db8::1]:3000").is_none());
}

#[test]
fn an_unusable_public_url_makes_webauthn_unavailable_without_panicking() {
    assert!(build_webauthn("").is_none());
    assert!(build_webauthn("not a url").is_none());
    assert!(build_webauthn("mailto:someone@example.com").is_none());
}

#[tokio::test]
async fn without_a_client_every_ceremony_is_service_unavailable() {
    let f = fixture_with(None, PasskeyCeremonies::new());
    f.passkeys.seed(f.florian, "MacBook");
    let unreadable: PublicKeyCredential = serde_json::from_value(serde_json::json!({
        "id": "AAAA", "rawId": "AAAA", "type": "public-key",
        "response": { "authenticatorData": "AAAA", "clientDataJSON": "AAAA", "signature": "AAAA" },
    }))
    .unwrap();

    let unreadable_registration: RegisterPublicKeyCredential =
        serde_json::from_value(serde_json::json!({
            "id": "AAAA", "rawId": "AAAA", "type": "public-key",
            "response": { "attestationObject": "AAAA", "clientDataJSON": "AAAA" },
        }))
        .unwrap();

    assert!(!f.service.available());
    assert!(matches!(
        f.service
            .finish_registration(
                f.florian,
                Uuid::new_v4(),
                &unreadable_registration,
                "MacBook"
            )
            .await,
        Err(DomainError::ServiceUnavailable(_))
    ));
    assert!(matches!(
        f.service.start_registration(f.florian).await,
        Err(DomainError::ServiceUnavailable(_))
    ));
    assert!(matches!(
        f.service.start_authentication(f.florian).await,
        Err(DomainError::ServiceUnavailable(_))
    ));
    assert!(matches!(
        f.service
            .finish_authentication(f.florian, Uuid::new_v4(), &unreadable)
            .await,
        Err(DomainError::ServiceUnavailable(_))
    ));
    assert_eq!(f.service.list(f.florian).await.unwrap().len(), 1);
}

#[tokio::test]
async fn with_a_client_the_service_is_available() {
    assert!(fixture().service.available());
}

#[tokio::test]
async fn a_full_registration_stores_a_passkey_for_that_user_only() {
    let f = fixture();
    let mut macbook = authenticator();

    let stored = register(&f, f.florian, &mut macbook, "MacBook").await;

    assert_eq!(stored.user_id, f.florian);
    assert_eq!(stored.name, "MacBook");
    assert!(!stored.credential_id.is_empty());
    assert_eq!(stored.last_used_at, None);
    let rows = f.passkeys.of(f.florian);
    assert_eq!(rows, vec![stored.clone()]);
    assert!(f.passkeys.of(f.marie).is_empty());
    let passkey: Passkey = serde_json::from_str(&stored.passkey_json).unwrap();
    assert_eq!(
        passkey.cred_id().as_ref(),
        stored.credential_id.as_slice(),
        "the credential id column is the passkey's own"
    );
}

#[tokio::test]
async fn the_registration_challenge_names_the_user_and_the_relying_party() {
    let f = fixture();

    let (_, challenge) = f.service.start_registration(f.florian).await.unwrap();

    let options = challenge.public_key;
    assert_eq!(options.rp.id, "localhost");
    assert_eq!(options.rp.name, "FerrisGit");
    assert_eq!(options.user.name, "florian");
    assert_eq!(options.user.display_name, "florian");
    assert_eq!(
        options.user.id.as_ref(),
        f.florian.as_bytes(),
        "the user handle is the user's UUID"
    );
    assert!(
        options.exclude_credentials.is_none()
            || options
                .exclude_credentials
                .as_ref()
                .is_some_and(|c| c.is_empty())
    );
}

#[tokio::test]
async fn starting_a_registration_for_an_unknown_user_is_not_found() {
    let f = fixture();

    assert!(matches!(
        f.service.start_registration(Uuid::new_v4()).await,
        Err(DomainError::NotFound(_))
    ));
}

#[tokio::test]
async fn a_second_registration_challenge_excludes_the_stored_credentials() {
    // The soft authenticator ignores excludeCredentials, so look at the challenge itself. The Conflict test below
    // covers duplicate ids at insert.
    let f = fixture();
    let first = register(&f, f.florian, &mut authenticator(), "MacBook").await;
    let second = register(&f, f.florian, &mut authenticator(), "Clé USB").await;
    register(&f, f.marie, &mut authenticator(), "Marie's key").await;

    let (_, challenge) = f.service.start_registration(f.florian).await.unwrap();

    let excluded: Vec<Vec<u8>> = challenge
        .public_key
        .exclude_credentials
        .iter()
        .flatten()
        .map(|c| c.id.as_ref().to_vec())
        .collect();
    assert_eq!(
        excluded,
        vec![first.credential_id, second.credential_id],
        "the user's own credentials, in order, and nobody else's"
    );
}

#[tokio::test]
async fn a_second_authenticator_registers_alongside_the_first() {
    let f = fixture();
    register(&f, f.florian, &mut authenticator(), "MacBook").await;
    register(&f, f.florian, &mut authenticator(), "Clé USB").await;

    let names: Vec<String> = f
        .service
        .list(f.florian)
        .await
        .unwrap()
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(names, vec!["MacBook", "Clé USB"]);
}

#[tokio::test]
async fn a_credential_id_that_already_exists_is_a_conflict_and_nothing_is_overwritten() {
    let f = fixture();
    let (challenge_id, challenge) = f.service.start_registration(f.florian).await.unwrap();
    let credential = authenticator()
        .do_registration(origin(), challenge)
        .unwrap();
    // Someone else (another user or a racing request) stored this credential id first.
    let taken = StoredPasskey {
        id: Uuid::new_v4(),
        user_id: f.marie,
        name: "first".to_string(),
        credential_id: credential.raw_id.as_ref().to_vec(),
        passkey_json: "{\"first\":true}".to_string(),
        created_at: Utc::now(),
        last_used_at: None,
    };
    assert!(f.passkeys.insert(&taken).await.unwrap());

    let result = f
        .service
        .finish_registration(f.florian, challenge_id, &credential, "MacBook")
        .await;

    assert!(
        matches!(result, Err(DomainError::Conflict(_))),
        "{result:?}"
    );
    assert_eq!(f.passkeys.of(f.marie), vec![taken]);
    assert!(f.passkeys.of(f.florian).is_empty());
}

#[tokio::test]
async fn a_registration_challenge_of_another_user_is_refused_and_stays_usable_by_its_owner() {
    let f = fixture();
    let (challenge_id, challenge) = f.service.start_registration(f.florian).await.unwrap();
    let credential = authenticator()
        .do_registration(origin(), challenge)
        .unwrap();

    assert!(is_invalid_passkey(
        f.service
            .finish_registration(f.marie, challenge_id, &credential, "Thief")
            .await
    ));
    assert!(f.passkeys.of(f.marie).is_empty());

    assert!(
        f.service
            .finish_registration(f.florian, challenge_id, &credential, "MacBook")
            .await
            .is_ok(),
        "the owner's ceremony was not burnt"
    );
}

#[tokio::test]
async fn a_registration_replayed_or_with_an_unknown_challenge_is_refused() {
    let f = fixture();
    let (challenge_id, challenge) = f.service.start_registration(f.florian).await.unwrap();
    let credential = authenticator()
        .do_registration(origin(), challenge)
        .unwrap();
    f.service
        .finish_registration(f.florian, challenge_id, &credential, "MacBook")
        .await
        .unwrap();

    assert!(
        is_invalid_passkey(
            f.service
                .finish_registration(f.florian, challenge_id, &credential, "again")
                .await
        ),
        "replay"
    );
    assert!(is_invalid_passkey(
        f.service
            .finish_registration(f.florian, Uuid::new_v4(), &credential, "unknown")
            .await
    ));
    assert_eq!(f.passkeys.of(f.florian).len(), 1);
}

#[tokio::test]
async fn an_expired_registration_challenge_is_refused() {
    let f = fixture();
    let service = expiring(&f);
    let (challenge_id, challenge) = service.start_registration(f.florian).await.unwrap();
    let credential = authenticator()
        .do_registration(origin(), challenge)
        .unwrap();

    assert!(is_invalid_passkey(
        service
            .finish_registration(f.florian, challenge_id, &credential, "MacBook")
            .await
    ));
    assert!(f.passkeys.of(f.florian).is_empty());
}

#[tokio::test]
async fn an_authentication_challenge_cannot_finish_a_registration() {
    let f = fixture();
    register(&f, f.florian, &mut authenticator(), "MacBook").await;
    let (auth_challenge_id, _) = f.service.start_authentication(f.florian).await.unwrap();
    let (_, challenge) = f.service.start_registration(f.florian).await.unwrap();
    let credential = authenticator()
        .do_registration(origin(), challenge)
        .unwrap();

    assert!(is_invalid_passkey(
        f.service
            .finish_registration(f.florian, auth_challenge_id, &credential, "Mixed up")
            .await
    ));
    assert_eq!(f.passkeys.of(f.florian).len(), 1);
}

#[tokio::test]
async fn a_registration_from_another_origin_is_refused() {
    let f = fixture();
    let (challenge_id, challenge) = f.service.start_registration(f.florian).await.unwrap();
    let credential = authenticator()
        .do_registration(Url::parse("http://localhost:4300").unwrap(), challenge)
        .unwrap();

    assert!(is_invalid_passkey(
        f.service
            .finish_registration(f.florian, challenge_id, &credential, "MacBook")
            .await
    ));
    assert!(f.passkeys.of(f.florian).is_empty());
}

#[tokio::test]
async fn passkey_names_are_trimmed_and_bounded() {
    let f = fixture();
    let mut macbook = authenticator();

    let stored = register(&f, f.florian, &mut macbook, "  MacBook de Florian \n").await;
    assert_eq!(stored.name, "MacBook de Florian");

    let long = "x".repeat(40);
    assert_eq!(
        register(&f, f.florian, &mut authenticator(), &long)
            .await
            .name,
        long
    );
    assert_eq!(
        register(&f, f.florian, &mut authenticator(), &"é".repeat(40))
            .await
            .name
            .chars()
            .count(),
        40,
        "the limit counts characters, not bytes"
    );
}

#[tokio::test]
async fn bad_names_are_refused_before_the_ceremony_is_consumed() {
    let f = fixture();
    let (challenge_id, challenge) = f.service.start_registration(f.florian).await.unwrap();
    let credential = authenticator()
        .do_registration(origin(), challenge)
        .unwrap();

    for bad in [
        "",
        "   ",
        "\t\n",
        &"x".repeat(41),
        &"é".repeat(41),
        "a\u{0}b",
        "line\nbreak",
        "bell\u{7}",
        "del\u{7f}",
        "c1\u{85}c2",
    ] {
        let result = f
            .service
            .finish_registration(f.florian, challenge_id, &credential, bad)
            .await;
        assert!(
            matches!(&result, Err(DomainError::Validation(message)) if message == "invalid passkey name"),
            "{bad:?}: {result:?}"
        );
    }
    assert!(f.passkeys.of(f.florian).is_empty());

    assert!(
        f.service
            .finish_registration(f.florian, challenge_id, &credential, "MacBook")
            .await
            .is_ok(),
        "the ceremony survived the bad names"
    );
}

#[tokio::test]
async fn a_full_authentication_succeeds_stamps_last_used_and_persists_the_counter() {
    let f = fixture();
    let mut macbook = authenticator();
    let registered = register(&f, f.florian, &mut macbook, "MacBook").await;
    assert_eq!(registered.last_used_at, None);
    let counter_before = counter_of(&registered);

    authenticate(&f, f.florian, &mut macbook).await.unwrap();

    let stored = f.passkeys.of(f.florian).remove(0);
    assert!(stored.last_used_at.is_some(), "last_used_at is set");
    assert!(
        counter_of(&stored) > counter_before,
        "the signature counter the assertion reported is persisted: {} -> {}",
        counter_before,
        counter_of(&stored)
    );
    assert_eq!(stored.credential_id, registered.credential_id);
    assert_eq!(stored.name, "MacBook");
    authenticate(&f, f.florian, &mut macbook).await.unwrap();
    assert!(counter_of(&f.passkeys.of(f.florian)[0]) > counter_of(&stored));
}

#[tokio::test]
async fn the_authentication_challenge_lists_only_the_users_own_credentials() {
    let f = fixture();
    let florians = register(&f, f.florian, &mut authenticator(), "MacBook").await;
    register(&f, f.marie, &mut authenticator(), "Marie's key").await;

    let (_, challenge) = f.service.start_authentication(f.florian).await.unwrap();

    let allowed: Vec<Vec<u8>> = challenge
        .public_key
        .allow_credentials
        .iter()
        .map(|c| c.id.as_ref().to_vec())
        .collect();
    assert_eq!(allowed, vec![florians.credential_id]);
    assert_eq!(challenge.public_key.rp_id, "localhost");
}

#[tokio::test]
async fn starting_an_authentication_without_a_passkey_is_a_validation_error() {
    let f = fixture();
    register(&f, f.marie, &mut authenticator(), "Marie's key").await;

    assert!(matches!(
        f.service.start_authentication(f.florian).await,
        Err(DomainError::Validation(_))
    ));
}

#[tokio::test]
async fn an_authentication_challenge_of_another_user_is_refused_and_stays_usable() {
    let f = fixture();
    let mut florians = authenticator();
    register(&f, f.florian, &mut florians, "MacBook").await;
    register(&f, f.marie, &mut authenticator(), "Marie's key").await;
    let (challenge_id, challenge) = f.service.start_authentication(f.florian).await.unwrap();
    let credential = florians.do_authentication(origin(), challenge).unwrap();

    assert!(is_invalid_code(
        f.service
            .finish_authentication(f.marie, challenge_id, &credential)
            .await
    ));
    assert_eq!(f.passkeys.of(f.florian)[0].last_used_at, None);

    assert!(
        f.service
            .finish_authentication(f.florian, challenge_id, &credential)
            .await
            .is_ok(),
        "the owner's ceremony was not burnt"
    );
}

#[tokio::test]
async fn a_replayed_assertion_is_refused() {
    let f = fixture();
    let mut macbook = authenticator();
    register(&f, f.florian, &mut macbook, "MacBook").await;
    let (challenge_id, challenge) = f.service.start_authentication(f.florian).await.unwrap();
    let credential = macbook.do_authentication(origin(), challenge).unwrap();
    f.service
        .finish_authentication(f.florian, challenge_id, &credential)
        .await
        .unwrap();

    assert!(is_invalid_code(
        f.service
            .finish_authentication(f.florian, challenge_id, &credential)
            .await
    ));
    // Replayed against a fresh challenge, it still fails: it was signed over the old one.
    let (fresh_id, _) = f.service.start_authentication(f.florian).await.unwrap();
    assert!(is_invalid_code(
        f.service
            .finish_authentication(f.florian, fresh_id, &credential)
            .await
    ));
}

#[tokio::test]
async fn an_unknown_or_expired_authentication_challenge_is_refused() {
    let f = fixture();
    let mut macbook = authenticator();
    register(&f, f.florian, &mut macbook, "MacBook").await;
    let (_, challenge) = f.service.start_authentication(f.florian).await.unwrap();
    let credential = macbook.do_authentication(origin(), challenge).unwrap();
    assert!(is_invalid_code(
        f.service
            .finish_authentication(f.florian, Uuid::new_v4(), &credential)
            .await
    ));

    let service = expiring(&f);
    let (challenge_id, challenge) = service.start_authentication(f.florian).await.unwrap();
    let credential = macbook.do_authentication(origin(), challenge).unwrap();
    assert!(is_invalid_code(
        service
            .finish_authentication(f.florian, challenge_id, &credential)
            .await
    ));
    assert_eq!(f.passkeys.of(f.florian)[0].last_used_at, None);
}

#[tokio::test]
async fn a_registration_challenge_cannot_finish_an_authentication() {
    let f = fixture();
    let mut macbook = authenticator();
    register(&f, f.florian, &mut macbook, "MacBook").await;
    let (registration_id, _) = f.service.start_registration(f.florian).await.unwrap();
    let (_, challenge) = f.service.start_authentication(f.florian).await.unwrap();
    let credential = macbook.do_authentication(origin(), challenge).unwrap();

    assert!(is_invalid_code(
        f.service
            .finish_authentication(f.florian, registration_id, &credential)
            .await
    ));
}

#[tokio::test]
async fn a_tampered_assertion_is_refused_and_burns_the_challenge() {
    let f = fixture();
    let mut macbook = authenticator();
    register(&f, f.florian, &mut macbook, "MacBook").await;
    let (challenge_id, challenge) = f.service.start_authentication(f.florian).await.unwrap();
    let mut credential = macbook.do_authentication(origin(), challenge).unwrap();
    let mut signature = credential.response.signature.as_ref().to_vec();
    let last = signature.len() - 1;
    signature[last] ^= 0x01;
    credential.response.signature = signature.into();

    assert!(is_invalid_code(
        f.service
            .finish_authentication(f.florian, challenge_id, &credential)
            .await
    ));

    assert_eq!(
        f.passkeys.of(f.florian)[0].last_used_at,
        None,
        "nothing was persisted"
    );
    assert!(
        is_invalid_code(
            f.service
                .finish_authentication(f.florian, challenge_id, &credential)
                .await
        ),
        "a failed attempt consumes the challenge"
    );
}

#[tokio::test]
async fn an_assertion_from_an_authenticator_the_user_has_not_registered_is_refused() {
    let f = fixture();
    register(&f, f.florian, &mut authenticator(), "MacBook").await;
    let mut stranger = authenticator();
    let stranger_credential = register(&f, f.marie, &mut stranger, "Marie's key").await;
    let (challenge_id, mut challenge) = f.service.start_authentication(f.florian).await.unwrap();
    // A malicious client signs Florian's challenge with its own credential.
    challenge.public_key.allow_credentials[0].id = stranger_credential.credential_id.clone().into();
    let credential = stranger.do_authentication(origin(), challenge).unwrap();

    assert!(is_invalid_code(
        f.service
            .finish_authentication(f.florian, challenge_id, &credential)
            .await
    ));
    assert_eq!(f.passkeys.of(f.florian)[0].last_used_at, None);
    assert_eq!(f.passkeys.of(f.marie)[0].last_used_at, None);
}

#[tokio::test]
async fn an_assertion_with_a_counter_regression_is_refused_and_persists_nothing() {
    let f = fixture();
    let mut macbook = authenticator();
    let registered = register(&f, f.florian, &mut macbook, "MacBook").await;
    // The stored counter is far ahead of what the authenticator reports, like a cloned key.
    let mut json: serde_json::Value = serde_json::from_str(&registered.passkey_json).unwrap();
    json["cred"]["counter"] = serde_json::json!(1_000);
    let ahead = json.to_string();
    f.passkeys
        .update_after_authentication(registered.id, &ahead)
        .await
        .unwrap();
    let before = f.passkeys.of(f.florian).remove(0);

    assert!(is_invalid_code(
        authenticate(&f, f.florian, &mut macbook).await
    ));

    let after = f.passkeys.of(f.florian).remove(0);
    assert_eq!(after.passkey_json, before.passkey_json);
    assert_eq!(after.last_used_at, before.last_used_at);
}

#[tokio::test]
async fn a_stored_passkey_that_cannot_be_read_fails_closed_as_an_infrastructure_error() {
    let f = fixture();
    f.passkeys.seed(f.florian, "corrupt");

    let result = f.service.start_authentication(f.florian).await;

    assert!(
        matches!(result, Err(DomainError::Infrastructure(_))),
        "{result:?}"
    );
}

#[tokio::test]
async fn list_returns_only_the_users_own_passkeys() {
    let f = fixture();
    register(&f, f.florian, &mut authenticator(), "MacBook").await;
    register(&f, f.marie, &mut authenticator(), "Marie's key").await;

    let listed = f.service.list(f.florian).await.unwrap();

    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "MacBook");
}

#[tokio::test]
async fn a_deleted_passkey_can_no_longer_authenticate_and_can_be_registered_again() {
    let f = fixture();
    let mut macbook = authenticator();
    let stored = register(&f, f.florian, &mut macbook, "MacBook").await;
    assert!(f.passkeys.delete(stored.id, f.florian).await.unwrap());

    assert!(matches!(
        f.service.start_authentication(f.florian).await,
        Err(DomainError::Validation(_))
    ));
    register(&f, f.florian, &mut macbook, "MacBook again").await;
    assert_eq!(f.passkeys.of(f.florian).len(), 1);
}

#[test]
fn the_ceremony_store_used_by_the_fixture_starts_empty() {
    // Guards the fixture: a store shared between tests would make the single-use tests meaningless.
    let f = fixture();
    assert!(f.ceremonies.take(Uuid::new_v4(), f.florian).is_none());
}

#[tokio::test]
async fn logging_in_with_the_second_passkey_updates_only_that_row() {
    let f = fixture();
    let mut macbook = authenticator();
    let mut yubikey = authenticator();
    register(&f, f.florian, &mut macbook, "MacBook").await;
    register(&f, f.florian, &mut yubikey, "YubiKey").await;
    let rows = f.passkeys.of(f.florian);
    let (macbook_before, yubikey_before) = (rows[0].clone(), rows[1].clone());
    assert_eq!(
        (macbook_before.name.as_str(), yubikey_before.name.as_str()),
        ("MacBook", "YubiKey")
    );

    authenticate(&f, f.florian, &mut yubikey).await.unwrap();

    let rows = f.passkeys.of(f.florian);
    assert_eq!(
        rows[0], macbook_before,
        "the first row is byte for byte unchanged (json and last_used_at)"
    );
    assert!(rows[1].last_used_at.is_some());
    assert!(
        counter_of(&rows[1]) > counter_of(&yubikey_before),
        "the YubiKey's own counter went up"
    );
    assert_eq!(rows[1].credential_id, yubikey_before.credential_id);

    let after_first = rows[1].clone();
    authenticate(&f, f.florian, &mut yubikey).await.unwrap();
    let rows = f.passkeys.of(f.florian);
    assert_eq!(
        rows[0], macbook_before,
        "still untouched after a second YubiKey login"
    );
    assert!(counter_of(&rows[1]) > counter_of(&after_first));

    authenticate(&f, f.florian, &mut macbook).await.unwrap();
    let rows = f.passkeys.of(f.florian);
    assert!(
        rows[0].last_used_at.is_some() && counter_of(&rows[0]) > counter_of(&macbook_before),
        "and the MacBook updates its own row"
    );
    assert!(
        counter_of(&rows[1]) > counter_of(&after_first),
        "the YubiKey row is not rewound"
    );
}

#[tokio::test]
async fn an_assertion_from_another_origin_is_refused_and_persists_nothing() {
    let f = fixture();
    let mut macbook = authenticator();
    register(&f, f.florian, &mut macbook, "MacBook").await;
    let before = f.passkeys.of(f.florian);
    let (challenge_id, challenge) = f.service.start_authentication(f.florian).await.unwrap();
    let credential = macbook
        .do_authentication(Url::parse("http://localhost:4300").unwrap(), challenge)
        .unwrap();

    assert!(is_invalid_code(
        f.service
            .finish_authentication(f.florian, challenge_id, &credential)
            .await
    ));

    assert_eq!(f.passkeys.of(f.florian), before);
}

#[tokio::test]
async fn a_subdomain_origin_is_refused_for_registration_and_for_assertions() {
    // Real https deployment, the soft client itself only does plain http on localhost.
    let f = fixture_with(
        build_webauthn("https://example.com").map(Arc::new),
        PasskeyCeremonies::new(),
    );
    let real = Url::parse("https://example.com").unwrap();
    let subdomain = Url::parse("https://sub.example.com").unwrap();

    let (challenge_id, challenge) = f.service.start_registration(f.florian).await.unwrap();
    let credential = authenticator()
        .do_registration(subdomain.clone(), challenge)
        .unwrap();
    assert!(is_invalid_passkey(
        f.service
            .finish_registration(f.florian, challenge_id, &credential, "MacBook")
            .await
    ));
    assert!(f.passkeys.of(f.florian).is_empty());

    let mut macbook = authenticator();
    let (challenge_id, challenge) = f.service.start_registration(f.florian).await.unwrap();
    let credential = macbook.do_registration(real.clone(), challenge).unwrap();
    f.service
        .finish_registration(f.florian, challenge_id, &credential, "MacBook")
        .await
        .unwrap();
    let (challenge_id, challenge) = f.service.start_authentication(f.florian).await.unwrap();
    let credential = macbook.do_authentication(subdomain, challenge).unwrap();
    assert!(is_invalid_code(
        f.service
            .finish_authentication(f.florian, challenge_id, &credential)
            .await
    ));
    assert_eq!(f.passkeys.of(f.florian)[0].last_used_at, None);

    let (challenge_id, challenge) = f.service.start_authentication(f.florian).await.unwrap();
    let credential = macbook.do_authentication(real, challenge).unwrap();
    assert!(
        f.service
            .finish_authentication(f.florian, challenge_id, &credential)
            .await
            .is_ok(),
        "the real origin still works"
    );
}

#[tokio::test]
async fn an_assertion_older_than_the_persisted_counter_is_refused_even_when_its_ceremony_is_older_too()
 {
    let f = fixture();
    let mut macbook = authenticator();
    register(&f, f.florian, &mut macbook, "MacBook").await;
    // Two ceremonies live at once, both started while the saved counter is still 0.
    let (first_id, first) = f.service.start_authentication(f.florian).await.unwrap();
    let (second_id, second) = f.service.start_authentication(f.florian).await.unwrap();
    let older = macbook.do_authentication(origin(), first).unwrap(); // reports counter N + 1
    let newer = macbook.do_authentication(origin(), second).unwrap(); // reports counter N + 2

    f.service
        .finish_authentication(f.florian, second_id, &newer)
        .await
        .unwrap();
    let persisted = f.passkeys.of(f.florian).remove(0);

    assert!(is_invalid_code(
        f.service
            .finish_authentication(f.florian, first_id, &older)
            .await
    ));

    assert_eq!(
        f.passkeys.of(f.florian).remove(0),
        persisted,
        "the counter is not rewound and nothing else changes"
    );
}

#[tokio::test]
async fn a_fresh_passkey_with_a_stored_counter_of_zero_accepts_its_first_assertion() {
    // 0 against 0 is fine: the rejection only applies when one of the counters is non-zero, and a new key starts at 0.
    let f = fixture();
    let mut macbook = authenticator();
    register(&f, f.florian, &mut macbook, "MacBook").await;
    assert_eq!(counter_of(&f.passkeys.of(f.florian)[0]), 0);

    authenticate(&f, f.florian, &mut macbook).await.unwrap();

    assert!(counter_of(&f.passkeys.of(f.florian)[0]) > 0);
}

#[test]
fn an_unreadable_stored_counter_fails_closed() {
    let stored = StoredPasskey {
        id: Uuid::new_v4(),
        user_id: Uuid::new_v4(),
        name: "x".to_string(),
        credential_id: vec![1],
        passkey_json: "{\"cred\":{}}".to_string(),
        created_at: Utc::now(),
        last_used_at: None,
    };

    assert!(matches!(
        stored_counter(&stored),
        Err(DomainError::Infrastructure(_))
    ));
}

#[tokio::test]
async fn a_user_cannot_hold_more_than_twenty_passkeys() {
    let f = fixture();
    for i in 0..19 {
        f.passkeys.seed(f.florian, &format!("key {i}"));
    }
    let (challenge_id, challenge) = f.service.start_registration(f.florian).await.unwrap();
    let credential = authenticator()
        .do_registration(origin(), challenge)
        .unwrap();
    // A parallel registration takes the twentieth slot between start and finish.
    f.passkeys.seed(f.florian, "the twentieth");

    let at_finish = f
        .service
        .finish_registration(f.florian, challenge_id, &credential, "one too many")
        .await;
    assert!(
        matches!(&at_finish, Err(DomainError::Validation(message)) if message == "too many passkeys"),
        "{at_finish:?}"
    );
    assert_eq!(f.passkeys.of(f.florian).len(), 20);

    let at_start = f.service.start_registration(f.florian).await;
    assert!(
        matches!(&at_start, Err(DomainError::Validation(message)) if message == "too many passkeys"),
        "{at_start:?}"
    );
    f.passkeys.seed(f.marie, "someone else's");
    assert!(
        f.service.start_registration(f.marie).await.is_ok(),
        "the cap is per user"
    );
}
