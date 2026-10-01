mod client;
mod config;
mod executor;

use client::RunnerClient;
use config::RunnerConfig;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let config = RunnerConfig::from_env();
    let client = RunnerClient::new(config.server_url.clone(), config.token.clone());
    tracing::info!(tags = ?config.tags, poll_interval_secs = config.poll_interval_secs, "ferrisgit-runner starting");

    loop {
        match client.claim_job(&config.tags).await {
            Ok(Some(job)) => {
                tracing::info!(job_id = %job.id, stage = %job.stage, name = %job.name, "claimed job");
                let status = executor::run_job(&client, &job, &config.workdir_root).await;
                tracing::info!(job_id = %job.id, status = ?status, "job finished");
                if let Err(err) = client.report_result(job.id, status).await {
                    tracing::error!(error = %err, job_id = %job.id, "failed to report job result");
                }
            }
            Ok(None) => {}
            Err(err) => tracing::error!(error = %err, "failed to claim job"),
        }
        tokio::time::sleep(std::time::Duration::from_secs(config.poll_interval_secs)).await;
    }
}
