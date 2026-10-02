use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::repository::{
    NewRepository, Repository, RepositoryStorePort, RepositoryVisibility,
};
use ferrisgit_domain::settings::{RepositorySettingsStorePort, RepositorySettingsUpdate};
use uuid::Uuid;

use super::name_rules::is_valid_path_name;

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

    /// The bare repo is initialised on disk before the row is created, so a failure there never leaves an orphaned row.
    /// `settings_update` is applied on top of the default settings.
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
        if !is_valid_path_name(&name) {
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
    use crate::test_support::{FakeRepositories, FakeRepositorySettings};
    use crate::use_cases::fixtures::{repository, repository_with_id};
    use ferrisgit_domain::settings::RepositorySettings;

    struct Fixture {
        use_case: CreateRepositoryUseCase,
        repositories: Arc<FakeRepositories>,
        settings: Arc<FakeRepositorySettings>,
    }

    fn fixture(existing: Vec<Repository>) -> Fixture {
        let repositories = Arc::new(FakeRepositories::new(existing));
        let settings = Arc::new(FakeRepositorySettings::new(RepositorySettings {
            repository_id: Uuid::nil(),
            pipeline_file_path: ".ferrisgit-ci.yml".to_string(),
            ci_enabled: true,
            required_approvals: 0,
        }));
        let use_case = CreateRepositoryUseCase::new(repositories.clone(), settings.clone());
        Fixture {
            use_case,
            repositories,
            settings,
        }
    }

    impl Fixture {
        /// A private personal repo for a new owner, default settings, disk init always succeeds.
        async fn create(&self, name: &str) -> Result<Repository, DomainError> {
            self.use_case
                .execute(
                    Uuid::new_v4(),
                    name.to_string(),
                    String::new(),
                    None,
                    RepositoryVisibility::Private,
                    RepositorySettingsUpdate::default(),
                    |_| Ok(()),
                )
                .await
        }

        async fn saved_settings(&self, repository_id: Uuid) -> RepositorySettings {
            self.settings
                .get_or_create_default(repository_id)
                .await
                .unwrap()
        }
    }

    #[tokio::test]
    async fn creating_a_repository_initializes_disk_before_persisting() {
        let f = fixture(vec![]);
        let owner_id = Uuid::new_v4();
        let mut init_called_with = None;

        let created = f
            .use_case
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
        let f = fixture(vec![]);

        let created = f
            .use_case
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
        let f = fixture(vec![]);

        let created = f
            .use_case
            .execute(
                Uuid::new_v4(),
                "hello".to_string(),
                String::new(),
                None,
                RepositoryVisibility::Private,
                RepositorySettingsUpdate {
                    ci_enabled: Some(false),
                    required_approvals: Some(2),
                    pipeline_file_path: Some("custom.yml".to_string()),
                },
                |_| Ok(()),
            )
            .await
            .unwrap();

        let saved = f.saved_settings(created.id).await;
        assert!(!saved.ci_enabled);
        assert_eq!(saved.required_approvals, 2);
        assert_eq!(saved.pipeline_file_path, "custom.yml");
    }

    #[tokio::test]
    async fn creating_a_repository_with_no_overrides_keeps_the_defaults() {
        let f = fixture(vec![]);

        let created = f.create("hello").await.unwrap();

        let saved = f.saved_settings(created.id).await;
        assert!(saved.ci_enabled);
        assert_eq!(saved.required_approvals, 0);
        assert_eq!(saved.pipeline_file_path, ".ferrisgit-ci.yml");
    }

    #[tokio::test]
    async fn a_disk_init_failure_prevents_the_repository_from_being_persisted() {
        let f = fixture(vec![]);

        let result = f
            .use_case
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
        assert!(f.repositories.snapshot().is_empty());
    }

    #[tokio::test]
    async fn rejects_a_name_with_invalid_characters() {
        let f = fixture(vec![]);

        let result = f.create("not a valid name!").await;

        assert!(matches!(result, Err(DomainError::Validation(_))));
    }

    #[tokio::test]
    async fn creating_a_repository_with_a_name_that_already_exists_for_the_owner_is_a_conflict_and_never_touches_disk()
     {
        let existing = repository(Uuid::new_v4());
        let owner_id = existing.owner_id;
        let f = fixture(vec![existing]);
        let mut init_was_called = false;

        let result = f
            .use_case
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
            group_id: Some(group_id),
            ..repository_with_id(Uuid::new_v4(), Uuid::new_v4())
        };
        let f = fixture(vec![existing]);
        let mut init_was_called = false;

        let result = f
            .use_case
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

        let personal = f.create("hello").await.unwrap();
        assert_eq!(personal.group_id, None);
    }
}
