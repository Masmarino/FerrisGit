//! Pure TOTP and backup-code helpers. No I/O and no clock: callers pass the time in.

use ferrisgit_domain::error::DomainError;
use rand::Rng;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use totp_rs::{Algorithm, Builder, Secret, Totp};

const TOTP_ISSUER: &str = "FerrisGit";
const BACKUP_CODE_COUNT: usize = 10;
pub const TOTP_STEP_SECS: u64 = 30;

/// 20 random bytes (the RFC 4226 seed size) as unpadded base32, 32 characters.
pub fn generate_secret_base32() -> String {
    let mut bytes = [0u8; 20];
    rand::rng().fill_bytes(&mut bytes);
    Secret::from(bytes).to_base32()
}

/// SHA1, 6 digits, 30 s step: what nearly every authenticator app assumes. A `:` in the account name would break the
/// otpauth label, so it becomes `_` instead of failing enrolment over an odd username.
fn build_totp(secret_base32: &str, username: &str) -> Result<Totp, DomainError> {
    let secret = Secret::try_from_base32(secret_base32).map_err(|_| {
        DomainError::Infrastructure("stored TOTP secret is not valid base32".to_string())
    })?;
    Builder::new()
        .with_algorithm(Algorithm::SHA1)
        .with_digits(6)
        .with_skew(1)
        .with_step_duration(TOTP_STEP_SECS)
        .with_secret(secret)
        .with_account_name(username.replace(':', "_"))
        .with_issuer(Some(TOTP_ISSUER))
        .build()
        .map_err(|e| DomainError::Infrastructure(e.to_string()))
}

/// `totp-rs` omits default parameters from the URL, so they're added back to tell every authenticator app exactly what
/// to compute.
pub fn otpauth_url(secret_base32: &str, username: &str) -> Result<String, DomainError> {
    let url = build_totp(secret_base32, username)?
        .to_url()
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
    Ok(format!(
        "{url}&algorithm=SHA1&digits=6&period={TOTP_STEP_SECS}"
    ))
}

/// The highest matching step wins, which is safest against replay, with one step of skew each way. Constant time: all
/// three candidates are always generated and compared, no early exit.
pub fn matching_step(secret_base32: &str, code: &str, now_unix: u64) -> Option<i64> {
    let totp = build_totp(secret_base32, "").ok()?;
    let candidate = code.trim().as_bytes();
    let now_step = now_unix / TOTP_STEP_SECS;
    let mut matched: Option<i64> = None;
    for step in [now_step.saturating_sub(1), now_step, now_step + 1] {
        let expected = totp.generate(step * TOTP_STEP_SECS).to_string();
        let equal: bool = expected.as_bytes().ct_eq(candidate).into();
        if equal {
            matched = Some(matched.map_or(step as i64, |m| m.max(step as i64)));
        }
    }
    matched
}

/// Code for a given time. Meant for tests, panics if the secret isn't ours.
pub fn generate_code_at(secret_base32: &str, unix_time: u64) -> String {
    build_totp(secret_base32, "")
        .expect("a secret produced by generate_secret_base32 is always valid")
        .generate(unix_time)
        .to_string()
}

