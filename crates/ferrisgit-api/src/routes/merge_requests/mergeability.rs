use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use ferrisgit_application::mergeability::MergeabilityChecker;
use ferrisgit_domain::merge_conditions::{MergeCondition, Mergeability};
use ferrisgit_domain::merge_request::MergeRequestStatus;
use ferrisgit_domain::repository_collaborator::CollaboratorRole;
use serde::Serialize;
use uuid::Uuid;

use crate::auth_middleware::AuthUser;
use crate::error::ApiError;
use crate::state::AppState;

use super::find_accessible_merge_request;

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum ConditionResponse {
    #[serde(rename_all = "camelCase")]
    Approvals {
        met: bool,
        required: i32,
        given: i32,
        /// What is missing, `null` when the condition is met.
        message: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    NoChangesRequested {
        met: bool,
        requested_by: Vec<String>,
        /// What is missing, `null` when the condition is met.
        message: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Check {
        met: bool,
        name: String,
        /// `null` when the check has not run on the current source commit.
        status: Option<&'static str>,
        /// What is missing, `null` when the condition is met.
        message: Option<String>,
    },
}

impl From<MergeCondition> for ConditionResponse {
    fn from(condition: MergeCondition) -> Self {
        let met = condition.is_met();
        let message = (!met).then(|| condition.problem());
        match condition {
            MergeCondition::Approvals { required, given } => ConditionResponse::Approvals {
                met,
                required,
                given,
                message,
            },
            MergeCondition::NoChangesRequested { requested_by } => {
                ConditionResponse::NoChangesRequested {
                    met,
                    requested_by,
                    message,
                }
            }
            MergeCondition::Check { name, status } => ConditionResponse::Check {
                met,
                name,
                status: status.map(|status| status.as_str()),
                message,
            },
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MergeabilityResponse {
    mergeable: bool,
    conditions: Vec<ConditionResponse>,
}

impl From<Mergeability> for MergeabilityResponse {
    fn from(mergeability: Mergeability) -> Self {
        Self {
            mergeable: mergeability.is_mergeable(),
            conditions: mergeability
                .conditions
                .into_iter()
                .map(Into::into)
                .collect(),
        }
    }
}

/// What the merge needs, condition by condition: what the merge would refuse, said before trying. A merge request that
/// is no longer open has nothing left to meet.
async fn mergeability(
    AuthUser(user_id): AuthUser,
    State(state): State<AppState>,
    Path(merge_request_id): Path<Uuid>,
) -> Result<Json<MergeabilityResponse>, ApiError> {
    let (mr, repo) =
        find_accessible_merge_request(&state, user_id, merge_request_id, CollaboratorRole::Reader)
            .await?;
    if mr.status != MergeRequestStatus::Open {
        return Ok(Json(Mergeability::default().into()));
    }
    let checker = MergeabilityChecker::new(
        state.repository_settings.clone(),
        state.protected_branches.clone(),
        state.merge_request_reviews.clone(),
        state.branch_reader.clone(),
        state.pipelines.clone(),
        state.jobs.clone(),
    );
    Ok(Json(checker.evaluate(&mr, &repo).await?.into()))
}

pub(super) fn router() -> Router<AppState> {
    Router::new().route("/merge-requests/{id}/mergeability", get(mergeability))
}
