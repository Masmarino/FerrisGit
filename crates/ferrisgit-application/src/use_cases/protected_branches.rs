use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::GroupStorePort;
use ferrisgit_domain::group_membership::GroupMembershipPort;
use ferrisgit_domain::protected_branch::{
    BranchRequirements, NewProtectedBranch, ProtectedBranch, ProtectedBranchStorePort,
    ProtectedBranchUpdate, RefRefusal, RefUpdate, branch_requirements, check_required_approvals,
    is_protected, normalize_check_names, normalize_pattern, refused_ref_updates,
};
use ferrisgit_domain::repository::Repository;
use ferrisgit_domain::repository_authz::effective_repository_role;
use ferrisgit_domain::repository_collaborator::RepositoryCollaboratorStorePort;
use uuid::Uuid;

/// The protected branches of a repository: the rules Maintainers manage, and the check every write to a branch goes
/// through. Callers check the caller's role on the repository before `protect` and `unprotect`.
pub struct ProtectedBranches {
    store: Arc<dyn ProtectedBranchStorePort>,
    collaborators: Arc<dyn RepositoryCollaboratorStorePort>,
    groups: Arc<dyn GroupStorePort>,
    group_membership: Arc<dyn GroupMembershipPort>,
}

impl ProtectedBranches {
    pub fn new(
        store: Arc<dyn ProtectedBranchStorePort>,
        collaborators: Arc<dyn RepositoryCollaboratorStorePort>,
        groups: Arc<dyn GroupStorePort>,
        group_membership: Arc<dyn GroupMembershipPort>,
    ) -> Self {
        Self {
            store,
            collaborators,
            groups,
            group_membership,
        }
    }

    pub async fn list(&self, repository_id: Uuid) -> Result<Vec<ProtectedBranch>, DomainError> {
        self.store.list_for_repository(repository_id).await
    }

    /// Protects `pattern`, a merge into it needing `required_approvals` approvals and every check of `required_checks`.
    pub async fn protect(
        &self,
        repository_id: Uuid,
        pattern: &str,
        required_approvals: i32,
        required_checks: &[String],
    ) -> Result<ProtectedBranch, DomainError> {
        let pattern = normalize_pattern(pattern)?;
        self.store
            .create(NewProtectedBranch {
                repository_id,
                pattern,
                required_approvals: check_required_approvals(required_approvals)?,
                required_checks: normalize_check_names(required_checks)?,
            })
            .await
    }

    /// Changes what a merge into the rule's branches needs. `None` keeps a requirement as it is.
    pub async fn update_requirements(
        &self,
        repository_id: Uuid,
        id: Uuid,
        required_approvals: Option<i32>,
        required_checks: Option<Vec<String>>,
    ) -> Result<ProtectedBranch, DomainError> {
        let update = ProtectedBranchUpdate {
            required_approvals: required_approvals
                .map(check_required_approvals)
                .transpose()?,
            required_checks: required_checks
                .as_deref()
                .map(normalize_check_names)
                .transpose()?,
        };
        self.store.update(repository_id, id, update).await
    }

    /// What a merge into `branch` needs from the repository's protections.
    pub async fn requirements(
        &self,
        repository_id: Uuid,
        branch: &str,
    ) -> Result<BranchRequirements, DomainError> {
        Ok(branch_requirements(
            &self.store.list_for_repository(repository_id).await?,
            branch,
        ))
    }

    pub async fn unprotect(&self, repository_id: Uuid, id: Uuid) -> Result<(), DomainError> {
        self.store.delete(repository_id, id).await
    }

    /// Whether `branch` is protected in the repository.
    pub async fn is_protected(
        &self,
        repository_id: Uuid,
        branch: &str,
    ) -> Result<bool, DomainError> {
        Ok(is_protected(
            &self.store.list_for_repository(repository_id).await?,
            branch,
        ))
    }

