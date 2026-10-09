use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use rand::Rng;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::GroupStorePort;
use ferrisgit_domain::job::Job;
use ferrisgit_domain::job_containers::{DockerInJobsPort, JobCloneTokenPort};
use ferrisgit_domain::pipeline::PipelineStorePort;
use ferrisgit_domain::repository::{Repository, RepositoryStorePort};
use ferrisgit_domain::settings::RepositorySettingsStorePort;
use ferrisgit_domain::user::UserRepositoryPort;
use uuid::Uuid;

use crate::token_hash::hash_token;

/// What a job needs from the server to run, on either engine: the commit to clone and where from, the repository's CI
/// variables, the values to hide in its logs, and whether it gets a Docker daemon.
#[derive(Debug, Clone, PartialEq)]
pub struct JobEnvironment {
    pub repository_id: Uuid,
    /// The repository as a clone URL names it: `owner/name`, or `group/subgroup/name`.
    pub repository_path: String,
    pub commit_sha: String,
    pub ci_variables: BTreeMap<String, String>,
    /// The values of the masked CI variables: a log line never shows them.
    pub masked_values: Vec<String>,
    pub docker: bool,
}

/// `owner/name` for a personal repository, the group's path then the name for a group's.
pub async fn repository_path(
    users: &dyn UserRepositoryPort,
    groups: &dyn GroupStorePort,
    repository: &Repository,
) -> Result<String, DomainError> {
    let mut segments: Vec<String> = match repository.group_id {
        Some(group_id) => groups
            .ancestor_chain(group_id)
            .await?
            .into_iter()
            .map(|group| group.name)
            .collect(),
        None => vec![
            users
                .find_by_id(repository.owner_id)
                .await?
                .ok_or_else(|| DomainError::NotFound("user".to_string()))?
                .username,
        ],
    };
    segments.push(repository.name.clone());
    Ok(segments.join("/"))
}

/// What an engine asks the server about a job it is about to run.
#[async_trait]
pub trait JobEnvironmentSource: Send + Sync {
    async fn environment(&self, job: &Job) -> Result<JobEnvironment, DomainError>;
    /// A new token to clone the job's repository with, replacing any earlier one. Only its hash is kept; it reads that
    /// repository only, until the job finishes.
    async fn issue_clone_token(&self, job_id: Uuid) -> Result<String, DomainError>;
}

pub struct JobEnvironments {
    pipelines: Arc<dyn PipelineStorePort>,
    repositories: Arc<dyn RepositoryStorePort>,
    users: Arc<dyn UserRepositoryPort>,
    groups: Arc<dyn GroupStorePort>,
    repository_settings: Arc<dyn RepositorySettingsStorePort>,
    docker: Arc<dyn DockerInJobsPort>,
    clone_tokens: Arc<dyn JobCloneTokenPort>,
}

impl JobEnvironments {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pipelines: Arc<dyn PipelineStorePort>,
        repositories: Arc<dyn RepositoryStorePort>,
        users: Arc<dyn UserRepositoryPort>,
        groups: Arc<dyn GroupStorePort>,
        repository_settings: Arc<dyn RepositorySettingsStorePort>,
        docker: Arc<dyn DockerInJobsPort>,
        clone_tokens: Arc<dyn JobCloneTokenPort>,
    ) -> Self {
        Self {
            pipelines,
            repositories,
            users,
            groups,
            repository_settings,
            docker,
            clone_tokens,
        }
    }
}

#[async_trait]
impl JobEnvironmentSource for JobEnvironments {
    async fn issue_clone_token(&self, job_id: Uuid) -> Result<String, DomainError> {
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        let token = format!("fgj_{}", hex::encode(bytes));
        self.clone_tokens.issue(job_id, &hash_token(&token)).await?;
        Ok(token)
    }

    async fn environment(&self, job: &Job) -> Result<JobEnvironment, DomainError> {
        let pipeline = self
            .pipelines
            .find_by_id(job.pipeline_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("pipeline".to_string()))?;
        let repository = self
            .repositories
            .find_by_id(pipeline.repository_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("repository".to_string()))?;
        let ci_variables = self
            .repository_settings
            .resolve_ci_variables_plaintext(repository.id)
            .await?;
        let masked_values = self
            .repository_settings
            .list_ci_variables(repository.id)
            .await?
            .into_iter()
            .filter(|variable| variable.masked)
            .filter_map(|variable| ci_variables.get(&variable.key).cloned())
            .collect();
        Ok(JobEnvironment {
            repository_id: repository.id,
            repository_path: repository_path(
                self.users.as_ref(),
                self.groups.as_ref(),
                &repository,
            )
            .await?,
            commit_sha: pipeline.commit_sha,
            ci_variables,
            masked_values,
            docker: self.docker.is_enabled().await?,
        })
    }
}

/// Every masked value in `text` replaced with `***`, longest first so that one holding another is hidden whole.
pub fn mask_values(text: &str, masked_values: &[String]) -> String {
    let mut values: Vec<&String> = masked_values.iter().filter(|v| !v.is_empty()).collect();
    values.sort_by_key(|value| std::cmp::Reverse(value.len()));
    values.into_iter().fold(text.to_string(), |text, value| {
        text.replace(value.as_str(), "***")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeGroups, FakeUsers};
    use crate::use_cases::fixtures::{group, repository, user};

    #[tokio::test]
    async fn a_personal_repository_is_under_its_owner_and_a_group_one_under_the_groups_path() {
        let alice = user("alice");
        let acme = group(None, "acme");
        let team = group(Some(acme.id), "team");
        let users = FakeUsers::new(vec![alice.clone()]);
        let groups = FakeGroups::new(vec![acme, team.clone()]);

        let personal = Repository {
            name: "hello".to_string(),
            ..repository(alice.id)
        };
        let shared = Repository {
            name: "widget".to_string(),
            group_id: Some(team.id),
            ..repository(alice.id)
        };

        assert_eq!(
            repository_path(&users, &groups, &personal).await.unwrap(),
            "alice/hello"
        );
        assert_eq!(
            repository_path(&users, &groups, &shared).await.unwrap(),
            "acme/team/widget"
        );
    }

    #[test]
    fn masking_hides_every_value_whole_even_when_one_holds_another() {
        let masked = vec!["abc".to_string(), "abcdef".to_string(), String::new()];

        assert_eq!(
            mask_values("token=abcdef, short=abc, none", &masked),
            "token=***, short=***, none"
        );
    }
}
