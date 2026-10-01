use async_trait::async_trait;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentHealth {
    Up,
    Down(String),
}

impl ComponentHealth {
    pub fn status_str(&self) -> &'static str {
        match self {
            ComponentHealth::Up => "up",
            ComponentHealth::Down(_) => "down",
        }
    }

    pub fn detail(&self) -> Option<String> {
        match self {
            ComponentHealth::Up => None,
            ComponentHealth::Down(message) => Some(message.clone()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DatabaseHealth {
    pub status: ComponentHealth,
    pub response_time_ms: u64,
    pub active_connections: u32,
    pub max_connections: u32,
    pub server_version: Option<String>,
}

#[async_trait]
pub trait HealthCheckPort: Send + Sync {
    async fn check(&self) -> DatabaseHealth;
}

#[derive(Debug, Clone)]
pub struct StorageHealth {
    pub status: ComponentHealth,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub total_bytes: u64,
}

#[async_trait]
pub trait StorageHealthCheckPort: Send + Sync {
    async fn check(&self) -> StorageHealth;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn up_has_no_detail_and_reports_status_up() {
        assert_eq!(ComponentHealth::Up.status_str(), "up");
        assert_eq!(ComponentHealth::Up.detail(), None);
    }

    #[test]
    fn down_carries_its_message_as_detail_and_reports_status_down() {
        let health = ComponentHealth::Down("connection refused".to_string());
        assert_eq!(health.status_str(), "down");
        assert_eq!(health.detail(), Some("connection refused".to_string()));
    }
}
