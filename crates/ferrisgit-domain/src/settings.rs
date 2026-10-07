use async_trait::async_trait;
use serde::Serialize;
use uuid::Uuid;

use crate::error::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionEngine {
    DockerRunners,
    Kubernetes,
}

impl ExecutionEngine {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecutionEngine::DockerRunners => "docker-runners",
            ExecutionEngine::Kubernetes => "kubernetes",
        }
    }

    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "docker-runners" => Ok(ExecutionEngine::DockerRunners),
            "kubernetes" => Ok(ExecutionEngine::Kubernetes),
            other => Err(DomainError::Validation(format!(
                "unknown execution engine: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SystemSettings {
    pub execution_engine: ExecutionEngine,
    pub k8s_namespace: Option<String>,
    pub k8s_cache_storage_class: Option<String>,
    pub runner_registration_token: Option<String>,
    pub log_retention_days: Option<i32>,
    pub max_concurrent_jobs: Option<i32>,
    pub jwt_ttl_hours: i32,
    pub max_push_size_mb: i32,
}

/// `None` leaves a field alone. Clearing an optional field like `k8s_namespace` takes `Some(None)`, hence the nested
/// options.
#[derive(Debug, Default)]
pub struct SystemSettingsUpdate {
    pub execution_engine: Option<ExecutionEngine>,
    pub k8s_namespace: Option<Option<String>>,
    pub k8s_cache_storage_class: Option<Option<String>>,
    pub runner_registration_token: Option<Option<String>>,
    pub log_retention_days: Option<Option<i32>>,
    pub max_concurrent_jobs: Option<Option<i32>>,
    pub jwt_ttl_hours: Option<i32>,
    pub max_push_size_mb: Option<i32>,
}

#[async_trait]
pub trait SystemSettingsStorePort: Send + Sync {
    async fn get(&self) -> Result<SystemSettings, DomainError>;
    async fn update(&self, update: SystemSettingsUpdate) -> Result<SystemSettings, DomainError>;
}

#[derive(Debug, Clone, Serialize)]
pub struct RepositorySettings {
    pub repository_id: Uuid,
    pub pipeline_file_path: String,
    pub ci_enabled: bool,
    pub required_approvals: i32,
}

#[derive(Debug, Default)]
pub struct RepositorySettingsUpdate {
    pub pipeline_file_path: Option<String>,
    pub ci_enabled: Option<bool>,
    pub required_approvals: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CiVariable {
    pub id: Uuid,
    pub repository_id: Uuid,
    pub key: String,
    pub masked: bool,
}

/// A name a shell can export, so one a CI variable can have: ASCII letters, digits and `_`, no digit first. The web
/// form says the same before sending (ci-variable-name.ts); a name that is not one would never reach a job's script.
pub fn is_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub struct NewCiVariable {
    pub repository_id: Uuid,
    pub key: String,
    pub plaintext_value: String,
    pub masked: bool,
}

#[async_trait]
pub trait RepositorySettingsStorePort: Send + Sync {
    async fn get_or_create_default(
        &self,
        repository_id: Uuid,
    ) -> Result<RepositorySettings, DomainError>;
    async fn update(
        &self,
        repository_id: Uuid,
        update: RepositorySettingsUpdate,
    ) -> Result<RepositorySettings, DomainError>;
    async fn list_ci_variables(&self, repository_id: Uuid) -> Result<Vec<CiVariable>, DomainError>;
    async fn set_ci_variable(&self, new_variable: NewCiVariable)
    -> Result<CiVariable, DomainError>;
    async fn delete_ci_variable(&self, id: Uuid, repository_id: Uuid) -> Result<(), DomainError>;
    /// Decrypts a repository's CI variables to inject into a job's environment. The only place plaintext is rebuilt.
    async fn resolve_ci_variables_plaintext(
        &self,
        repository_id: Uuid,
    ) -> Result<std::collections::BTreeMap<String, String>, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_engine_round_trips_through_its_string_form() {
        assert_eq!(
            ExecutionEngine::parse("kubernetes").unwrap(),
            ExecutionEngine::Kubernetes
        );
        assert_eq!(ExecutionEngine::DockerRunners.as_str(), "docker-runners");
    }

    #[test]
    fn an_env_name_is_letters_digits_and_underscores_with_no_digit_first() {
        for name in ["A", "_", "DATABASE_URL", "x1", "_9"] {
            assert!(is_env_name(name), "{name}");
        }
        for name in ["", "1A", "MY-VAR", "MY VAR", "A=B", "ÉTÉ", "A\n"] {
            assert!(!is_env_name(name), "{name:?}");
        }
    }

    #[test]
    fn parsing_an_unknown_engine_is_a_validation_error() {
        assert!(matches!(
            ExecutionEngine::parse("bogus"),
            Err(DomainError::Validation(_))
        ));
    }
}
