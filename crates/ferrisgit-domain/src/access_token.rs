use std::fmt;
use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::DomainError;

/// What identifies an API token in the Authorization header, ahead of any lookup. A session JWT never starts with it.
pub const ACCESS_TOKEN_PREFIX: &str = "fga_";

/// A token always expires, and a year is as long as it gets.
pub const MAX_LIFETIME_DAYS: i64 = 365;
const MAX_NAME_CHARS: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScopeArea {
    Repositories,
    Issues,
    MergeRequests,
    Pipelines,
    Releases,
    Admin,
}

impl ScopeArea {
    pub const ALL: [ScopeArea; 6] = [
        ScopeArea::Repositories,
        ScopeArea::Issues,
        ScopeArea::MergeRequests,
        ScopeArea::Pipelines,
        ScopeArea::Releases,
        ScopeArea::Admin,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ScopeArea::Repositories => "repositories",
            ScopeArea::Issues => "issues",
            ScopeArea::MergeRequests => "merge-requests",
            ScopeArea::Pipelines => "pipelines",
            ScopeArea::Releases => "releases",
            ScopeArea::Admin => "admin",
        }
    }
}

/// Ordered: `write` includes `read`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ScopeLevel {
    Read,
    Write,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TokenScope {
    pub area: ScopeArea,
    pub level: ScopeLevel,
}

impl TokenScope {
    pub const fn new(area: ScopeArea, level: ScopeLevel) -> Self {
        Self { area, level }
    }

    pub fn covers(self, required: TokenScope) -> bool {
        self.area == required.area && self.level >= required.level
    }
}

impl fmt::Display for TokenScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let level = match self.level {
            ScopeLevel::Read => "read",
            ScopeLevel::Write => "write",
        };
        write!(f, "{}:{level}", self.area.as_str())
    }
}

impl FromStr for TokenScope {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let unknown = || DomainError::Validation(format!("unknown scope: {value}"));
        let (area, level) = value.split_once(':').ok_or_else(unknown)?;
        let area = ScopeArea::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == area)
            .ok_or_else(unknown)?;
        let level = match level {
            "read" => ScopeLevel::Read,
            "write" => ScopeLevel::Write,
            _ => return Err(unknown()),
        };
        Ok(TokenScope { area, level })
    }
}

/// Scopes from a request: at least one, each known, repeats counted once.
pub fn parse_requested_scopes(values: &[String]) -> Result<Vec<TokenScope>, DomainError> {
    if values.is_empty() {
        return Err(DomainError::Validation(
            "at least one scope is required".to_string(),
        ));
    }
    let mut scopes = Vec::with_capacity(values.len());
    for value in values {
        let scope: TokenScope = value.parse()?;
        if !scopes.contains(&scope) {
            scopes.push(scope);
        }
    }
    Ok(scopes)
}

/// `AdminUser` would refuse such a token anyway, but a later promotion must not wake up an old one.
pub fn ensure_owner_can_hold(scopes: &[TokenScope], is_admin: bool) -> Result<(), DomainError> {
    if !is_admin && scopes.iter().any(|s| s.area == ScopeArea::Admin) {
        return Err(DomainError::Validation(
            "only an administrator can create a token with the admin scope".to_string(),
        ));
    }
    Ok(())
}

pub fn validate_token_name(name: &str) -> Result<String, DomainError> {
    let name = name.trim();
    if name.is_empty()
        || name.chars().count() > MAX_NAME_CHARS
        || name.chars().any(char::is_control)
    {
        return Err(DomainError::Validation(format!(
            "a token name takes 1 to {MAX_NAME_CHARS} characters, without control characters"
        )));
    }
    Ok(name.to_string())
}

pub fn validate_lifetime_days(days: i64) -> Result<i64, DomainError> {
    if !(1..=MAX_LIFETIME_DAYS).contains(&days) {
        return Err(DomainError::Validation(format!(
            "a token lives between 1 and {MAX_LIFETIME_DAYS} days"
        )));
    }
    Ok(days)
}

#[derive(Debug, Clone)]
pub struct AccessToken {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub token_hash: String,
    pub scopes: Vec<TokenScope>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

impl AccessToken {
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.expires_at <= now
    }

    pub fn allows(&self, required: TokenScope) -> bool {
        self.scopes.iter().any(|scope| scope.covers(required))
    }

    pub fn has_area(&self, area: ScopeArea) -> bool {
        self.scopes.iter().any(|scope| scope.area == area)
    }
}

pub struct NewAccessToken {
    pub user_id: Uuid,
    pub name: String,
    pub token_hash: String,
    pub scopes: Vec<TokenScope>,
    pub expires_at: DateTime<Utc>,
}