    /// The commands of a push by `pusher_id` that the protections refuse; empty when the push may go ahead.
    pub async fn refused_updates(
        &self,
        repo: &Repository,
        pusher_id: Uuid,
        updates: &[RefUpdate],
    ) -> Result<Vec<RefRefusal>, DomainError> {
        let protections = self.store.list_for_repository(repo.id).await?;
        if protections.is_empty() {
            return Ok(vec![]);
        }
        let role = effective_repository_role(
            self.collaborators.as_ref(),
            self.groups.as_ref(),
            self.group_membership.as_ref(),
            repo,
            pusher_id,
        )
        .await?;
        Ok(refused_ref_updates(&protections, updates, role))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeCollaborators, FakeGroups, FakeProtectedBranches};
    use crate::use_cases::fixtures::repository;
    use ferrisgit_domain::repository_collaborator::CollaboratorRole;

    const ZERO: &str = "0000000000000000000000000000000000000000";
    const A: &str = "1111111111111111111111111111111111111111";
    const B: &str = "2222222222222222222222222222222222222222";

    fn service(collaborators: Vec<(Uuid, Uuid, CollaboratorRole)>) -> ProtectedBranches {
        let groups = Arc::new(FakeGroups::empty());
        ProtectedBranches::new(
            Arc::new(FakeProtectedBranches::default()),
            Arc::new(FakeCollaborators::new(collaborators)),
            groups.clone(),
            groups,
        )
    }

    fn update(old: &str, new: &str, branch: &str) -> RefUpdate {
        RefUpdate {
            old_sha: old.to_string(),
            new_sha: new.to_string(),
            ref_name: format!("refs/heads/{branch}"),
        }
    }

    #[tokio::test]
    async fn a_pattern_is_checked_and_trimmed_before_it_is_stored() {
        let service = service(vec![]);
        let repository_id = Uuid::new_v4();

        let rule = service
            .protect(repository_id, " main ", 0, &[])
            .await
            .unwrap();
        assert_eq!(rule.pattern, "main");
        assert!(matches!(
            service
                .protect(repository_id, "refs/heads/main", 0, &[])
                .await,
            Err(DomainError::Validation(_))
        ));
        assert!(matches!(
            service.protect(repository_id, "main", 0, &[]).await,
            Err(DomainError::Conflict(_))
        ));
        assert!(service.is_protected(repository_id, "main").await.unwrap());

        let checks = vec![" pipeline ".to_string(), "pipeline".to_string()];
        let updated = service
            .update_requirements(repository_id, rule.id, Some(2), Some(checks))
            .await
            .unwrap();
        assert_eq!(updated.required_checks, vec!["pipeline"]);
        assert_eq!(
            service
                .requirements(repository_id, "main")
                .await
                .unwrap()
                .required_approvals,
            2
        );
        assert!(matches!(
            service
                .update_requirements(repository_id, rule.id, Some(-1), None)
                .await,
            Err(DomainError::Validation(_))
        ));

        service.unprotect(repository_id, rule.id).await.unwrap();
        assert!(!service.is_protected(repository_id, "main").await.unwrap());
    }

    #[tokio::test]
    async fn a_push_to_a_protected_branch_is_refused_and_one_elsewhere_goes_through() {
        let owner = Uuid::new_v4();
        let contributor = Uuid::new_v4();
        let repo = repository(owner);
        let service = service(vec![(repo.id, contributor, CollaboratorRole::Contributor)]);
        service.protect(repo.id, "main", 0, &[]).await.unwrap();

        let refused = service
            .refused_updates(&repo, contributor, &[update(A, B, "main")])
            .await
            .unwrap();
        assert_eq!(refused.len(), 1);
        assert!(
            service
                .refused_updates(&repo, contributor, &[update(A, B, "feature")])
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn the_owner_counts_as_a_maintainer_and_may_create_a_protected_branch() {
        let owner = Uuid::new_v4();
        let contributor = Uuid::new_v4();
        let repo = repository(owner);
        let service = service(vec![(repo.id, contributor, CollaboratorRole::Contributor)]);
        service.protect(repo.id, "main", 0, &[]).await.unwrap();
        let first_push = [update(ZERO, A, "main")];

        assert!(
            service
                .refused_updates(&repo, owner, &first_push)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            service
                .refused_updates(&repo, contributor, &first_push)
                .await
                .unwrap()
                .len(),
            1
        );
    }
}
