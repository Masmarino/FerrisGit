use std::sync::Arc;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::group::GroupStorePort;
use ferrisgit_domain::label::LabelStorePort;
use ferrisgit_domain::milestone::MilestoneStorePort;
use ferrisgit_domain::repository::Repository;
use uuid::Uuid;

use crate::scope_check::is_in_repository_scope;

/// Every label must exist and be attached to `repository` itself or to one of its groups.
pub(crate) async fn require_labels_in_scope(
    labels: &dyn LabelStorePort,
    groups: &Arc<dyn GroupStorePort>,
    repository: &Repository,
    label_ids: &[Uuid],
) -> Result<(), DomainError> {
    for label_id in label_ids {
        let label = labels
            .find_by_id(*label_id)
            .await?
            .ok_or_else(|| DomainError::NotFound("label".to_string()))?;
        if !is_in_repository_scope(groups, repository, label.repository_id, label.group_id).await? {
            return Err(DomainError::Validation(format!(
                "label {label_id} is not usable on this repository"
            )));
        }
    }
    Ok(())
}

/// The milestone must exist and be attached to `repository` itself or to one of its groups.
pub(crate) async fn require_milestone_in_scope(
    milestones: &dyn MilestoneStorePort,
    groups: &Arc<dyn GroupStorePort>,
    repository: &Repository,
    milestone_id: Uuid,
) -> Result<(), DomainError> {
    let milestone = milestones
        .find_by_id(milestone_id)
        .await?
        .ok_or_else(|| DomainError::NotFound("milestone".to_string()))?;
    if !is_in_repository_scope(
        groups,
        repository,
        milestone.repository_id,
        milestone.group_id,
    )
    .await?
    {
        return Err(DomainError::Validation(format!(
            "milestone {milestone_id} is not usable on this repository"
        )));
    }
    Ok(())
}
