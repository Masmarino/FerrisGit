use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::DomainError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum SecurityEvent {
    LoginFailed {
        username: String,
    },
    LoginSucceeded {
        user_id: Uuid,
    },
    GitAccessDenied {
        username: String,
        repository: String,
    },
    GitTokenInvalid {
        username: String,
    },
    MfaVerificationFailed {
        user_id: Uuid,
    },
    MfaEnrolled {
        user_id: Uuid,
    },
    MfaDisabled {
        user_id: Uuid,
    },
    MfaResetByAdmin {
        target_user_id: Uuid,
    },
    PasswordResetByAdmin {
        target_user_id: Uuid,
    },
    AdminGranted {
        target_user_id: Uuid,
    },
    AdminRevoked {
        target_user_id: Uuid,
    },
    /// The account row is gone by the time this is read, so the event keeps who it was and which personal repos went with it.
    UserDeletedByAdmin {
        target_user_id: Uuid,
        username: String,
        deleted_repositories: Vec<String>,
    },
    PasskeyAdded {
        user_id: Uuid,
    },
    PasskeyDeleted {
        user_id: Uuid,
    },
    PasskeyVerificationFailed {
        user_id: Uuid,
    },
    /// The user signed out everywhere: every session of the account ended, this one included.
    SessionsRevoked {
        user_id: Uuid,
    },
}

impl SecurityEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            SecurityEvent::LoginFailed { .. } => "LoginFailed",
            SecurityEvent::LoginSucceeded { .. } => "LoginSucceeded",
            SecurityEvent::GitAccessDenied { .. } => "GitAccessDenied",
            SecurityEvent::GitTokenInvalid { .. } => "GitTokenInvalid",
            SecurityEvent::MfaVerificationFailed { .. } => "MfaVerificationFailed",
            SecurityEvent::MfaEnrolled { .. } => "MfaEnrolled",
            SecurityEvent::MfaDisabled { .. } => "MfaDisabled",
            SecurityEvent::MfaResetByAdmin { .. } => "MfaResetByAdmin",
            SecurityEvent::PasswordResetByAdmin { .. } => "PasswordResetByAdmin",
            SecurityEvent::AdminGranted { .. } => "AdminGranted",
            SecurityEvent::AdminRevoked { .. } => "AdminRevoked",
            SecurityEvent::UserDeletedByAdmin { .. } => "UserDeletedByAdmin",
            SecurityEvent::PasskeyAdded { .. } => "PasskeyAdded",
            SecurityEvent::PasskeyDeleted { .. } => "PasskeyDeleted",
            SecurityEvent::PasskeyVerificationFailed { .. } => "PasskeyVerificationFailed",
            SecurityEvent::SessionsRevoked { .. } => "SessionsRevoked",
        }
    }
}

#[async_trait]
pub trait EventPublisherPort: Send + Sync {
    async fn publish_security_event(
        &self,
        event: SecurityEvent,
        actor_id: Option<Uuid>,
    ) -> Result<(), DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_type_matches_the_variant() {
        let event = SecurityEvent::LoginFailed {
            username: "florian".to_string(),
        };
        assert_eq!(event.event_type(), "LoginFailed");
    }

    #[test]
    fn security_event_round_trips_through_json() {
        let event = SecurityEvent::GitAccessDenied {
            username: "florian".to_string(),
            repository: "hello".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let decoded: SecurityEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.event_type(), "GitAccessDenied");
    }

    #[test]
    fn git_token_invalid_event_type_matches_the_variant() {
        let event = SecurityEvent::GitTokenInvalid {
            username: "florian".to_string(),
        };
        assert_eq!(event.event_type(), "GitTokenInvalid");
    }

    #[test]
    fn mfa_event_types_match_the_variants() {
        let id = Uuid::new_v4();
        assert_eq!(
            SecurityEvent::MfaVerificationFailed { user_id: id }.event_type(),
            "MfaVerificationFailed"
        );
        assert_eq!(
            SecurityEvent::MfaEnrolled { user_id: id }.event_type(),
            "MfaEnrolled"
        );
        assert_eq!(
            SecurityEvent::MfaDisabled { user_id: id }.event_type(),
            "MfaDisabled"
        );
        assert_eq!(
            SecurityEvent::MfaResetByAdmin { target_user_id: id }.event_type(),
            "MfaResetByAdmin"
        );
    }

    #[test]
    fn mfa_events_round_trip_through_json_with_their_variant_name() {
        let event = SecurityEvent::MfaResetByAdmin {
            target_user_id: Uuid::new_v4(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"event_type\":\"MfaResetByAdmin\""));
        let decoded: SecurityEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.event_type(), "MfaResetByAdmin");
    }

    #[test]
    fn admin_account_management_event_types_match_the_variants() {
        let id = Uuid::new_v4();
        assert_eq!(
            SecurityEvent::PasswordResetByAdmin { target_user_id: id }.event_type(),
            "PasswordResetByAdmin"
        );
        assert_eq!(
            SecurityEvent::AdminGranted { target_user_id: id }.event_type(),
            "AdminGranted"
        );
        assert_eq!(
            SecurityEvent::AdminRevoked { target_user_id: id }.event_type(),
            "AdminRevoked"
        );
        assert_eq!(
            SecurityEvent::UserDeletedByAdmin {
                target_user_id: id,
                username: "alice".to_string(),
                deleted_repositories: vec![]
            }
            .event_type(),
            "UserDeletedByAdmin"
        );
    }

    #[test]
    fn admin_account_management_events_round_trip_through_json_with_their_variant_name() {
        for event in [
            SecurityEvent::PasswordResetByAdmin {
                target_user_id: Uuid::new_v4(),
            },
            SecurityEvent::AdminGranted {
                target_user_id: Uuid::new_v4(),
            },
            SecurityEvent::AdminRevoked {
                target_user_id: Uuid::new_v4(),
            },
            SecurityEvent::UserDeletedByAdmin {
                target_user_id: Uuid::new_v4(),
                username: "alice".to_string(),
                deleted_repositories: vec!["hello".to_string()],
            },
        ] {
            let json = serde_json::to_string(&event).unwrap();
            assert!(
                json.contains(&format!("\"event_type\":\"{}\"", event.event_type())),
                "{json}"
            );
            let decoded: SecurityEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded.event_type(), event.event_type());
        }
    }

    #[test]
    fn passkey_event_types_match_the_variants() {
        let id = Uuid::new_v4();
        assert_eq!(
            SecurityEvent::PasskeyAdded { user_id: id }.event_type(),
            "PasskeyAdded"
        );
        assert_eq!(
            SecurityEvent::PasskeyDeleted { user_id: id }.event_type(),
            "PasskeyDeleted"
        );
        assert_eq!(
            SecurityEvent::PasskeyVerificationFailed { user_id: id }.event_type(),
            "PasskeyVerificationFailed"
        );
    }

    #[test]
    fn sessions_revoked_round_trips_through_json_with_its_variant_name() {
        let event = SecurityEvent::SessionsRevoked {
            user_id: Uuid::new_v4(),
        };
        assert_eq!(event.event_type(), "SessionsRevoked");
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"event_type\":\"SessionsRevoked\""));
        let decoded: SecurityEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.event_type(), "SessionsRevoked");
    }

    #[test]
    fn passkey_events_round_trip_through_json_with_their_variant_name() {
        let event = SecurityEvent::PasskeyVerificationFailed {
            user_id: Uuid::new_v4(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"event_type\":\"PasskeyVerificationFailed\""));
        let decoded: SecurityEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.event_type(), "PasskeyVerificationFailed");
    }
}
