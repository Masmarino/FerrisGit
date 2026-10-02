//! Issue and merge request lines that carry their repository, shared by the search and the dashboard.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::issue::Issue;
use ferrisgit_domain::merge_request::MergeRequest;
use serde::Serialize;
use uuid::Uuid;

use crate::error::ApiError;
use crate::routes::repositories::repository_path;
use crate::routes::user_ref::require_user;
use crate::state::AppState;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RepositoryRef {
    id: Uuid,
    name: String,
    path: Vec<String>,
}

/// Results often share a repository and resolving one costs lookups, hence the cache.
pub(crate) type RepositoryRefs = HashMap<Uuid, RepositoryRef>;

pub(crate) async fn repository_ref(
    state: &AppState,
    repository_id: Uuid,
    cache: &mut RepositoryRefs,
) -> Result<RepositoryRef, ApiError> {
    if let Some(cached) = cache.get(&repository_id) {
        return Ok(cached.clone());
    }
    let repo = state
        .repositories
        .find_by_id(repository_id)
        .await?
        .ok_or_else(|| DomainError::NotFound("repository".to_string()))?;
    let owner = require_user(state, repo.owner_id).await?;
    let path = repository_path(state, &repo, &owner.username).await?;
    let result = RepositoryRef {
        id: repo.id,
        name: repo.name,
        path,
    };
    cache.insert(repository_id, result.clone());
    Ok(result)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IssueSummary {
    id: Uuid,
    number: i32,
    title: String,
    status: String,
    kind: String,
    created_at: DateTime<Utc>,
    repository: RepositoryRef,
}

pub(crate) async fn issue_summaries(
    state: &AppState,
    issues: Vec<Issue>,
    cache: &mut RepositoryRefs,
) -> Result<Vec<IssueSummary>, ApiError> {
    let mut summaries = Vec::with_capacity(issues.len());
    for issue in issues {
        let repository = repository_ref(state, issue.repository_id, cache).await?;
        summaries.push(IssueSummary {
            id: issue.id,
            number: issue.number,
            title: issue.title,
            status: issue.status.as_str().to_string(),
            kind: issue.kind.as_str().to_string(),
            created_at: issue.created_at,
            repository,
        });
    }
    Ok(summaries)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MergeRequestSummary {
    id: Uuid,
    title: String,
    status: String,
    source_branch: String,
    target_branch: String,
    created_at: DateTime<Utc>,
    repository: RepositoryRef,
}

pub(crate) async fn merge_request_summaries(
    state: &AppState,
    merge_requests: Vec<MergeRequest>,
    cache: &mut RepositoryRefs,
) -> Result<Vec<MergeRequestSummary>, ApiError> {
    let mut summaries = Vec::with_capacity(merge_requests.len());
    for mr in merge_requests {
        let repository = repository_ref(state, mr.repository_id, cache).await?;
        summaries.push(MergeRequestSummary {
            id: mr.id,
            title: mr.title,
            status: mr.status.as_str().to_string(),
            source_branch: mr.source_branch,
            target_branch: mr.target_branch,
            created_at: mr.created_at,
            repository,
        });
    }
    Ok(summaries)
}
