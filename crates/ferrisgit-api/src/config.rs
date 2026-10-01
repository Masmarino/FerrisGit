use crate::client_ip::{Cidr, parse_cidr_list};

#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    pub jwt_secret: String,
    pub storage_root: String,
    pub bind_addr: String,
    pub static_dir: String,
    pub bootstrap_admin_username: Option<String>,
    pub bootstrap_admin_password: Option<String>,
    pub settings_encryption_key: [u8; 32],
    /// The public origin (`https://host[:port]`, no trailing slash or path); mail links are built from it.
    pub public_url: String,
    /// `TRUSTED_PROXY_CIDRS`. Empty by default: `X-Forwarded-For` is then never trusted. See `client_ip`.
    pub trusted_proxy_cidrs: Vec<Cidr>,
}

/// The result is the ASCII-serialised origin (`scheme://host[:port]`, lower-cased, default port dropped). The
/// error is the message the server panics with.
fn parse_public_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    // Refused before parsing: the WHATWG parser silently drops tabs/newlines and reads `\` as `/`.
    if trimmed
        .chars()
        .any(|c| c == '\\' || c.is_control() || c.is_whitespace())
    {
        return Err(
            "PUBLIC_URL must not contain whitespace, control characters or backslashes".to_string(),
        );
    }
    // Require a literal `://` (the parser would also accept `https:example.com`). The scheme can be in any case.
    let lower = trimmed.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err("PUBLIC_URL must start with http:// or https://".to_string());
    }
    let url = url::Url::parse(trimmed)
        .map_err(|error| format!("PUBLIC_URL is not a valid origin ({error})"))?;
    if url.host().is_none() {
        return Err("PUBLIC_URL must contain a host".to_string());
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("PUBLIC_URL must be a bare origin (scheme, host and optional port; no path, query, fragment or credentials)".to_string());
    }
    Ok(url.origin().ascii_serialization())
}

