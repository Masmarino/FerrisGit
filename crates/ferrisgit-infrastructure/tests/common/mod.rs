//! Fakes and seeding shared by the `kind`-backed integration tests.

use async_trait::async_trait;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::pipeline_events::{JobEvent, PipelineEvent, PipelineEventPublisherPort};
use ferrisgit_domain::pipeline_file_reader::PipelineFileReaderPort;
use ferrisgit_domain::settings::{RepositorySettings, RepositorySettingsStorePort};
use ferrisgit_domain::webhook_dispatcher::WebhookDispatcherPort;
use ferrisgit_domain::webhook_event::WebhookEvent;
use uuid::Uuid;

pub struct FakeRepositorySettings;
#[async_trait]
impl RepositorySettingsStorePort for FakeRepositorySettings {
    async fn get_or_create_default(
        &self,
        repository_id: Uuid,
    ) -> Result<RepositorySettings, DomainError> {
        Ok(RepositorySettings {
            repository_id,
            pipeline_file_path: ".ferrisgit-ci.yml".to_string(),
            ci_enabled: true,
            required_approvals: 0,
        })
    }
    async fn update(
        &self,
        _repository_id: Uuid,
        _update: ferrisgit_domain::settings::RepositorySettingsUpdate,
    ) -> Result<RepositorySettings, DomainError> {
        unimplemented!()
    }
    async fn list_ci_variables(
        &self,
        _repository_id: Uuid,
    ) -> Result<Vec<ferrisgit_domain::settings::CiVariable>, DomainError> {
        unimplemented!()
    }
    async fn set_ci_variable(
        &self,
        _new_variable: ferrisgit_domain::settings::NewCiVariable,
    ) -> Result<ferrisgit_domain::settings::CiVariable, DomainError> {
        unimplemented!()
    }
    async fn delete_ci_variable(&self, _id: Uuid, _repository_id: Uuid) -> Result<(), DomainError> {
        unimplemented!()
    }
    async fn resolve_ci_variables_plaintext(
        &self,
        _repository_id: Uuid,
    ) -> Result<std::collections::BTreeMap<String, String>, DomainError> {
        unimplemented!()
    }
}

pub struct FakeFileReader(pub String);
#[async_trait]
impl PipelineFileReaderPort for FakeFileReader {
    async fn read_file_at_revision(
        &self,
        _repository_disk_path: &str,
        _commit_sha: &str,
        _path: &str,
    ) -> Result<Option<Vec<u8>>, DomainError> {
        Ok(Some(self.0.clone().into_bytes()))
    }
}

#[derive(Default)]
pub struct FakeEvents;
#[async_trait]
impl PipelineEventPublisherPort for FakeEvents {
    async fn publish_pipeline_event(
        &self,
        _pipeline_id: Uuid,
        _event: PipelineEvent,
    ) -> Result<(), DomainError> {
        Ok(())
    }
    async fn publish_job_event(&self, _job_id: Uuid, _event: JobEvent) -> Result<(), DomainError> {
        Ok(())
    }
}

pub struct FakeWebhooks;
#[async_trait]
impl WebhookDispatcherPort for FakeWebhooks {
    async fn dispatch(
        &self,
        _repository_id: Uuid,
        _event: WebhookEvent,
    ) -> Result<(), DomainError> {
        Ok(())
    }
}

/// Inserts a user and a repository directly: the foreign keys into `repositories`/`users` are plumbing
/// these tests aren't proving. Returns `(repository_id, owner_id)`.
pub async fn insert_repository(pool: &sqlx::PgPool) -> (Uuid, Uuid) {
    let repository_id = Uuid::new_v4();
    let owner_id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, username, email, password_hash) VALUES ($1, $2, $3, $4)")
        .bind(owner_id)
        .bind(format!("owner-{owner_id}"))
        .bind(format!("owner-{owner_id}@example.com"))
        .bind("not-a-real-hash")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO repositories (id, owner_id, name, disk_path) VALUES ($1, $2, $3, $4)")
        .bind(repository_id)
        .bind(owner_id)
        .bind(format!("repo-{repository_id}"))
        .bind("unused-disk-path")
        .execute(pool)
        .await
        .unwrap();
    (repository_id, owner_id)
}