#[async_trait]
pub trait AccessTokenRepositoryPort: Send + Sync {
    async fn create(&self, new_token: NewAccessToken) -> Result<AccessToken, DomainError>;
    async fn list_for_user(&self, user_id: Uuid) -> Result<Vec<AccessToken>, DomainError>;
    async fn find_by_hash(&self, token_hash: &str) -> Result<Option<AccessToken>, DomainError>;
    async fn revoke(&self, id: Uuid, user_id: Uuid) -> Result<(), DomainError>;
    /// At most one write a minute per token, since every API call lands here.
    async fn touch_last_used(&self, id: Uuid) -> Result<(), DomainError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceAuthorizationStatus {
    Pending,
    Approved,
    Denied,
}

impl DeviceAuthorizationStatus {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "pending" => Ok(Self::Pending),
            "approved" => Ok(Self::Approved),
            "denied" => Ok(Self::Denied),
            other => Err(DomainError::Infrastructure(format!(
                "unknown device authorization status: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DeviceAuthorization {
    pub id: Uuid,
    pub user_code: String,
    pub client_name: String,
    pub scopes: Vec<TokenScope>,
    pub token_lifetime_days: i32,
    pub status: DeviceAuthorizationStatus,
    pub user_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

pub struct NewDeviceAuthorization {
    pub device_code_hash: String,
    pub user_code: String,
    pub client_name: String,
    pub scopes: Vec<TokenScope>,
    pub token_lifetime_days: i32,
    pub expires_at: DateTime<Utc>,
}

/// What a terminal's poll found. `Approved` hands the request over once: it is gone afterwards.
#[derive(Debug)]
pub enum DevicePoll {
    Pending,
    TooFast,
    Denied,
    /// Unknown, expired or already collected: nothing tells them apart.
    Expired,
    Approved(DeviceAuthorization),
}

#[async_trait]
pub trait DeviceAuthorizationPort: Send + Sync {
    async fn create(&self, new_request: NewDeviceAuthorization) -> Result<(), DomainError>;
    async fn find_pending_by_user_code(
        &self,
        user_code: &str,
    ) -> Result<Option<DeviceAuthorization>, DomainError>;
    /// Only a pending, unexpired request moves. False means it no longer can.
    async fn approve(&self, user_code: &str, user_id: Uuid) -> Result<bool, DomainError>;
    async fn deny(&self, user_code: &str) -> Result<bool, DomainError>;
    async fn poll(
        &self,
        device_code_hash: &str,
        min_interval_seconds: i32,
    ) -> Result<DevicePoll, DomainError>;
    async fn delete_expired(&self) -> Result<(), DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(value: &str) -> TokenScope {
        value.parse().unwrap()
    }

    fn token_with(scopes: &[&str]) -> AccessToken {
        AccessToken {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            name: "ci".to_string(),
            token_hash: "h".to_string(),
            scopes: scopes.iter().map(|s| scope(s)).collect(),
            expires_at: Utc::now() + chrono::Duration::days(1),
            created_at: Utc::now(),
            last_used_at: None,
        }
    }

    #[test]
    fn every_scope_reads_back_from_what_it_prints() {
        for area in ScopeArea::ALL {
            for level in [ScopeLevel::Read, ScopeLevel::Write] {
                let original = TokenScope::new(area, level);
                assert_eq!(scope(&original.to_string()), original);
            }
        }
    }

    #[test]
    fn write_includes_read_in_the_same_area_only() {
        let token = token_with(&["issues:write"]);
        assert!(token.allows(scope("issues:read")));
        assert!(token.allows(scope("issues:write")));
        assert!(!token.allows(scope("merge-requests:read")));
    }

    #[test]
    fn read_does_not_include_write() {
        assert!(!token_with(&["issues:read"]).allows(scope("issues:write")));
    }

    #[test]
    fn an_unknown_scope_is_refused_rather_than_ignored() {
        for value in [
            "issues",
            "issues:admin",
            "wiki:read",
            "",
            ":",
            "ISSUES:READ",
        ] {
            assert!(
                parse_requested_scopes(&[value.to_string()]).is_err(),
                "{value}"
            );
        }
    }

    #[test]
    fn at_least_one_scope_is_required() {
        assert!(parse_requested_scopes(&[]).is_err());
    }

    #[test]
    fn a_repeated_scope_counts_once() {
        let parsed =
            parse_requested_scopes(&["issues:read".to_string(), "issues:read".to_string()])
                .unwrap();
        assert_eq!(parsed.len(), 1);
    }

    #[test]
    fn only_an_administrator_can_hold_the_admin_scope() {
        let admin = [scope("admin:read")];
        assert!(ensure_owner_can_hold(&admin, false).is_err());
        assert!(ensure_owner_can_hold(&admin, true).is_ok());
        assert!(ensure_owner_can_hold(&[scope("issues:write")], false).is_ok());
    }

    #[test]
    fn the_lifetime_stays_within_a_year() {
        assert!(validate_lifetime_days(0).is_err());
        assert!(validate_lifetime_days(366).is_err());
        assert_eq!(validate_lifetime_days(365).unwrap(), 365);
    }

    #[test]
    fn a_name_is_trimmed_and_must_be_printable() {
        assert_eq!(validate_token_name("  ci ").unwrap(), "ci");
        assert!(validate_token_name("   ").is_err());
        assert!(validate_token_name("a\u{1b}[2Jb").is_err());
        assert!(validate_token_name(&"x".repeat(101)).is_err());
    }

    #[test]
    fn a_token_is_expired_from_its_expiry_on() {
        let token = token_with(&["issues:read"]);
        assert!(!token.is_expired(token.expires_at - chrono::Duration::seconds(1)));
        assert!(token.is_expired(token.expires_at));
    }
}