impl Config {
    pub fn from_env() -> Self {
        let jwt_secret = std::env::var("JWT_SECRET").expect("JWT_SECRET must be set");
        assert!(
            jwt_secret.len() >= 32,
            "JWT_SECRET must be at least 32 characters long, got {}",
            jwt_secret.len()
        );

        let settings_encryption_key_raw =
            std::env::var("SETTINGS_ENCRYPTION_KEY").expect("SETTINGS_ENCRYPTION_KEY must be set");
        assert_eq!(
            settings_encryption_key_raw.len(),
            32,
            "SETTINGS_ENCRYPTION_KEY must be exactly 32 bytes long, got {}",
            settings_encryption_key_raw.len()
        );
        let mut settings_encryption_key = [0u8; 32];
        settings_encryption_key.copy_from_slice(settings_encryption_key_raw.as_bytes());

        let public_url_raw = std::env::var("PUBLIC_URL").expect("PUBLIC_URL must be set");
        let public_url = parse_public_url(&public_url_raw)
            .unwrap_or_else(|reason| panic!("{reason}, got {public_url_raw:?}"));

        let trusted_proxy_cidrs =
            parse_cidr_list(&std::env::var("TRUSTED_PROXY_CIDRS").unwrap_or_default())
                .unwrap_or_else(|reason| panic!("TRUSTED_PROXY_CIDRS: {reason}"));

        Self {
            database_url: std::env::var("DATABASE_URL").expect("DATABASE_URL must be set"),
            jwt_secret,
            storage_root: std::env::var("STORAGE_ROOT").unwrap_or_else(|_| "./data".to_string()),
            bind_addr: std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string()),
            static_dir: std::env::var("STATIC_DIR").unwrap_or_else(|_| "./static".to_string()),
            bootstrap_admin_username: std::env::var("FERRISGIT_BOOTSTRAP_ADMIN_USERNAME")
                .ok()
                .filter(|s| !s.is_empty()),
            bootstrap_admin_password: std::env::var("FERRISGIT_BOOTSTRAP_ADMIN_PASSWORD")
                .ok()
                .filter(|s| !s.is_empty()),
            settings_encryption_key,
            public_url,
            trusted_proxy_cidrs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// These tests change the process environment, so they must not interleave.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        // A `should_panic` test poisons the lock on purpose.
        ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    #[should_panic(expected = "JWT_SECRET must be at least")]
    fn from_env_panics_on_a_too_short_jwt_secret() {
        let _guard = env_lock();
        unsafe {
            std::env::set_var("DATABASE_URL", "postgres://x");
            std::env::set_var("JWT_SECRET", "short");
        }
        let _ = Config::from_env();
    }

    #[test]
    #[should_panic(expected = "SETTINGS_ENCRYPTION_KEY must be exactly 32 bytes")]
    fn from_env_panics_on_a_wrong_length_settings_encryption_key() {
        let _guard = env_lock();
        unsafe {
            std::env::set_var("DATABASE_URL", "postgres://x");
            std::env::set_var("JWT_SECRET", "a".repeat(32));
            std::env::set_var("SETTINGS_ENCRYPTION_KEY", "too-short");
        }
        let _ = Config::from_env();
    }

    /// Sets the variables `from_env` reads before `PUBLIC_URL`, so the test reaches it.
    fn set_valid_env_except_public_url() {
        unsafe {
            std::env::set_var("DATABASE_URL", "postgres://x");
            std::env::set_var("JWT_SECRET", "a".repeat(32));
            std::env::set_var("SETTINGS_ENCRYPTION_KEY", "k".repeat(32));
        }
    }

    #[test]
    #[should_panic(expected = "PUBLIC_URL must be set")]
    fn from_env_panics_when_public_url_is_missing() {
        let _guard = env_lock();
        set_valid_env_except_public_url();
        unsafe { std::env::remove_var("PUBLIC_URL") };
        let _ = Config::from_env();
    }

    #[test]
    #[should_panic(expected = "PUBLIC_URL must start with http:// or https://")]
    fn from_env_panics_on_a_public_url_with_another_scheme() {
        let _guard = env_lock();
        set_valid_env_except_public_url();
        unsafe { std::env::set_var("PUBLIC_URL", "ftp://x") };
        let _ = Config::from_env();
    }

    #[test]
    #[should_panic(expected = "PUBLIC_URL must be a bare origin")]
    fn from_env_panics_on_a_public_url_with_a_path() {
        let _guard = env_lock();
        set_valid_env_except_public_url();
        unsafe { std::env::set_var("PUBLIC_URL", "https://app.example.com/ferrisgit") };
        let _ = Config::from_env();
    }

    #[test]
    fn from_env_accepts_a_valid_public_url_and_strips_the_trailing_slash() {
        let _guard = env_lock();
        set_valid_env_except_public_url();
        unsafe { std::env::set_var("PUBLIC_URL", "https://app.example.com/") };
        let config = Config::from_env();
        unsafe { std::env::remove_var("PUBLIC_URL") };
        assert_eq!(config.public_url, "https://app.example.com");
    }

    #[test]
    fn parse_public_url_accepts_bare_origins() {
        assert_eq!(
            parse_public_url("http://localhost:4200"),
            Ok("http://localhost:4200".to_string())
        );
        assert_eq!(
            parse_public_url("https://ferrisgit.example.com"),
            Ok("https://ferrisgit.example.com".to_string())
        );
        assert_eq!(
            parse_public_url("https://ferrisgit.example.com/"),
            Ok("https://ferrisgit.example.com".to_string())
        );
        assert_eq!(
            parse_public_url("  http://127.0.0.1:8080/ \n"),
            Ok("http://127.0.0.1:8080".to_string())
        );
        // The ASCII-serialised origin: lower-cased host and scheme, default port dropped, IPv6 kept in brackets.
        assert_eq!(
            parse_public_url("HTTPS://Example.COM"),
            Ok("https://example.com".to_string())
        );
        assert_eq!(
            parse_public_url("https://app.example.com:443"),
            Ok("https://app.example.com".to_string())
        );
        assert_eq!(
            parse_public_url("http://app.example.com:80/"),
            Ok("http://app.example.com".to_string())
        );
        assert_eq!(
            parse_public_url("http://[::1]:8080"),
            Ok("http://[::1]:8080".to_string())
        );
    }

    #[test]
    fn parse_public_url_rejects_everything_else() {
        for bad in [
            "",
            "localhost:4200",
            "ftp://x",
            "//example.com",
            "http://",
            "https:///",
            "https://example.com/app",
            "https://example.com//",
            "https://example.com?x=1",
            "https://example.com/?x=1",
            "https://example.com#frag",
            "https://user:pw@example.com",
            "https://exa mple.com",
            "https://:8080",
            "https://host:abc",
            "https://host:99999",
            "https://exa\tmple.com",
            "https://exa\u{0}mple.com",
            "https://exa\u{a0}mple.com",
            "https://host\\evil",
            "https://host\\",
            "https://user@example.com",
            "https://example.com/path/",
            "https://example.com?",
            "https://example.com#",
            "https:example.com",
        ] {
            assert!(parse_public_url(bad).is_err(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn trusted_proxy_cidrs_default_to_none() {
        let _guard = env_lock();
        set_valid_env_except_public_url();
        unsafe {
            std::env::set_var("PUBLIC_URL", "https://app.example.com");
            std::env::remove_var("TRUSTED_PROXY_CIDRS");
        }
        let unset = Config::from_env();
        unsafe { std::env::set_var("TRUSTED_PROXY_CIDRS", "  ") };
        let blank = Config::from_env();
        unsafe {
            std::env::remove_var("TRUSTED_PROXY_CIDRS");
            std::env::remove_var("PUBLIC_URL");
        }
        assert!(
            unset.trusted_proxy_cidrs.is_empty(),
            "X-Forwarded-For is never trusted unless configured"
        );
        assert!(blank.trusted_proxy_cidrs.is_empty());
    }

    #[test]
    fn trusted_proxy_cidrs_are_parsed_from_the_environment() {
        let _guard = env_lock();
        set_valid_env_except_public_url();
        unsafe {
            std::env::set_var("PUBLIC_URL", "https://app.example.com");
            std::env::set_var("TRUSTED_PROXY_CIDRS", "10.2.0.0/16, fd00::/8");
        }
        let config = Config::from_env();
        unsafe {
            std::env::remove_var("TRUSTED_PROXY_CIDRS");
            std::env::remove_var("PUBLIC_URL");
        }
        assert_eq!(config.trusted_proxy_cidrs.len(), 2);
        assert!(config.trusted_proxy_cidrs[0].contains("10.2.0.9".parse().unwrap()));
        assert!(config.trusted_proxy_cidrs[1].contains("fd00::1".parse().unwrap()));
    }

    #[test]
    #[should_panic(expected = "TRUSTED_PROXY_CIDRS: invalid entry \"10.2.0.0/40\"")]
    fn from_env_panics_naming_an_invalid_trusted_proxy_cidr() {
        let _guard = env_lock();
        set_valid_env_except_public_url();
        unsafe {
            std::env::set_var("PUBLIC_URL", "https://app.example.com");
            std::env::set_var("TRUSTED_PROXY_CIDRS", "10.2.0.0/16,10.2.0.0/40");
        }
        let _ = Config::from_env();
    }
}
