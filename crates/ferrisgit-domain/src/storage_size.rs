/// Not async: it does blocking filesystem I/O, so callers run it in `tokio::task::spawn_blocking`.
pub trait DirectorySizePort: Send + Sync {
    fn directory_size(&self, disk_path: &str) -> std::io::Result<u64>;
}
