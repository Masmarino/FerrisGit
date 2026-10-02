use std::collections::HashMap;

use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::user::User;
use serde::Serialize;
use uuid::Uuid;

use crate::state::AppState;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UserRef {
    pub id: Uuid,
    pub username: String,
}

/// Ids that no longer resolve are left out of the map and render as an unknown user (`null`).
pub async fn load_user_refs(
    state: &AppState,
    ids: impl IntoIterator<Item = Uuid>,
) -> HashMap<Uuid, UserRef> {
    let mut distinct: Vec<Uuid> = ids.into_iter().collect();
    distinct.sort();
    distinct.dedup();
    let mut users = HashMap::new();
    for id in distinct {
        if let Ok(Some(user)) = state.users.find_by_id(id).await {
            users.insert(
                id,
                UserRef {
                    id: user.id,
                    username: user.username,
                },
            );
        }
    }
    users
}

pub(crate) async fn require_user(state: &AppState, id: Uuid) -> Result<User, DomainError> {
    state
        .users
        .find_by_id(id)
        .await?
        .ok_or_else(|| DomainError::NotFound("user".to_string()))
}
