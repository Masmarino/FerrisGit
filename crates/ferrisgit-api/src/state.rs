use std::path::PathBuf;
use std::sync::Arc;

use crate::first_setup_lock::FirstSetupLocks;
use crate::login_rate_limiter::LoginRateLimiter;
use crate::mfa_rate_limiter::MfaRateLimiter;

use ferrisgit_application::job_execution_resolver::JobExecutionResolver;
use ferrisgit_application::mailer::Mailer;
use ferrisgit_application::merge_request_activity::MergeRequestActivity;
use ferrisgit_application::passkey_ceremonies::PasskeyCeremonies;
use ferrisgit_application::single_use_tokens::SingleUseTokens;
use ferrisgit_application::use_cases::mfa::MfaService;
use ferrisgit_application::use_cases::passkeys::{PasskeyService, build_webauthn};
use ferrisgit_application::use_cases::purge_expired_job_logs::PurgeExpiredJobLogsUseCase;
use ferrisgit_application::use_cases::record_metrics_snapshot::RecordMetricsSnapshotUseCase;
use ferrisgit_application::use_cases::report_job_result::{
    AppendJobLogsUseCase, ReportJobResultUseCase,
};
use ferrisgit_domain::api_token::ApiTokenRepositoryPort;
use ferrisgit_domain::apply_suggestion_executor::ApplySuggestionExecutorPort;
use ferrisgit_domain::audit::EventPublisherPort;
use ferrisgit_domain::branch::BranchReaderPort;
use ferrisgit_domain::diff::DiffReaderPort;
use ferrisgit_domain::email::SmtpSettingsPort;
use ferrisgit_domain::group::GroupStorePort;
use ferrisgit_domain::group_membership::GroupMembershipPort;
use ferrisgit_domain::health::{HealthCheckPort, StorageHealthCheckPort};
use ferrisgit_domain::invitation::UserInvitationPort;
use ferrisgit_domain::issue::IssueStorePort;
use ferrisgit_domain::issue_comment::IssueCommentPort;
use ferrisgit_domain::job::{JobLogRetentionPort, JobStorePort};
use ferrisgit_domain::job_execution::JobExecutionPort;
use ferrisgit_domain::label::LabelStorePort;
use ferrisgit_domain::merge_executor::MergeExecutorPort;
use ferrisgit_domain::merge_request::{MergeRequestReviewPort, MergeRequestStorePort};
use ferrisgit_domain::merge_request_comment::MergeRequestCommentPort;
use ferrisgit_domain::merge_request_event::MergeRequestEventPort;
use ferrisgit_domain::metrics_snapshot::MetricsSnapshotRepositoryPort;
use ferrisgit_domain::mfa::{MfaPendingTokenPort, TotpCredentialPort};
use ferrisgit_domain::milestone::MilestoneStorePort;
use ferrisgit_domain::notification::NotificationStorePort;
use ferrisgit_domain::password_reset::PasswordResetPort;
use ferrisgit_domain::pipeline::PipelineStorePort;
use ferrisgit_domain::pipeline_events::PipelineEventPublisherPort;
use ferrisgit_domain::pipeline_file_reader::PipelineFileReaderPort;
use ferrisgit_domain::public_pages::{PublicCatalogPort, PublicPagesSettingsPort};
use ferrisgit_domain::registration::RegistrationSettingsPort;
use ferrisgit_domain::release::ReleaseStorePort;
use ferrisgit_domain::release_asset_storage::ReleaseAssetStoragePort;
use ferrisgit_domain::repository::RepositoryStorePort;
use ferrisgit_domain::repository_collaborator::RepositoryCollaboratorStorePort;
use ferrisgit_domain::repository_star::RepositoryStarStorePort;
use ferrisgit_domain::runner::RunnerRepositoryPort;
use ferrisgit_domain::settings::{RepositorySettingsStorePort, SystemSettingsStorePort};
use ferrisgit_domain::storage_size::DirectorySizePort;
use ferrisgit_domain::tag::{TagCreatorPort, TagReaderPort};
use ferrisgit_domain::user::{PasswordHasherPort, TokenIssuerPort, UserRepositoryPort};
use ferrisgit_domain::webauthn::WebauthnCredentialPort;
use ferrisgit_domain::webhook::WebhookStorePort;
use ferrisgit_domain::webhook_dispatcher::WebhookDispatcherPort;
use ferrisgit_domain::wiki::WikiStorePort;
use ferrisgit_domain::wiki_page::{WikiReaderPort, WikiWriterPort};
use ferrisgit_infrastructure::aes_gcm_encryptor::AesGcmSecretEncryptor;
use ferrisgit_infrastructure::argon2_hasher::Argon2PasswordHasher;
use ferrisgit_infrastructure::disk_space_health::FilesystemStorageHealthCheck;
use ferrisgit_infrastructure::docker_runner_executor::DockerRunnerExecutor;
use ferrisgit_infrastructure::git_apply_suggestion_executor::GitApplySuggestionExecutor;
use ferrisgit_infrastructure::git_backend::GitBackend;
use ferrisgit_infrastructure::git_merge_executor::GitMergeExecutor;
use ferrisgit_infrastructure::git_tag_creator::GitTagCreator;
use ferrisgit_infrastructure::git_wiki_writer::GitWikiWriter;
use ferrisgit_infrastructure::gix_merge_request_reader::GixMergeRequestReader;
use ferrisgit_infrastructure::gix_pipeline_file_reader::GixPipelineFileReader;
use ferrisgit_infrastructure::gix_reader::GixRepositoryReader;
use ferrisgit_infrastructure::http_webhook_dispatcher::HttpWebhookDispatcher;
use ferrisgit_infrastructure::jwt_mfa_pending_token_issuer::JwtMfaPendingTokenIssuer;
use ferrisgit_infrastructure::jwt_token_issuer::JwtTokenIssuer;
use ferrisgit_infrastructure::kubernetes::discovery::detect_default_storage_class;
use ferrisgit_infrastructure::kubernetes::pod_executor::KubernetesPodExecutor;
use ferrisgit_infrastructure::kubernetes::unavailable::UnavailableKubernetesExecutor;
use ferrisgit_infrastructure::kubernetes::watcher::PodWatcher;
use ferrisgit_infrastructure::local_release_asset_storage::LocalReleaseAssetStorage;
use ferrisgit_infrastructure::postgres::api_token_repository::PostgresApiTokenRepository;
use ferrisgit_infrastructure::postgres::backup_code_store::PostgresBackupCodeStore;
use ferrisgit_infrastructure::postgres::event_publisher::PostgresEventPublisher;
use ferrisgit_infrastructure::postgres::group_store::PostgresGroupStore;
use ferrisgit_infrastructure::postgres::health_check::PostgresHealthCheck;
use ferrisgit_infrastructure::postgres::issue_store::PostgresIssueStore;
use ferrisgit_infrastructure::postgres::job_log_retention_store::PostgresJobLogRetentionStore;
use ferrisgit_infrastructure::postgres::job_store::PostgresJobStore;
use ferrisgit_infrastructure::postgres::label_store::PostgresLabelStore;
use ferrisgit_infrastructure::postgres::merge_request_event_store::PostgresMergeRequestEventStore;
use ferrisgit_infrastructure::postgres::merge_request_store::PostgresMergeRequestStore;
use ferrisgit_infrastructure::postgres::metrics_snapshot_store::PostgresMetricsSnapshotStore;
use ferrisgit_infrastructure::postgres::milestone_store::PostgresMilestoneStore;
use ferrisgit_infrastructure::postgres::notification_store::PostgresNotificationStore;
use ferrisgit_infrastructure::postgres::password_reset_store::PostgresPasswordResetStore;
use ferrisgit_infrastructure::postgres::pipeline_store::PostgresPipelineStore;
use ferrisgit_infrastructure::postgres::public_catalog_store::PostgresPublicCatalogStore;
use ferrisgit_infrastructure::postgres::public_pages_settings_store::PostgresPublicPagesSettingsStore;
use ferrisgit_infrastructure::postgres::registration_settings_store::PostgresRegistrationSettingsStore;
use ferrisgit_infrastructure::postgres::release_store::PostgresReleaseStore;
use ferrisgit_infrastructure::postgres::repository_collaborator_store::PostgresRepositoryCollaboratorStore;
use ferrisgit_infrastructure::postgres::repository_settings_store::PostgresRepositorySettingsStore;
use ferrisgit_infrastructure::postgres::repository_star_store::PostgresRepositoryStarStore;
use ferrisgit_infrastructure::postgres::repository_store::PostgresRepositoryStore;
use ferrisgit_infrastructure::postgres::runner_repository::PostgresRunnerRepository;
use ferrisgit_infrastructure::postgres::smtp_settings_store::PostgresSmtpSettingsStore;
use ferrisgit_infrastructure::postgres::system_settings_store::PostgresSystemSettingsStore;
use ferrisgit_infrastructure::postgres::totp_credential_store::PostgresTotpCredentialStore;
use ferrisgit_infrastructure::postgres::user_invitation_store::PostgresUserInvitationStore;
use ferrisgit_infrastructure::postgres::user_repository::PostgresUserRepository;
use ferrisgit_infrastructure::postgres::webauthn_credential_store::PostgresWebauthnCredentialStore;
use ferrisgit_infrastructure::postgres::webhook_store::PostgresWebhookStore;
use ferrisgit_infrastructure::postgres::wiki_store::PostgresWikiStore;
use ferrisgit_infrastructure::smtp_email_sender::SmtpEmailSender;
use sqlx::PgPool;

