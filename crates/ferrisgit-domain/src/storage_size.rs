/// Not `#[async_trait]`: implementations do synchronous filesystem I/O, so callers wrap the call in
/// `tokio::task::spawn_blocking`.
pub trait DirectorySizePort: Send + Sync {
    fn directory_size(&self, disk_path: &str) -> std::io::Result<u64>;
}
