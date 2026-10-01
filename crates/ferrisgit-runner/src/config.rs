pub struct RunnerConfig {
    pub server_url: String,
    pub token: String,
    pub tags: Vec<String>,
    pub poll_interval_secs: u64,
    pub workdir_root: std::path::PathBuf,
}

impl RunnerConfig {
    pub fn from_env() -> Self {
        Self {
            server_url: std::env::var("FERRISGIT_SERVER_URL")
                .expect("FERRISGIT_SERVER_URL must be set"),
            token: std::env::var("FERRISGIT_RUNNER_TOKEN")
                .expect("FERRISGIT_RUNNER_TOKEN must be set"),
            tags: parse_tags(&std::env::var("FERRISGIT_RUNNER_TAGS").unwrap_or_default()),
            poll_interval_secs: std::env::var("FERRISGIT_POLL_INTERVAL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5),
            workdir_root: std::env::var("FERRISGIT_WORKDIR_ROOT")
                .unwrap_or_else(|_| "/tmp/ferrisgit-runner".to_string())
                .into(),
        }
    }
}

fn parse_tags(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tags_splits_trims_and_drops_empty_entries() {
        assert_eq!(
            parse_tags("docker, linux ,,arm64"),
            vec!["docker", "linux", "arm64"]
        );
        assert_eq!(parse_tags(""), Vec::<String>::new());
    }
}
