use async_trait::async_trait;
use ferrisgit_domain::health::{ComponentHealth, StorageHealth, StorageHealthCheckPort};
use std::path::PathBuf;

pub struct FilesystemStorageHealthCheck {
    root: PathBuf,
}

impl FilesystemStorageHealthCheck {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

/// Returns `(total_bytes, free_bytes)` for the filesystem containing `path`, using a raw
/// `statvfs(2)` syscall because Rust's std has no stable disk-space API. This blocks, so the
/// caller (`check`, above) runs it inside `spawn_blocking`.
fn statvfs(path: &std::path::Path) -> std::io::Result<(u64, u64)> {
    let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(std::io::Error::other)?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let total_bytes = stat.f_blocks as u64 * stat.f_frsize as u64;
    let free_bytes = stat.f_bavail as u64 * stat.f_frsize as u64;
    Ok((total_bytes, free_bytes))
}

#[async_trait]
impl StorageHealthCheckPort for FilesystemStorageHealthCheck {
    async fn check(&self) -> StorageHealth {
        let root = self.root.clone();
        let result = tokio::task::spawn_blocking(move || statvfs(&root)).await;

        match result {
            Ok(Ok((total_bytes, free_bytes))) => StorageHealth {
                status: ComponentHealth::Up,
                used_bytes: total_bytes.saturating_sub(free_bytes),
                free_bytes,
                total_bytes,
            },
            Ok(Err(e)) => StorageHealth {
                status: ComponentHealth::Down(e.to_string()),
                used_bytes: 0,
                free_bytes: 0,
                total_bytes: 0,
            },
            Err(e) => StorageHealth {
                status: ComponentHealth::Down(e.to_string()),
                used_bytes: 0,
                free_bytes: 0,
                total_bytes: 0,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn an_existing_directory_reports_up_with_nonzero_total_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let checker = FilesystemStorageHealthCheck::new(dir.path().to_path_buf());

        let health = checker.check().await;

        assert_eq!(health.status, ComponentHealth::Up);
        assert!(
            health.total_bytes > 0,
            "a real filesystem's total_bytes must be nonzero"
        );
        assert!(health.used_bytes <= health.total_bytes);
    }

    #[tokio::test]
    async fn a_nonexistent_path_reports_down() {
        let checker = FilesystemStorageHealthCheck::new(PathBuf::from(
            "/nonexistent/path/that/does/not/exist",
        ));

        let health = checker.check().await;

        assert_eq!(health.status.status_str(), "down");
    }
}
