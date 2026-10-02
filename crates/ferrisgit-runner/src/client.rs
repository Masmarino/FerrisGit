use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimedJob {
    pub id: Uuid,
    pub stage: String,
    pub name: String,
    pub image: String,
    pub script: Vec<String>,
    pub variables: BTreeMap<String, String>,
    pub repository_owner: String,
    pub repository_name: String,
    pub commit_sha: String,
    pub ci_variables: BTreeMap<String, String>,
    #[serde(default)]
    pub masked_values: Vec<String>,
}

/// Nothing tells this runner to stop mid-execution, so a canceled job still runs to completion and reports what it
/// observed.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JobResultStatus {
    Success,
    Failed,
}

pub struct RunnerClient {
    http: reqwest::Client,
    pub server_url: String,
    pub token: String,
}

impl RunnerClient {
    pub fn new(server_url: String, token: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            server_url,
            token,
        }
    }

    pub async fn claim_job(&self, tags: &[String]) -> Result<Option<ClaimedJob>, reqwest::Error> {
        let response = self
            .http
            .post(format!("{}/api/runner/jobs/claim", self.server_url))
            .bearer_auth(&self.token)
            .json(&serde_json::json!({ "tags": tags }))
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::NO_CONTENT {
            return Ok(None);
        }
        Ok(Some(response.error_for_status()?.json().await?))
    }

    pub async fn append_logs(&self, job_id: Uuid, chunk: &str) -> Result<(), reqwest::Error> {
        self.http
            .post(format!("{}/api/runner/jobs/{job_id}/logs", self.server_url))
            .bearer_auth(&self.token)
            .json(&serde_json::json!({ "chunk": chunk }))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    pub async fn report_result(
        &self,
        job_id: Uuid,
        status: JobResultStatus,
    ) -> Result<(), reqwest::Error> {
        self.http
            .post(format!(
                "{}/api/runner/jobs/{job_id}/result",
                self.server_url
            ))
            .bearer_auth(&self.token)
            .json(&serde_json::json!({ "status": status }))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }
}

/// Authenticated git clone URL, with the runner token as Basic Auth (the smart-HTTP route accepts runner tokens like
/// user tokens). The username is never checked, so any placeholder works.
///
/// `git clone` persists this URL into `.git/config`, so `executor::strip_git_metadata` deletes `.git` before the
/// container starts.
pub fn build_clone_url(server_url: &str, token: &str, owner: &str, repo: &str) -> String {
    let (scheme, host) = server_url.split_once("://").unwrap_or(("http", server_url));
    format!("{scheme}://runner:{token}@{host}/{owner}/{repo}.git")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_clone_url_embeds_the_token_as_basic_auth_credentials() {
        let url = build_clone_url("http://localhost:8080", "fgr_abc123", "florian", "hello");
        assert_eq!(
            url,
            "http://runner:fgr_abc123@localhost:8080/florian/hello.git"
        );
    }

    #[test]
    fn build_clone_url_preserves_https() {
        let url = build_clone_url("https://ci.example.com", "fgr_abc123", "florian", "hello");
        assert_eq!(
            url,
            "https://runner:fgr_abc123@ci.example.com/florian/hello.git"
        );
    }
}
