use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::repository::{
    NewRepository, Repository, RepositoryStorePort, RepositoryVisibility,
};
use ferrisgit_domain::settings::{RepositorySettingsStorePort, RepositorySettingsUpdate};
use uuid::Uuid;

pub struct CreateRepositoryUseCase {
    repositories: Arc<dyn RepositoryStorePort>,
    settings: Arc<dyn RepositorySettingsStorePort>,
}

impl CreateRepositoryUseCase {
    pub fn new(
        repositories: Arc<dyn RepositoryStorePort>,
        settings: Arc<dyn RepositorySettingsStorePort>,
    ) -> Self {
        Self {
            repositories,
            settings,
        }
    }

    /// `init_disk_repo` gets the disk path (`{owner_id}/{name}.git`) before the row is persisted, so a disk-init
    /// failure never leaves an orphaned row. `settings_update` applies on top of the default settings (created lazily).
    /// An all-`None` update is a no-op.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        &self,
        owner_id: Uuid,
        name: String,
        description: String,
        group_id: Option<Uuid>,
        visibility: RepositoryVisibility,
        settings_update: RepositorySettingsUpdate,
        init_disk_repo: impl FnOnce(&str) -> std::io::Result<()>,
    ) -> Result<Repository, DomainError> {
        if name.trim().is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(DomainError::Validation(
                "repository name must be non-empty and alphanumeric/-/_ only".to_string(),
            ));
        }

        match group_id {
            None => {
                if self
                    .repositories
                    .find_by_owner_and_name(owner_id, &name)
                    .await?
                    .is_some()
                {
                    return Err(DomainError::Conflict(format!(
                        "a repository named '{name}' already exists"
                    )));
                }
            }
            Some(gid) => {
                if self
                    .repositories
                    .find_by_group_and_name(gid, &name)
                    .await?
                    .is_some()
                {
                    return Err(DomainError::Conflict(format!(
                        "a repository named '{name}' already exists in this group"
                    )));
                }
            }
        }

        let disk_path = match group_id {
            None => format!("{owner_id}/{name}.git"),
            Some(gid) => format!("groups/{gid}/{name}.git"),
        };
        init_disk_repo(&disk_path).map_err(|e| DomainError::Infrastructure(e.to_string()))?;

        let repo = self
            .repositories
            .create(
                NewRepository {
                    owner_id,
                    name,
                    description,
                    group_id,
                    visibility,
                },
                disk_path,
            )
            .await?;

        self.settings.get_or_create_default(repo.id).await?;
        let has_overrides = settings_update.pipeline_file_path.is_some()
            || settings_update.ci_enabled.is_some()
            || settings_update.required_approvals.is_some();
        if has_overrides {
            self.settings.update(repo.id, settings_update).await?;
        }

        Ok(repo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use ferrisgit_domain::settings::{CiVariable, NewCiVariable, RepositorySettings};
    use std::sync::Mutex;

    use crate::test_support::FakeRepositories;

    #[derive(Default)]
    struct FakeSettings(Mutex<Vec<RepositorySettings>>);
    #[async_trait]
    impl RepositorySettingsStorePort for FakeSettings {
        async fn get_or_create_default(
            &self,
            repository_id: Uuid,
        ) -> Result<RepositorySettings, DomainError> {
            let mut rows = self.0.lock().unwrap();
            if let Some(existing) = rows.iter().find(|s| s.repository_id == repository_id) {
                return Ok(existing.clone());
            }
            let defaults = RepositorySettings {
                repository_id,
                pipeline_file_path: ".ferrisgit-ci.yml".to_string(),
                ci_enabled: true,
                required_approvals: 0,
            };
            rows.push(defaults.clone());
            Ok(defaults)
        }
        async fn update(
            &self,
            repository_id: Uuid,
            update: RepositorySettingsUpdate,
        ) -> Result<RepositorySettings, DomainError> {
            let mut rows = self.0.lock().unwrap();
            let row = rows
                .iter_mut()
                .find(|s| s.repository_id == repository_id)
                .expect("get_or_create_default called first");
            if let Some(path) = update.pipeline_file_path {
                row.pipeline_file_path = path;
            }
            if let Some(enabled) = update.ci_enabled {
                row.ci_enabled = enabled;
            }
            if let Some(approvals) = update.required_approvals {
                row.required_approvals = approvals;
            }
            Ok(row.clone())
        }
        async fn list_ci_variables(
            &self,
            _repository_id: Uuid,
        ) -> Result<Vec<CiVariable>, DomainError> {
            unimplemented!("not used by CreateRepositoryUseCase")
        }
        async fn set_ci_variable(
            &self,
            _new_variable: NewCiVariable,
        ) -> Result<CiVariable, DomainError> {
            unimplemented!("not used by CreateRepositoryUseCase")
        }
        async fn delete_ci_variable(
            &self,
            _id: Uuid,
            _repository_id: Uuid,
        ) -> Result<(), DomainError> {
            unimplemented!("not used by CreateRepositoryUseCase")
        }
        async fn resolve_ci_variables_plaintext(
            &self,
            _repository_id: Uuid,
        ) -> Result<std::collections::BTreeMap<String, String>, DomainError> {
            unimplemented!("not used by CreateRepositoryUseCase")
        }
    }

    #[tokio::test]
    async fn creating_a_repository_initializes_disk_before_persisting() {
        let repo_store = Arc::new(FakeRepositories::empty());
        let use_case =
            CreateRepositoryUseCase::new(repo_store.clone(), Arc::new(FakeSettings::default()));
        let owner_id = Uuid::new_v4();
        let mut init_called_with = None;

        let created = use_case
            .execute(
                owner_id,
                "hello".to_string(),
                String::new(),
                None,
                RepositoryVisibility::Private,
                RepositorySettingsUpdate::default(),
                |disk_path| {
                    init_called_with = Some(disk_path.to_string());
                    Ok(())
                },
            )
            .await
            .unwrap();

        assert_eq!(init_called_with, Some(format!("{owner_id}/hello.git")));
        assert_eq!(created.disk_path, format!("{owner_id}/hello.git"));
    }

    #[tokio::test]
    async fn creating_a_repository_persists_its_description() {
        let repo_store = Arc::new(FakeRepositories::empty());
        let use_case = CreateRepositoryUseCase::new(repo_store, Arc::new(FakeSettings::default()));

        let created = use_case
            .execute(
                Uuid::new_v4(),
                "hello".to_string(),
                "a test repo".to_string(),
                None,
                RepositoryVisibility::Private,
                RepositorySettingsUpdate::default(),
                |_| Ok(()),
            )
            .await
            .unwrap();

        assert_eq!(created.description, "a test repo");
    }

    #[tokio::test]
    async fn creating_a_repository_applies_settings_overrides() {
        let repo_store = Arc::new(FakeRepositories::empty());
        let settings = Arc::new(FakeSettings::default());
        let use_case = CreateRepositoryUseCase::new(repo_store, settings.clone());
        let overrides = RepositorySettingsUpdate {
            ci_enabled: Some(false),
            required_approvals: Some(2),
            pipeline_file_path: Some("custom.yml".to_string()),
        };

        let created = use_case
            .execute(
                Uuid::new_v4(),
                "hello".to_string(),
                String::new(),
                None,
                RepositoryVisibility::Private,
                overrides,
                |_| Ok(()),
            )
            .await
            .unwrap();

        let saved = settings
            .0
            .lock()
            .unwrap()
            .iter()
            .find(|s| s.repository_id == created.id)
            .unwrap()
            .clone();
        assert!(!saved.ci_enabled);
        assert_eq!(saved.required_approvals, 2);
        assert_eq!(saved.pipeline_file_path, "custom.yml");
    }

    #[tokio::test]
    async fn creating_a_repository_with_no_overrides_keeps_the_defaults() {
        let repo_store = Arc::new(FakeRepositories::empty());
        let settings = Arc::new(FakeSettings::default());
        let use_case = CreateRepositoryUseCase::new(repo_store, settings.clone());

        let created = use_case
            .execute(
                Uuid::new_v4(),
                "hello".to_string(),
                String::new(),
                None,
                RepositoryVisibility::Private,
                RepositorySettingsUpdate::default(),
                |_| Ok(()),
            )
            .await
            .unwrap();

        let saved = settings
            .0
            .lock()
            .unwrap()
            .iter()
            .find(|s| s.repository_id == created.id)
            .unwrap()
            .clone();
        assert!(saved.ci_enabled);
        assert_eq!(saved.required_approvals, 0);
        assert_eq!(saved.pipeline_file_path, ".ferrisgit-ci.yml");
    }

    #[tokio::test]
    async fn a_disk_init_failure_prevents_the_repository_from_being_persisted() {
        let repo_store = Arc::new(FakeRepositories::empty());
        let use_case =
            CreateRepositoryUseCase::new(repo_store.clone(), Arc::new(FakeSettings::default()));

        let result = use_case
            .execute(
                Uuid::new_v4(),
                "hello".to_string(),
                String::new(),
                None,
                RepositoryVisibility::Private,
                RepositorySettingsUpdate::default(),
                |_| Err(std::io::Error::other("disk full")),
            )
            .await;

        assert!(result.is_err());
        assert!(repo_store.snapshot().is_empty());
    }

    #[tokio::test]
    async fn rejects_a_name_with_invalid_characters() {
        let repo_store = Arc::new(FakeRepositories::empty());
        let use_case = CreateRepositoryUseCase::new(repo_store, Arc::new(FakeSettings::default()));

        let result = use_case
            .execute(
                Uuid::new_v4(),
                "not a valid name!".to_string(),
                String::new(),
                None,
                RepositoryVisibility::Private,
                RepositorySettingsUpdate::default(),
                |_| Ok(()),
            )
            .await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn creating_a_repository_with_a_name_that_already_exists_for_the_owner_is_a_conflict_and_never_touches_disk()
     {
        let existing = Repository {
            id: Uuid::new_v4(),
            owner_id: Uuid::new_v4(),
            name: "hello".to_string(),
            group_id: None,
            description: String::new(),
            disk_path: "existing/hello.git".to_string(),
            visibility: RepositoryVisibility::Private,
            created_at: chrono::Utc::now(),
        };
        let owner_id = existing.owner_id;
        let repo_store = Arc::new(FakeRepositories::new(vec![existing]));
        let use_case =
            CreateRepositoryUseCase::new(repo_store.clone(), Arc::new(FakeSettings::default()));
        let mut init_was_called = false;

        let result = use_case
            .execute(
                owner_id,
                "hello".to_string(),
                String::new(),
                None,
                RepositoryVisibility::Private,
                RepositorySettingsUpdate::default(),
                |_| {
                    init_was_called = true;
                    Ok(())
                },
            )
            .await;

        assert!(
            matches!(result, Err(DomainError::Conflict(_))),
            "expected Conflict, got {result:?}"
        );
        assert!(
            !init_was_called,
            "disk init must not run when the name already exists"
        );
    }

    #[tokio::test]
    async fn creating_a_repository_in_a_group_checks_group_scoped_uniqueness() {
        let group_id = Uuid::new_v4();
        let existing = Repository {
            id: Uuid::new_v4(),
            owner_id: Uuid::new_v4(),
            name: "hello".to_string(),
            group_id: Some(group_id),
            description: String::new(),
            disk_path: "existing/hello.git".to_string(),
            visibility: RepositoryVisibility::Private,
            created_at: chrono::Utc::now(),
        };
        let repo_store = Arc::new(FakeRepositories::new(vec![existing]));
        let use_case =
            CreateRepositoryUseCase::new(repo_store.clone(), Arc::new(FakeSettings::default()));
        let mut init_was_called = false;

        let result = use_case
            .execute(
                Uuid::new_v4(),
                "hello".to_string(),
                String::new(),
                Some(group_id),
                RepositoryVisibility::Private,
                RepositorySettingsUpdate::default(),
                |_| {
                    init_was_called = true;
                    Ok(())
                },
            )
            .await;

        assert!(
            matches!(result, Err(DomainError::Conflict(_))),
            "expected Conflict, got {result:?}"
        );
        assert!(
            !init_was_called,
            "disk init must not run when the name already exists in the group"
        );

        let personal = use_case
            .execute(
                Uuid::new_v4(),
                "hello".to_string(),
                String::new(),
                None,
                RepositoryVisibility::Private,
                RepositorySettingsUpdate::default(),
                |_| Ok(()),
            )
            .await
            .unwrap();
        assert_eq!(personal.group_id, None);
    }
}