fn generate_backup_code() -> String {
    let mut bytes = [0u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// The plaintext codes and their salted hashes, index-aligned. Only the hashes are persisted.
pub fn generate_backup_codes() -> (Vec<String>, Vec<String>) {
    let plaintext: Vec<String> = (0..BACKUP_CODE_COUNT)
        .map(|_| generate_backup_code())
        .collect();
    let hashes = plaintext
        .iter()
        .map(|code| hash_backup_code(code))
        .collect();
    (plaintext, hashes)
}

fn salted_digest(salt_hex: &str, plaintext: &str) -> [u8; 32] {
    Sha256::digest(format!("{salt_hex}{plaintext}").as_bytes()).into()
}

/// Stored as `<32 hex salt>:<64 hex sha256 of salt hex + code>`. The salt keeps identical codes from sharing a stored
/// value and defeats a rainbow table. The codes are 128-bit random, so a fast hash is enough.
pub fn hash_backup_code(plaintext: &str) -> String {
    let mut salt = [0u8; 16];
    rand::rng().fill_bytes(&mut salt);
    let salt_hex = hex::encode(salt);
    format!(
        "{salt_hex}:{}",
        hex::encode(salted_digest(&salt_hex, plaintext))
    )
}

/// Counterpart of `hash_backup_code`, compared in constant time so it doesn't stop at the first differing byte. A
/// malformed stored value never verifies.
pub fn verify_backup_code(plaintext: &str, stored: &str) -> bool {
    let Some((salt_hex, digest_hex)) = stored.split_once(':') else {
        return false;
    };
    let Ok(stored_digest) = hex::decode(digest_hex) else {
        return false;
    };
    salted_digest(salt_hex, plaintext)
        .as_slice()
        .ct_eq(stored_digest.as_slice())
        .into()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    const SECRET: &str = "JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP";
    const NOW: u64 = 1_700_000_010; // step 56_666_667, 10 s into it

    fn step_of(unix: u64) -> i64 {
        (unix / TOTP_STEP_SECS) as i64
    }

    /// RFC 6238 appendix B SHA-1 vectors (secret is ASCII "12345678901234567890"), last six digits of each reference value.
    /// An independent check of algorithm, digits and step, since every other test goes through the same `build_totp`.
    #[test]
    fn matches_the_rfc_6238_known_answers() {
        const RFC_SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

        for (unix_time, expected) in [
            (59, "287082"),
            (1_111_111_109, "081804"),
            (1_234_567_890, "005924"),
            (2_000_000_000, "279037"),
        ] {
            assert_eq!(
                generate_code_at(RFC_SECRET, unix_time),
                expected,
                "T = {unix_time}"
            );
            assert_eq!(
                matching_step(RFC_SECRET, expected, unix_time),
                Some(step_of(unix_time)),
                "T = {unix_time}"
            );
        }
    }

    #[test]
    fn a_generated_secret_is_20_bytes_of_base32_and_never_repeats() {
        let a = generate_secret_base32();
        let b = generate_secret_base32();

        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| matches!(c, 'A'..='Z' | '2'..='7')));
        assert_eq!(Secret::try_from_base32(&a).unwrap().as_bytes().len(), 20);
        assert_ne!(a, b);
    }

    #[test]
    fn the_otpauth_url_carries_every_parameter_authenticators_need() {
        let url = otpauth_url(SECRET, "florian").unwrap();

        assert!(url.starts_with("otpauth://totp/"), "{url}");
        assert!(url.contains("FerrisGit:florian"), "{url}");
        assert!(url.contains(&format!("secret={SECRET}")), "{url}");
        assert!(url.contains("issuer=FerrisGit"), "{url}");
        assert!(url.contains("digits=6"), "{url}");
        assert!(url.contains("period=30"), "{url}");
        assert!(url.contains("algorithm=SHA1"), "{url}");
    }

    #[test]
    fn the_otpauth_url_percent_encodes_the_username_and_survives_a_colon() {
        let url = otpauth_url(SECRET, "flo rian").unwrap();
        assert!(url.contains("flo%20rian"), "{url}");

        // `:` separates the otpauth label, so the name gets sanitised instead of failing enrolment.
        assert!(otpauth_url(SECRET, "a:b").is_ok());
    }

    #[test]
    fn an_invalid_stored_secret_is_an_infrastructure_error() {
        assert!(matches!(
            build_totp("not base32 !!", "florian"),
            Err(DomainError::Infrastructure(_))
        ));
        assert!(matches!(
            otpauth_url("not base32 !!", "florian"),
            Err(DomainError::Infrastructure(_))
        ));
    }

    #[test]
    fn a_generated_code_is_six_digits_and_matches_its_own_step() {
        let code = generate_code_at(SECRET, NOW);

        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
        assert_eq!(matching_step(SECRET, &code, NOW), Some(step_of(NOW)));
    }

    #[test]
    fn the_previous_and_the_next_step_are_accepted_and_report_their_own_step() {
        let previous = generate_code_at(SECRET, NOW - TOTP_STEP_SECS);
        let next = generate_code_at(SECRET, NOW + TOTP_STEP_SECS);

        assert_eq!(
            matching_step(SECRET, &previous, NOW),
            Some(step_of(NOW) - 1)
        );
        assert_eq!(matching_step(SECRET, &next, NOW), Some(step_of(NOW) + 1));
    }

    #[test]
    fn a_code_two_steps_away_is_refused() {
        let old = generate_code_at(SECRET, NOW - 2 * TOTP_STEP_SECS);
        let future = generate_code_at(SECRET, NOW + 2 * TOTP_STEP_SECS);

        assert_eq!(matching_step(SECRET, &old, NOW), None);
        assert_eq!(matching_step(SECRET, &future, NOW), None);
    }

    #[test]
    fn a_wrong_non_numeric_or_empty_code_is_refused() {
        let good = generate_code_at(SECRET, NOW);
        let wrong = if good == "000000" {
            "000001".to_string()
        } else {
            "000000".to_string()
        };
        // The previous or next step could match `wrong` by chance, which would make the test flaky.
        let candidates: HashSet<String> = [NOW - 30, NOW, NOW + 30]
            .iter()
            .map(|t| generate_code_at(SECRET, *t))
            .collect();
        let wrong = if candidates.contains(&wrong) {
            "999999".to_string()
        } else {
            wrong
        };
        assert!(!candidates.contains(&wrong));

        assert_eq!(matching_step(SECRET, &wrong, NOW), None);
        assert_eq!(matching_step(SECRET, "abcdef", NOW), None);
        assert_eq!(matching_step(SECRET, "", NOW), None);
        assert_eq!(
            matching_step(SECRET, &format!("{good}0"), NOW),
            None,
            "a longer code is not a prefix match"
        );
    }

    #[test]
    fn whitespace_around_a_code_is_trimmed() {
        let code = generate_code_at(SECRET, NOW);

        assert_eq!(
            matching_step(SECRET, &format!("  {code}\n"), NOW),
            Some(step_of(NOW))
        );
    }

    #[test]
    fn matching_step_with_an_unusable_secret_is_none() {
        assert_eq!(matching_step("not base32 !!", "123456", NOW), None);
    }

    #[test]
    fn matching_step_near_the_epoch_does_not_underflow() {
        let code = generate_code_at(SECRET, 5);
        assert_eq!(matching_step(SECRET, &code, 5), Some(0));
    }

    #[test]
    fn backup_codes_are_ten_unique_32_hex_codes_with_matching_salted_hashes() {
        let (plaintext, hashes) = generate_backup_codes();

        assert_eq!(plaintext.len(), BACKUP_CODE_COUNT);
        assert_eq!(hashes.len(), BACKUP_CODE_COUNT);
        assert_eq!(
            plaintext.iter().collect::<HashSet<_>>().len(),
            BACKUP_CODE_COUNT
        );
        for code in &plaintext {
            assert_eq!(code.len(), 32);
            assert!(
                code.chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
                "{code}"
            );
        }
        for (code, hash) in plaintext.iter().zip(&hashes) {
            let (salt, digest) = hash.split_once(':').expect("<salt>:<digest>");
            assert_eq!(salt.len(), 32);
            assert_eq!(digest.len(), 64);
            assert!(
                salt.chars()
                    .chain(digest.chars())
                    .all(|c| c.is_ascii_hexdigit())
            );
            assert!(verify_backup_code(code, hash));
        }
    }

    #[test]
    fn hashing_the_same_code_twice_differs_thanks_to_the_salt_and_both_verify() {
        let a = hash_backup_code("abc123");
        let b = hash_backup_code("abc123");

        assert_ne!(a, b);
        assert!(verify_backup_code("abc123", &a));
        assert!(verify_backup_code("abc123", &b));
    }

    #[test]
    fn a_wrong_code_or_a_tampered_hash_does_not_verify() {
        let hash = hash_backup_code("abc123");
        let (salt, digest) = hash.split_once(':').unwrap();
        let flipped_last = format!(
            "{}{}",
            &digest[..63],
            if digest.ends_with('0') { '1' } else { '0' }
        );
        let flipped_first = format!(
            "{}{}",
            if digest.starts_with('0') { '1' } else { '0' },
            &digest[1..]
        );

        assert!(!verify_backup_code("wrong", &hash));
        assert!(!verify_backup_code(
            "abc123",
            &format!("{salt}:{flipped_last}")
        ));
        assert!(!verify_backup_code(
            "abc123",
            &format!("{salt}:{flipped_first}")
        ));
        assert!(
            !verify_backup_code("abc123", &format!("{}:{digest}", "0".repeat(32))),
            "another salt"
        );
    }

    #[test]
    fn a_malformed_stored_hash_never_verifies() {
        let digest = "0".repeat(64);
        assert!(!verify_backup_code("abc123", ""));
        assert!(!verify_backup_code("abc123", "no-separator"));
        assert!(!verify_backup_code("abc123", &format!("salt:{digest}z")));
        assert!(!verify_backup_code("abc123", "salt:zz"));
        assert!(!verify_backup_code("abc123", "salt:"));
        assert!(!verify_backup_code(
            "abc123",
            &hex::encode(Sha256::digest(b"abc123"))
        ));
    }
}
