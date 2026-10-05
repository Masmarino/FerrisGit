//! Account rules shared by self-registration and admin invitation, so the two paths can't disagree.

use std::sync::Arc;

use ferrisgit_domain::email::is_valid_mailbox;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::GroupStorePort;
use ferrisgit_domain::user::{PasswordHasherPort, UserRepositoryPort};
use rand::Rng;

pub const MIN_PASSWORD_LEN: usize = 8;
pub const INVITATION_TTL_HOURS: i64 = 24;

/// An account nobody activated is deleted this long after it was created. Until then a reminder with a fresh link goes
/// out whenever the previous link has expired, so once a day.
pub const PENDING_ACCOUNT_RETENTION_DAYS: i64 = 7;
/// Much shorter than an invitation: this link sets the password of an active account, so a leaked one is worth more.
pub const PASSWORD_RESET_TTL_HOURS: i64 = 1;

const USERNAME_MIN_LEN: usize = 3;
const USERNAME_MAX_LEN: usize = 32;

/// Names nobody can take: every top-level SPA route (a user page lives at `/<username>`, so a user called `login` would
/// shadow the login page) plus infrastructure names. `reserved_names_cover_every_frontend_route` fails until new routes
/// from `app.routes.ts` are added here.
const RESERVED_USERNAMES: &[&str] = &[
    "login",
    "register",
    "activate",
    "invitation",
    "reset-password",
    "home",
    "repositories",
    "groups",
    "account",
    "search",
    "runners",
    "admin",
    "api",
    "assets",
    "static",
    "settings",
    "health",
    "git",
    "explore",
    "docs",
    "help",
    "about",
    "me",
    "new",
    "notifications",
    "users",
    "wiki",
];

/// Trims and lower-cases, then requires 3-32 chars, a leading ASCII letter, only `[a-z0-9_-]` and a non-reserved name.
/// No `.`: a username like `x.git` would make the git-vs-SPA fallback treat the user's pages as git requests.
pub fn normalize_username(raw: &str) -> Result<String, DomainError> {
    let username = raw.trim().to_lowercase();
    if !username
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err(DomainError::Validation(
            "username may only contain letters, digits, '-' and '_'".to_string(),
        ));
    }
    if !(USERNAME_MIN_LEN..=USERNAME_MAX_LEN).contains(&username.len()) {
        return Err(DomainError::Validation(format!(
            "username must be {USERNAME_MIN_LEN} to {USERNAME_MAX_LEN} characters"
        )));
    }
    if !username.starts_with(|c: char| c.is_ascii_lowercase()) {
        return Err(DomainError::Validation(
            "username must start with a letter".to_string(),
        ));
    }
    if RESERVED_USERNAMES.contains(&username.as_str()) || is_placeholder_username(&username) {
        return Err(DomainError::Validation("username is reserved".to_string()));
    }
    Ok(username)
}

const PLACEHOLDER_USERNAME_PREFIX: &str = "invite-";

/// The name of an account an admin invited by e-mail, until its owner chooses theirs at activation: `invite-` and 12
/// hex digits. Valid as a username, so the account fits every place a user does, but never accepted as a chosen one.
pub fn is_placeholder_username(username: &str) -> bool {
    username
        .strip_prefix(PLACEHOLDER_USERNAME_PREFIX)
        .is_some_and(|rest| {
            rest.len() == 12 && rest.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        })
}

pub(crate) fn placeholder_username() -> String {
    let mut bytes = [0u8; 6];
    rand::rng().fill_bytes(&mut bytes);
    format!("{PLACEHOLDER_USERNAME_PREFIX}{}", hex::encode(bytes))
}

/// The link of an activation mail. An invited account whose owner still has to choose a name lands on `/invitation`,
/// which asks for it; one that already has its name (self-registration) on `/activate`. The token is in the fragment,
/// which browsers never send, so it stays out of access logs.
pub fn activation_url(public_url: &str, username: &str, token: &str) -> String {
    let page = if is_placeholder_username(username) {
        "invitation"
    } else {
        "activate"
    };
    format!("{public_url}/{page}#token={token}")
}