use crate::config::Config;

const ACCOUNT_CREATION_MAX_ATTEMPTS: u32 = 10;
const ACCOUNT_CREATION_WINDOW: std::time::Duration = std::time::Duration::from_secs(300);

const PASSKEY_START_MAX_ATTEMPTS: u32 = 30;
const PASSKEY_START_WINDOW: std::time::Duration = std::time::Duration::from_secs(300);

const PUBLIC_PAGES_MAX_REQUESTS: u32 = 120;
const PUBLIC_PAGES_WINDOW: std::time::Duration = std::time::Duration::from_secs(60);

#[derive(Clone)]
pub struct AppState {
    pub users: Arc<dyn UserRepositoryPort>,
    pub api_tokens: Arc<dyn ApiTokenRepositoryPort>,
    pub repositories: Arc<dyn RepositoryStorePort>,
    pub repository_collaborators: Arc<dyn RepositoryCollaboratorStorePort>,
    pub repository_stars: Arc<dyn RepositoryStarStorePort>,
    pub groups: Arc<dyn GroupStorePort>,
    pub group_membership: Arc<dyn GroupMembershipPort>,
    pub hasher: Arc<dyn PasswordHasherPort>,
    pub token_issuer: Arc<dyn TokenIssuerPort>,
    pub events: Arc<dyn EventPublisherPort>,
    pub git_backend: Arc<GitBackend>,
    pub git_reader: Arc<GixRepositoryReader>,
    pub config: Arc<Config>,
    pub pipelines: Arc<dyn PipelineStorePort>,
    pub jobs: Arc<dyn JobStorePort>,
    pub job_log_retention: Arc<dyn JobLogRetentionPort>,
    pub runners: Arc<dyn RunnerRepositoryPort>,
    pub pipeline_events: Arc<dyn PipelineEventPublisherPort>,
    pub system_settings: Arc<dyn SystemSettingsStorePort>,
    pub repository_settings: Arc<dyn RepositorySettingsStorePort>,
    pub smtp_settings: Arc<dyn SmtpSettingsPort>,
    pub mailer: Arc<Mailer>,
    pub job_execution: Arc<JobExecutionResolver>,
    pub pipeline_file_reader: Arc<dyn PipelineFileReaderPort>,
    pub merge_requests: Arc<dyn MergeRequestStorePort>,
    pub merge_request_comments: Arc<dyn MergeRequestCommentPort>,
    pub merge_request_reviews: Arc<dyn MergeRequestReviewPort>,
    pub merge_request_events: Arc<dyn MergeRequestEventPort>,
    pub merge_request_activity: Arc<MergeRequestActivity>,
    pub notifications: Arc<dyn NotificationStorePort>,
    pub issues: Arc<dyn IssueStorePort>,
    pub issue_comments: Arc<dyn IssueCommentPort>,
    pub milestones: Arc<dyn MilestoneStorePort>,
    pub labels: Arc<dyn LabelStorePort>,
    pub branch_reader: Arc<dyn BranchReaderPort>,
    pub diff_reader: Arc<dyn DiffReaderPort>,
    pub merge_executor: Arc<dyn MergeExecutorPort>,
    pub suggestion_executor: Arc<dyn ApplySuggestionExecutorPort>,
    pub webhooks: Arc<dyn WebhookDispatcherPort>,
    pub webhook_store: Arc<dyn WebhookStorePort>,
    pub releases: Arc<dyn ReleaseStorePort>,
    pub tags: Arc<dyn TagReaderPort>,
    pub tag_creator: Arc<dyn TagCreatorPort>,
    pub release_asset_storage: Arc<dyn ReleaseAssetStoragePort>,
    pub wikis: Arc<dyn WikiStorePort>,
    pub wiki_reader: Arc<dyn WikiReaderPort>,
    pub wiki_writer: Arc<dyn WikiWriterPort>,
    pub login_rate_limiter: Arc<LoginRateLimiter>,
    pub register_rate_limiter: Arc<LoginRateLimiter>,
    /// Shared by activate and reset-password: both redeem a token, so one budget caps a client's argon2 cost
    /// across the two.
    pub activation_rate_limiter: Arc<LoginRateLimiter>,
    pub invitations: Arc<dyn UserInvitationPort>,
    pub password_resets: Arc<dyn PasswordResetPort>,
    pub registration_settings: Arc<dyn RegistrationSettingsPort>,
    pub public_pages_settings: Arc<dyn PublicPagesSettingsPort>,
    pub public_catalog: Arc<dyn PublicCatalogPort>,
    /// One per-IP budget for all the anonymous `/api/public/*` routes.
    pub public_rate_limiter: Arc<LoginRateLimiter>,
    pub totp_credentials: Arc<dyn TotpCredentialPort>,
    pub passkey_credentials: Arc<dyn WebauthnCredentialPort>,
    /// Without a usable `PUBLIC_URL` the passkey routes answer 503 and TOTP keeps working.
    pub passkeys: Arc<PasskeyService>,
    pub passkey_start_limiter: Arc<LoginRateLimiter>,
    /// Doesn't publish security events or bump the epoch, the routes do.
    pub mfa: Arc<MfaService>,
    pub mfa_pending: Arc<dyn MfaPendingTokenPort>,
    /// Tokens that already finished a login or enrolment. Only a success spends one.
    pub mfa_spent_tokens: SingleUseTokens,
    pub first_setup_locks: Arc<FirstSetupLocks>,
    pub mfa_limiter: Arc<MfaRateLimiter>,
    /// Tests only: nothing at runtime changes it, so production can't turn MFA off. Tests that aren't about MFA set
    /// it to false so login returns a plain session.
    pub mfa_enforced: bool,
    /// Caps the git request bodies buffered at once before authentication (see `MAX_BUFFERED_GIT_BODY_MB`).
    pub git_body_semaphore: Arc<tokio::sync::Semaphore>,
    pub metrics_snapshots: Arc<dyn MetricsSnapshotRepositoryPort>,
    pub health_check: Arc<dyn HealthCheckPort>,
    pub storage_health: Arc<dyn StorageHealthCheckPort>,
    pub directory_size: Arc<dyn DirectorySizePort>,
    /// Detected at boot. Pre-fills `k8s_namespace` in the admin settings and is the fallback while it's unset.
    pub detected_k8s_namespace: Option<String>,
    /// Best-effort guess that only pre-fills `k8s_cache_storage_class`. Never a fallback: a wrong ReadWriteMany
    /// guess could quietly break caches.
    pub detected_k8s_default_storage_class: Option<String>,
    pub started_at: std::time::Instant,
    /// Built up front because the hourly timer in `main.rs` runs it outside any request.
    pub record_metrics_snapshot: Arc<RecordMetricsSnapshotUseCase>,
    pub purge_expired_job_logs: Arc<PurgeExpiredJobLogsUseCase>,
}

