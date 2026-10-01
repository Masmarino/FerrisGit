use serde::Serialize;
use uuid::Uuid;

/// One variant per event that triggers an in-app notification. It is a separate enum from `NotificationKind`, so a
/// payload change never affects notifications. `kind()` returns the same strings as `NotificationKind::as_str()`, which
/// a webhook's stored `events` are matched against.
// Each serde rename equals `kind()` so the payload's `"event"` matches the subscription string (`"issue_closed"`)
// rather than serde's default variant name. `every_variant_serializes_its_tag_as_its_own_kind` catches drift.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "event", rename_all_fields = "camelCase")]
pub enum WebhookEvent {
    #[serde(rename = "merge_request_approved")]
    MergeRequestApproved {
        repository_owner: String,
        repository_name: String,
        actor_username: String,
        merge_request_id: Uuid,
        merge_request_title: String,
    },
    #[serde(rename = "merge_request_changes_requested")]
    MergeRequestChangesRequested {
        repository_owner: String,
        repository_name: String,
        actor_username: String,
        merge_request_id: Uuid,
        merge_request_title: String,
    },
    #[serde(rename = "merge_request_commented")]
    MergeRequestCommented {
        repository_owner: String,
        repository_name: String,
        actor_username: String,
        merge_request_id: Uuid,
        merge_request_title: String,
    },
    #[serde(rename = "merge_request_merged")]
    MergeRequestMerged {
        repository_owner: String,
        repository_name: String,
        actor_username: String,
        merge_request_id: Uuid,
        merge_request_title: String,
    },
    #[serde(rename = "merge_request_closed")]
    MergeRequestClosed {
        repository_owner: String,
        repository_name: String,
        actor_username: String,
        merge_request_id: Uuid,
        merge_request_title: String,
    },
    #[serde(rename = "collaborator_added")]
    CollaboratorAdded {
        repository_owner: String,
        repository_name: String,
        actor_username: String,
        target_username: String,
        role: String,
    },
    #[serde(rename = "collaborator_role_changed")]
    CollaboratorRoleChanged {
        repository_owner: String,
        repository_name: String,
        actor_username: String,
        target_username: String,
        role: String,
    },
    #[serde(rename = "collaborator_removed")]
    CollaboratorRemoved {
        repository_owner: String,
        repository_name: String,
        actor_username: String,
        target_username: String,
    },
    #[serde(rename = "pipeline_failed")]
    PipelineFailed {
        repository_owner: String,
        repository_name: String,
        pipeline_id: Uuid,
        commit_sha: String,
    },
    #[serde(rename = "issue_assigned")]
    IssueAssigned {
        repository_owner: String,
        repository_name: String,
        actor_username: String,
        issue_id: Uuid,
        issue_title: String,
    },
    #[serde(rename = "issue_commented")]
    IssueCommented {
        repository_owner: String,
        repository_name: String,
        actor_username: String,
        issue_id: Uuid,
        issue_title: String,
    },
    #[serde(rename = "issue_closed")]
    IssueClosed {
        repository_owner: String,
        repository_name: String,
        actor_username: String,
        issue_id: Uuid,
        issue_title: String,
    },
}