pub fn validate_password(password: &str) -> Result<(), DomainError> {
    if password.len() < MIN_PASSWORD_LEN {
        return Err(DomainError::Validation(format!(
            "password must be at least {MIN_PASSWORD_LEN} characters"
        )));
    }
    Ok(())
}

pub fn normalize_email(raw: &str) -> Result<String, DomainError> {
    let email = raw.trim();
    if !is_valid_mailbox(email) {
        return Err(DomainError::Validation(
            "email is not a valid address".to_string(),
        ));
    }
    Ok(email.to_string())
}

/// Only its `token_hash::hash_token` is stored. Admin password reset links use it too.
pub fn generate_invitation_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Checked before any hashing so garbage sent to the public activation and reset endpoints costs nothing.
pub fn is_invitation_token_shaped(token: &str) -> bool {
    token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Argon2 is slow on purpose. On the async runtime a burst of unauthenticated requests would block the workers that
/// also serve the API and git, hence the blocking pool.
pub(crate) async fn hash_blocking(
    hasher: &Arc<dyn PasswordHasherPort>,
    secret: String,
) -> Result<String, DomainError> {
    let hasher = hasher.clone();
    tokio::task::spawn_blocking(move || hasher.hash(&secret))
        .await
        .map_err(|e| DomainError::Infrastructure(e.to_string()))?
}

/// Username not taken (case-insensitive), not the name of a root group (both live at `/<name>`), email not in use.
pub(crate) async fn ensure_account_available(
    users: &Arc<dyn UserRepositoryPort>,
    groups: &Arc<dyn GroupStorePort>,
    username: &str,
    email: &str,
) -> Result<(), DomainError> {
    ensure_username_available(users, groups, username).await?;
    if users.find_by_email_ignore_case(email).await?.is_some() {
        return Err(DomainError::Conflict("email already in use".to_string()));
    }
    Ok(())
}

pub(crate) async fn ensure_username_available(
    users: &Arc<dyn UserRepositoryPort>,
    groups: &Arc<dyn GroupStorePort>,
    username: &str,
) -> Result<(), DomainError> {
    if users
        .find_by_username_ignore_case(username)
        .await?
        .is_some()
    {
        return Err(DomainError::Conflict("username already taken".to_string()));
    }
    if groups.find_child_by_name(None, username).await?.is_some() {
        return Err(DomainError::Conflict(
            "username collides with an existing root group".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_placeholder_is_a_valid_username_nobody_can_choose() {
        let placeholder = placeholder_username();

        assert!(is_placeholder_username(&placeholder), "{placeholder}");
        assert_eq!(placeholder.len(), "invite-".len() + 12);
        assert!(is_validation(normalize_username(&placeholder)));
        assert!(is_validation(normalize_username(
            &placeholder.to_uppercase()
        )));
        for chosen in [
            "invite-bob",
            "invite-0123456789ab0",
            "invite-0123456789aB",
            "invited",
        ] {
            assert!(!is_placeholder_username(chosen), "{chosen}");
        }
        assert_eq!(normalize_username("invite-bob").unwrap(), "invite-bob");
    }

    #[test]
    fn an_invitation_without_a_chosen_name_links_to_the_page_that_asks_for_it() {
        let token = "ab".repeat(32);

        assert_eq!(
            activation_url("https://git.example.com", "invite-0123456789ab", &token),
            format!("https://git.example.com/invitation#token={token}")
        );
        assert_eq!(
            activation_url("https://git.example.com", "marie", &token),
            format!("https://git.example.com/activate#token={token}")
        );
    }

    fn is_validation<T: std::fmt::Debug>(result: Result<T, DomainError>) -> bool {
        matches!(result, Err(DomainError::Validation(_)))
    }

    #[test]
    fn only_64_hex_characters_are_shaped_like_an_invitation_token() {
        assert!(is_invitation_token_shaped(&generate_invitation_token()));
        assert!(is_invitation_token_shaped(&"a".repeat(64)));
        assert!(is_invitation_token_shaped(
            &"0123456789abcdefABCDEF".repeat(3)[..64]
        ));
        for bad in [
            "",
            "abc",
            &"a".repeat(63),
            &"a".repeat(65),
            &"g".repeat(64),
            &format!("{} ", "a".repeat(63)),
            &format!("{}é", "a".repeat(62)),
            &"a".repeat(1_000_000),
        ] {
            assert!(
                !is_invitation_token_shaped(bad),
                "{:?}",
                bad.chars().take(20).collect::<String>()
            );
        }
    }

    #[tokio::test]
    async fn hash_blocking_hashes_with_the_hasher_on_another_thread() {
        let hasher = Arc::new(crate::test_support::ThreadRecordingHasher::default());
        let port: Arc<dyn PasswordHasherPort> = hasher.clone();

        let hash = hash_blocking(&port, "secret".to_string()).await.unwrap();

        assert_eq!(hash, "hashed:secret");
        let threads = hasher.hash_threads();
        assert_eq!(threads.len(), 1);
        assert_ne!(
            threads[0],
            std::thread::current().id(),
            "argon2 must not run on the calling (async) thread"
        );
    }

    #[test]
    fn a_username_is_trimmed_and_lower_cased() {
        assert_eq!(normalize_username("  Alice ").unwrap(), "alice");
    }

    #[test]
    fn a_username_may_contain_digits_underscores_and_hyphens_after_the_first_letter() {
        assert_eq!(normalize_username("a_b-c1").unwrap(), "a_b-c1");
        assert_eq!(normalize_username("abc").unwrap(), "abc");
        assert_eq!(normalize_username(&"a".repeat(32)).unwrap(), "a".repeat(32));
    }

    #[test]
    fn a_username_of_the_wrong_length_is_refused() {
        assert!(is_validation(normalize_username("ab")));
        assert!(is_validation(normalize_username("")));
        assert!(is_validation(normalize_username("   ")));
        assert!(is_validation(normalize_username(&"a".repeat(33))));
    }

    #[test]
    fn a_username_must_start_with_a_letter() {
        for bad in ["1abc", "_abc", "-abc"] {
            assert!(
                is_validation(normalize_username(bad)),
                "{bad} should be refused"
            );
        }
    }

    #[test]
    fn a_username_outside_the_ascii_charset_is_refused() {
        for bad in [
            "a b", "a.git", "abc.", "éclair", "abé", "a/b", "a@b", "a\tb",
        ] {
            assert!(
                is_validation(normalize_username(bad)),
                "{bad:?} should be refused"
            );
        }
    }

    #[test]
    fn every_reserved_name_is_refused_in_every_casing() {
        assert!(!RESERVED_USERNAMES.is_empty());
        for name in RESERVED_USERNAMES {
            for variant in [
                name.to_string(),
                name.to_uppercase(),
                name.to_lowercase(),
                format!("{}{}", name[..1].to_uppercase(), &name[1..]),
            ] {
                assert!(
                    is_validation(normalize_username(&variant)),
                    "{variant} should be reserved"
                );
            }
        }
        assert!(is_validation(normalize_username("Admin")));
        assert!(is_validation(normalize_username("LOGIN")));
    }

    #[test]
    fn reserved_names_are_stored_lower_case_in_the_username_charset() {
        for name in RESERVED_USERNAMES {
            assert_eq!(
                *name,
                name.to_lowercase(),
                "{name} must be stored lower-case"
            );
            assert!(
                name.chars()
                    .all(|c| c.is_ascii_lowercase() || c == '-' || c == '_'),
                "{name} could never be a username anyway"
            );
        }
    }

    #[test]
    fn reserved_names_cover_every_frontend_route() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../frontend/src/app/app.routes.ts"
        );
        let routes =
            std::fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
        let mut segments = vec![];
        for line in routes.lines() {
            let Some(rest) = line.split("path: '").nth(1) else {
                continue;
            };
            let Some(path) = rest.split('\'').next() else {
                continue;
            };
            let first = path.split('/').next().unwrap_or("");
            if !first.is_empty() && !first.starts_with(':') && first != "**" {
                segments.push(first.to_string());
            }
        }
        assert!(
            segments.len() >= 5,
            "the route parser found too few routes: {segments:?}"
        );
        for segment in segments {
            assert!(
                RESERVED_USERNAMES.contains(&segment.as_str()),
                "top-level route `{segment}` of app.routes.ts is missing from RESERVED_USERNAMES"
            );
        }
    }

    #[test]
    fn the_registration_and_activation_pages_are_reserved_before_their_routes_exist() {
        for name in [
            "login",
            "register",
            "activate",
            "reset-password",
            "home",
            "repositories",
            "groups",
            "account",
            "search",
            "runners",
            "admin",
            "api",
        ] {
            assert!(RESERVED_USERNAMES.contains(&name), "{name}");
        }
    }

    /// `/docs` serves the product documentation, so no account may take the name.
    #[test]
    fn the_documentation_is_reserved() {
        for name in ["docs", "Docs", " DOCS "] {
            assert!(
                is_validation(normalize_username(name)),
                "{name:?} should be refused"
            );
        }
    }

    /// The reserved-name check has to run on the lower-cased name, or a Unicode case fold could smuggle a reserved one in.
    #[test]
    fn a_unicode_fold_cannot_smuggle_a_reserved_name_through() {
        // U+212A (Kelvin sign) lower-cases to ASCII `k`, so `wi<K>i` would fold to the reserved `wiki`.
        assert!(is_validation(normalize_username("wi\u{212A}i")));
        // U+0130 (dotted capital I) lower-cases to `i` plus a combining dot, which isn't in the charset.
        assert!(is_validation(normalize_username("ADM\u{130}N")));
        // U+0131 (dotless i) isn't ASCII and stays that way.
        assert!(is_validation(normalize_username("adm\u{131}n")));
        assert!(is_validation(normalize_username("ａｄｍｉｎ")));
    }

    #[test]
    fn control_and_invisible_characters_are_refused() {
        for bad in [
            "abc\0",
            "\0abc",
            "ab\0c",
            "abc\u{200B}",
            "a\u{200B}bc",
            "abc\u{FEFF}",
            "ab\nc",
            "ab\u{7f}c",
        ] {
            assert!(
                is_validation(normalize_username(bad)),
                "{bad:?} should be refused"
            );
        }
    }

    #[test]
    fn the_kelvin_sign_fold_is_accepted_as_the_ascii_name_it_becomes() {
        assert_eq!(normalize_username("\u{212A}evin").unwrap(), "kevin");
    }

    #[test]
    fn a_password_needs_at_least_eight_bytes() {
        assert!(is_validation(validate_password("1234567")));
        assert!(is_validation(validate_password("")));
        assert!(validate_password("12345678").is_ok());
    }

    #[test]
    fn a_password_length_is_counted_in_bytes_like_the_existing_rule() {
        assert!(validate_password("éééé").is_ok());
        assert!(is_validation(validate_password("ééé")));
    }

    #[test]
    fn an_e_mail_is_trimmed_but_keeps_the_case_typed() {
        assert_eq!(
            normalize_email("  Alice@Example.com ").unwrap(),
            "Alice@Example.com"
        );
    }

    #[test]
    fn an_invalid_e_mail_is_refused() {
        for bad in [
            "",
            "alice",
            "alice@localhost",
            "a b@example.com",
            "@example.com",
            "a@@example.com",
        ] {
            assert!(
                is_validation(normalize_email(bad)),
                "{bad:?} should be refused"
            );
        }
    }

    #[test]
    fn an_invitation_token_is_64_hex_characters_and_never_repeats() {
        let first = generate_invitation_token();
        let second = generate_invitation_token();

        assert_eq!(first.len(), 64);
        assert!(
            first
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        assert_ne!(first, second);
    }
}