impl AppState {
    pub async fn new(pool: PgPool, config: Config) -> Self {
        let storage_root = PathBuf::from(&config.storage_root);
        let config = Arc::new(config);
        let started_at = std::time::Instant::now();
        let jobs: Arc<dyn JobStorePort> = Arc::new(PostgresJobStore::new(pool.clone()));
        let job_log_retention: Arc<dyn JobLogRetentionPort> =
            Arc::new(PostgresJobLogRetentionStore::new(pool.clone()));
        let pipelines: Arc<dyn PipelineStorePort> =
            Arc::new(PostgresPipelineStore::new(pool.clone()));
        let system_settings: Arc<dyn SystemSettingsStorePort> =
            Arc::new(PostgresSystemSettingsStore::new(pool.clone()));
        let pipeline_events: Arc<dyn PipelineEventPublisherPort> =
            Arc::new(PostgresEventPublisher::new(pool.clone()));
        let repositories: Arc<dyn RepositoryStorePort> =
            Arc::new(PostgresRepositoryStore::new(pool.clone()));
        let users: Arc<dyn UserRepositoryPort> =
            Arc::new(PostgresUserRepository::new(pool.clone()));
        let notifications: Arc<dyn NotificationStorePort> =
            Arc::new(PostgresNotificationStore::new(pool.clone()));
        let issue_store = Arc::new(PostgresIssueStore::new(pool.clone()));
        let issues: Arc<dyn IssueStorePort> = issue_store.clone();
        let issue_comments: Arc<dyn IssueCommentPort> = issue_store;
        let milestones: Arc<dyn MilestoneStorePort> =
            Arc::new(PostgresMilestoneStore::new(pool.clone()));
        let labels: Arc<dyn LabelStorePort> = Arc::new(PostgresLabelStore::new(pool.clone()));
        let group_store = Arc::new(PostgresGroupStore::new(pool.clone()));
        let groups: Arc<dyn GroupStorePort> = group_store.clone();
        let group_membership: Arc<dyn GroupMembershipPort> = group_store;
        let secret_encryptor =
            Arc::new(AesGcmSecretEncryptor::new(&config.settings_encryption_key));
        let hasher: Arc<dyn PasswordHasherPort> = Arc::new(Argon2PasswordHasher::new());
        let mfa_pending: Arc<dyn MfaPendingTokenPort> =
            Arc::new(JwtMfaPendingTokenIssuer::new(config.jwt_secret.clone()));
        let totp_credentials: Arc<dyn TotpCredentialPort> = Arc::new(
            PostgresTotpCredentialStore::new(pool.clone(), secret_encryptor.clone()),
        );
        let passkey_credentials: Arc<dyn WebauthnCredentialPort> =
            Arc::new(PostgresWebauthnCredentialStore::new(pool.clone()));
        let mfa = Arc::new(MfaService::new(
            totp_credentials.clone(),
            Arc::new(PostgresBackupCodeStore::new(pool.clone())),
            users.clone(),
            hasher.clone(),
            passkey_credentials.clone(),
        ));
        // `None` (with a warning) when the public URL can't carry passkeys.
        let passkeys = Arc::new(PasskeyService::new(
            build_webauthn(&config.public_url).map(Arc::new),
            passkey_credentials.clone(),
            Arc::new(PasskeyCeremonies::new()),
            users.clone(),
        ));
        let smtp_settings: Arc<dyn SmtpSettingsPort> = Arc::new(PostgresSmtpSettingsStore::new(
            pool.clone(),
            secret_encryptor.clone(),
        ));
        let mailer = Arc::new(Mailer::new(Arc::new(SmtpEmailSender::new(
            smtp_settings.clone(),
        ))));
        let webhook_store: Arc<dyn WebhookStorePort> = Arc::new(PostgresWebhookStore::new(
            pool.clone(),
            secret_encryptor.clone(),
        ));
        let webhooks: Arc<dyn WebhookDispatcherPort> =
            Arc::new(HttpWebhookDispatcher::new(webhook_store.clone()));

        let docker_executor: Arc<dyn JobExecutionPort> =
            Arc::new(DockerRunnerExecutor::new(jobs.clone()));

        // Not `kube::Client::try_default()`: we want the inferred config too, for its default namespace.
        let kubernetes_config = kube::Config::infer().await.ok();
        let kubernetes_client = kubernetes_config
            .as_ref()
            .and_then(|cfg| kube::Client::try_from(cfg.clone()).ok());
        let detected_k8s_namespace = kubernetes_config
            .as_ref()
            .map(|cfg| cfg.default_namespace.clone());
        let default_k8s_namespace = detected_k8s_namespace
            .clone()
            .unwrap_or_else(|| "ferrisgit-jobs".to_string());
        let detected_k8s_default_storage_class = match &kubernetes_client {
            Some(client) => detect_default_storage_class(client).await,
            None => None,
        };
        let kubernetes_executor: Arc<dyn JobExecutionPort> = match &kubernetes_client {
            Some(client) => Arc::new(KubernetesPodExecutor::new(
                client.clone(),
                pipelines.clone(),
                jobs.clone(),
                system_settings.clone(),
                pipeline_events.clone(),
                default_k8s_namespace.clone(),
            )),
            None => {
                // infer() never checks connectivity, so this just means no config was found.
                tracing::warn!(
                    "no Kubernetes configuration found at startup; the Kubernetes execution engine will explicitly fail if selected"
                );
                Arc::new(UnavailableKubernetesExecutor)
            }
        };
        let job_execution = Arc::new(JobExecutionResolver::new(
            docker_executor,
            kubernetes_executor,
        ));

        if let Some(client) = kubernetes_client {
            // Read once: the watcher won't notice a later change to k8s_namespace.
            let namespace = system_settings
                .get()
                .await
                .ok()
                .and_then(|s| s.k8s_namespace)
                .unwrap_or_else(|| default_k8s_namespace.clone());
            let report = Arc::new(ReportJobResultUseCase::new(
                jobs.clone(),
                pipelines.clone(),
                pipeline_events.clone(),
                job_execution.clone(),
            ));
            let append_logs = Arc::new(AppendJobLogsUseCase::new(jobs.clone()));
            let watcher = Arc::new(PodWatcher::new(
                client,
                namespace,
                report,
                append_logs,
                jobs.clone(),
                pipelines.clone(),
                repositories.clone(),
                users.clone(),
                notifications.clone(),
                webhooks.clone(),
            ));
            tokio::spawn(async move { watcher.run().await });
        }

        let merge_request_reader = Arc::new(GixMergeRequestReader::new(storage_root.clone()));
        let merge_request_store = Arc::new(PostgresMergeRequestStore::new(pool.clone()));
        let merge_request_events: Arc<dyn MergeRequestEventPort> =
            Arc::new(PostgresMergeRequestEventStore::new(pool.clone()));
        let merge_request_activity =
            Arc::new(MergeRequestActivity::new(merge_request_events.clone()));

        let releases: Arc<dyn ReleaseStorePort> = Arc::new(PostgresReleaseStore::new(pool.clone()));
        let tag_creator: Arc<dyn TagCreatorPort> =
            Arc::new(GitTagCreator::new(storage_root.clone()));
        let wikis: Arc<dyn WikiStorePort> = Arc::new(PostgresWikiStore::new(pool.clone()));
        let wiki_writer: Arc<dyn WikiWriterPort> =
            Arc::new(GitWikiWriter::new(storage_root.clone()));
        let release_asset_storage: Arc<dyn ReleaseAssetStoragePort> =
            Arc::new(LocalReleaseAssetStorage::new(storage_root.clone()));

        let metrics_snapshots: Arc<dyn MetricsSnapshotRepositoryPort> =
            Arc::new(PostgresMetricsSnapshotStore::new(pool.clone()));
        let health_check: Arc<dyn HealthCheckPort> =
            Arc::new(PostgresHealthCheck::new(pool.clone()));
        let storage_health: Arc<dyn StorageHealthCheckPort> =
            Arc::new(FilesystemStorageHealthCheck::new(storage_root.clone()));
        let git_backend = Arc::new(GitBackend::new(storage_root.clone()));
        let directory_size: Arc<dyn DirectorySizePort> = git_backend.clone();
        let record_metrics_snapshot = Arc::new(RecordMetricsSnapshotUseCase::new(
            repositories.clone(),
            users.clone(),
            directory_size.clone(),
            metrics_snapshots.clone(),
        ));

        let purge_expired_job_logs = Arc::new(PurgeExpiredJobLogsUseCase::new(
            system_settings.clone(),
            job_log_retention.clone(),
        ));

        Self {
            users,
            api_tokens: Arc::new(PostgresApiTokenRepository::new(pool.clone())),
            repositories,
            repository_collaborators: Arc::new(PostgresRepositoryCollaboratorStore::new(
                pool.clone(),
            )),
            repository_stars: Arc::new(PostgresRepositoryStarStore::new(pool.clone())),
            groups,
            group_membership,
            hasher,
            token_issuer: Arc::new(JwtTokenIssuer::new(config.jwt_secret.clone())),
            events: Arc::new(PostgresEventPublisher::new(pool.clone())),
            git_backend,
            git_reader: Arc::new(GixRepositoryReader),
            pipelines,
            jobs: jobs.clone(),
            job_log_retention,
            runners: Arc::new(PostgresRunnerRepository::new(pool.clone())),
            pipeline_events,
            system_settings,
            repository_settings: Arc::new(PostgresRepositorySettingsStore::new(
                pool.clone(),
                secret_encryptor.clone(),
            )),
            smtp_settings,
            mailer,
            job_execution,
            pipeline_file_reader: Arc::new(GixPipelineFileReader::new(storage_root.clone())),
            merge_requests: merge_request_store.clone(),
            merge_request_comments: merge_request_store.clone(),
            merge_request_reviews: merge_request_store,
            merge_request_events,
            merge_request_activity,
            notifications,
            issues,
            issue_comments,
            milestones,
            labels,
            branch_reader: merge_request_reader.clone(),
            diff_reader: merge_request_reader.clone(),
            merge_executor: Arc::new(GitMergeExecutor::new(storage_root.clone())),
            suggestion_executor: Arc::new(GitApplySuggestionExecutor::new(storage_root.clone())),
            webhooks,
            webhook_store,
            releases,
            tags: merge_request_reader.clone(),
            tag_creator,
            release_asset_storage,
            wikis,
            wiki_reader: merge_request_reader,
            wiki_writer,
            config,
            login_rate_limiter: Arc::new(LoginRateLimiter::default()),
            register_rate_limiter: Arc::new(LoginRateLimiter::with_limits(
                ACCOUNT_CREATION_MAX_ATTEMPTS,
                ACCOUNT_CREATION_WINDOW,
            )),
            activation_rate_limiter: Arc::new(LoginRateLimiter::with_limits(
                ACCOUNT_CREATION_MAX_ATTEMPTS,
                ACCOUNT_CREATION_WINDOW,
            )),
            invitations: Arc::new(PostgresUserInvitationStore::new(pool.clone())),
            password_resets: Arc::new(PostgresPasswordResetStore::new(pool.clone())),
            registration_settings: Arc::new(PostgresRegistrationSettingsStore::new(pool.clone())),
            public_pages_settings: Arc::new(PostgresPublicPagesSettingsStore::new(pool.clone())),
            public_catalog: Arc::new(PostgresPublicCatalogStore::new(pool.clone())),
            public_rate_limiter: Arc::new(LoginRateLimiter::with_limits(
                PUBLIC_PAGES_MAX_REQUESTS,
                PUBLIC_PAGES_WINDOW,
            )),
            totp_credentials,
            passkey_credentials,
            passkeys,
            passkey_start_limiter: Arc::new(LoginRateLimiter::with_limits(
                PASSKEY_START_MAX_ATTEMPTS,
                PASSKEY_START_WINDOW,
            )),
            mfa,
            mfa_pending,
            mfa_spent_tokens: SingleUseTokens::new(std::time::Duration::from_secs(600)),
            first_setup_locks: Arc::new(FirstSetupLocks::default()),
            mfa_limiter: Arc::new(MfaRateLimiter::default()),
            mfa_enforced: true,
            git_body_semaphore: Arc::new(tokio::sync::Semaphore::new(
                crate::routes::git_http::MAX_CONCURRENT_GIT_BODY_BUFFERS,
            )),
            metrics_snapshots,
            health_check,
            storage_health,
            directory_size,
            detected_k8s_namespace,
            detected_k8s_default_storage_class,
            started_at,
            record_metrics_snapshot,
            purge_expired_job_logs,
        }
    }
}