impl WebhookEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            WebhookEvent::MergeRequestApproved { .. } => "merge_request_approved",
            WebhookEvent::MergeRequestChangesRequested { .. } => "merge_request_changes_requested",
            WebhookEvent::MergeRequestCommented { .. } => "merge_request_commented",
            WebhookEvent::MergeRequestMerged { .. } => "merge_request_merged",
            WebhookEvent::MergeRequestClosed { .. } => "merge_request_closed",
            WebhookEvent::CollaboratorAdded { .. } => "collaborator_added",
            WebhookEvent::CollaboratorRoleChanged { .. } => "collaborator_role_changed",
            WebhookEvent::CollaboratorRemoved { .. } => "collaborator_removed",
            WebhookEvent::PipelineFailed { .. } => "pipeline_failed",
            WebhookEvent::IssueAssigned { .. } => "issue_assigned",
            WebhookEvent::IssueCommented { .. } => "issue_commented",
            WebhookEvent::IssueClosed { .. } => "issue_closed",
        }
    }

    /// Every kind string a webhook may subscribe to, checked at create/update time. Kept next to `kind()`;
    /// `every_variant_serializes_its_tag_as_its_own_kind` covers the payload side.
    pub const ALL_KINDS: &'static [&'static str] = &[
        "merge_request_approved",
        "merge_request_changes_requested",
        "merge_request_commented",
        "merge_request_merged",
        "merge_request_closed",
        "collaborator_added",
        "collaborator_role_changed",
        "collaborator_removed",
        "pipeline_failed",
        "issue_assigned",
        "issue_commented",
        "issue_closed",
    ];
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notification::NotificationKind;

    fn sample(kind: &str) -> WebhookEvent {
        match kind {
            "merge_request_approved" => WebhookEvent::MergeRequestApproved {
                repository_owner: String::new(),
                repository_name: String::new(),
                actor_username: String::new(),
                merge_request_id: Uuid::nil(),
                merge_request_title: String::new(),
            },
            "merge_request_changes_requested" => WebhookEvent::MergeRequestChangesRequested {
                repository_owner: String::new(),
                repository_name: String::new(),
                actor_username: String::new(),
                merge_request_id: Uuid::nil(),
                merge_request_title: String::new(),
            },
            "merge_request_commented" => WebhookEvent::MergeRequestCommented {
                repository_owner: String::new(),
                repository_name: String::new(),
                actor_username: String::new(),
                merge_request_id: Uuid::nil(),
                merge_request_title: String::new(),
            },
            "merge_request_merged" => WebhookEvent::MergeRequestMerged {
                repository_owner: String::new(),
                repository_name: String::new(),
                actor_username: String::new(),
                merge_request_id: Uuid::nil(),
                merge_request_title: String::new(),
            },
            "merge_request_closed" => WebhookEvent::MergeRequestClosed {
                repository_owner: String::new(),
                repository_name: String::new(),
                actor_username: String::new(),
                merge_request_id: Uuid::nil(),
                merge_request_title: String::new(),
            },
            "collaborator_added" => WebhookEvent::CollaboratorAdded {
                repository_owner: String::new(),
                repository_name: String::new(),
                actor_username: String::new(),
                target_username: String::new(),
                role: String::new(),
            },
            "collaborator_role_changed" => WebhookEvent::CollaboratorRoleChanged {
                repository_owner: String::new(),
                repository_name: String::new(),
                actor_username: String::new(),
                target_username: String::new(),
                role: String::new(),
            },
            "collaborator_removed" => WebhookEvent::CollaboratorRemoved {
                repository_owner: String::new(),
                repository_name: String::new(),
                actor_username: String::new(),
                target_username: String::new(),
            },
            "pipeline_failed" => WebhookEvent::PipelineFailed {
                repository_owner: String::new(),
                repository_name: String::new(),
                pipeline_id: Uuid::nil(),
                commit_sha: String::new(),
            },
            "issue_assigned" => WebhookEvent::IssueAssigned {
                repository_owner: String::new(),
                repository_name: String::new(),
                actor_username: String::new(),
                issue_id: Uuid::nil(),
                issue_title: String::new(),
            },
            "issue_commented" => WebhookEvent::IssueCommented {
                repository_owner: String::new(),
                repository_name: String::new(),
                actor_username: String::new(),
                issue_id: Uuid::nil(),
                issue_title: String::new(),
            },
            "issue_closed" => WebhookEvent::IssueClosed {
                repository_owner: String::new(),
                repository_name: String::new(),
                actor_username: String::new(),
                issue_id: Uuid::nil(),
                issue_title: String::new(),
            },
            other => panic!("unhandled kind in test helper: {other}"),
        }
    }

    #[test]
    fn every_notification_kind_has_a_matching_webhook_event_kind() {
        let notification_kinds = [
            NotificationKind::MergeRequestApproved,
            NotificationKind::MergeRequestChangesRequested,
            NotificationKind::MergeRequestCommented,
            NotificationKind::MergeRequestMerged,
            NotificationKind::MergeRequestClosed,
            NotificationKind::CollaboratorAdded,
            NotificationKind::CollaboratorRoleChanged,
            NotificationKind::CollaboratorRemoved,
            NotificationKind::PipelineFailed,
            NotificationKind::IssueAssigned,
            NotificationKind::IssueCommented,
            NotificationKind::IssueClosed,
        ];
        for nk in notification_kinds {
            let webhook_event = sample(nk.as_str());
            assert_eq!(webhook_event.kind(), nk.as_str());
        }
    }

    #[test]
    fn serializing_tags_the_json_with_the_kind_string_not_the_variant_name() {
        let event = WebhookEvent::IssueClosed {
            repository_owner: "alice".to_string(),
            repository_name: "hello".to_string(),
            actor_username: "bob".to_string(),
            issue_id: Uuid::nil(),
            issue_title: "bug".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"event\":\"issue_closed\""));
    }

    #[test]
    fn serializing_uses_camel_case_field_names() {
        let event = WebhookEvent::IssueClosed {
            repository_owner: "alice".to_string(),
            repository_name: "hello".to_string(),
            actor_username: "bob".to_string(),
            issue_id: Uuid::nil(),
            issue_title: "bug".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"repositoryOwner\":\"alice\""));
        assert!(json.contains("\"repositoryName\":\"hello\""));
        assert!(json.contains("\"actorUsername\":\"bob\""));
        assert!(json.contains("\"issueId\""));
        assert!(json.contains("\"issueTitle\":\"bug\""));
    }

    #[test]
    fn all_kinds_lists_every_kind_exactly_once_with_no_unknowns() {
        let mut expected: Vec<&str> = WebhookEvent::ALL_KINDS.to_vec();
        expected.sort_unstable();
        expected.dedup();
        assert_eq!(
            expected.len(),
            WebhookEvent::ALL_KINDS.len(),
            "ALL_KINDS must not contain duplicates"
        );

        let notification_kinds = [
            NotificationKind::MergeRequestApproved,
            NotificationKind::MergeRequestChangesRequested,
            NotificationKind::MergeRequestCommented,
            NotificationKind::MergeRequestMerged,
            NotificationKind::MergeRequestClosed,
            NotificationKind::CollaboratorAdded,
            NotificationKind::CollaboratorRoleChanged,
            NotificationKind::CollaboratorRemoved,
            NotificationKind::PipelineFailed,
            NotificationKind::IssueAssigned,
            NotificationKind::IssueCommented,
            NotificationKind::IssueClosed,
        ];
        for nk in notification_kinds {
            assert!(
                WebhookEvent::ALL_KINDS.contains(&nk.as_str()),
                "ALL_KINDS is missing {}",
                nk.as_str()
            );
        }
        assert_eq!(
            WebhookEvent::ALL_KINDS.len(),
            notification_kinds.len(),
            "ALL_KINDS must have no entries beyond the known notification kinds"
        );
    }

    #[test]
    fn every_variant_serializes_its_tag_as_its_own_kind() {
        for kind in [
            "merge_request_approved",
            "merge_request_changes_requested",
            "merge_request_commented",
            "merge_request_merged",
            "merge_request_closed",
            "collaborator_added",
            "collaborator_role_changed",
            "collaborator_removed",
            "pipeline_failed",
            "issue_assigned",
            "issue_commented",
            "issue_closed",
        ] {
            let event = sample(kind);
            let json = serde_json::to_string(&event).unwrap();
            assert!(
                json.contains(&format!("\"event\":\"{kind}\"")),
                "expected tag {kind:?} in {json}"
            );
        }
    }
}
