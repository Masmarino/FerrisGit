use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use ferrisgit_application::use_cases::authenticate_git_request::{
    AuthenticateGitRequestUseCase, GitAccess,
};
use ferrisgit_application::use_cases::create_pipeline::CreatePipelineUseCase;
use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::merge_request::MergeRequestStatus;
use ferrisgit_domain::wiki::{NewWiki, wiki_disk_path_for};
use ferrisgit_infrastructure::git_backend::SmartHttpRequest;
use uuid::Uuid;

use crate::git_auth::parse_basic_auth;
use crate::state::AppState;

/// Requires at least one segment (owner or group) before the `.git` segment.
fn find_git_segment(segments: &[&str]) -> Option<usize> {
    let idx = segments.iter().position(|s| s.ends_with(".git"))?;
    (idx > 0).then_some(idx)
}

pub fn is_git_request_path(path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    find_git_segment(&segments).is_some()
}

/// The router's static body limit is only a ceiling. This enforces the admin-configured
/// `max_push_size_mb`.
fn exceeds_configured_limit(body_len: usize, max_push_size_mb: i32) -> bool {
    body_len > (max_push_size_mb as usize) * 1024 * 1024
}

/// The `info/refs?service=git-receive-pack` advertisement counts as a write so it is not
/// readable anonymously on public repos.
fn classify_access(rest: &str, query_string: &str) -> GitAccess {
    let is_receive_pack_info_refs =
        rest == "info/refs" && query_string.contains("service=git-receive-pack");
    if rest.ends_with("git-receive-pack") || is_receive_pack_info_refs {
        GitAccess::Write
    } else {
        GitAccess::Read
    }
}

/// The smart-HTTP body is not parsed for ref updates, so any successful receive-pack
/// triggers at most one pipeline for the repository's current `HEAD`.
fn should_trigger_pipeline_creation(access: &GitAccess, rest: &str, status: u16) -> bool {
    matches!(access, GitAccess::Write) && rest.ends_with("git-receive-pack") && status == 200
}

/// Awaited before the git response is built so a client that pushes and immediately lists
/// pipelines sees the new one, at the cost of delaying `git push` by the `HEAD` read and
/// pipeline creation. Failures are logged and swallowed.
async fn trigger_pipeline_creation(
    state: &AppState,
    repo: &ferrisgit_domain::repository::Repository,
    triggered_by: Uuid,
) {
    let full_path = std::path::PathBuf::from(&state.config.storage_root).join(&repo.disk_path);
    let git_reader = state.git_reader.clone();
    let latest_commit_sha = match tokio::task::spawn_blocking(move || {
        git_reader.list_commits(&full_path, 1)
    })
    .await
    {
        Ok(Ok(commits)) => commits.into_iter().next().map(|c| c.sha),
        Ok(Err(err)) => {
            tracing::error!(error = %err, repository_id = %repo.id, "failed to read HEAD after push");
            None
        }
        Err(err) => {
            tracing::error!(error = %err, repository_id = %repo.id, "blocking task panicked reading HEAD after push");
            None
        }
    };
    let Some(commit_sha) = latest_commit_sha else {
        return;
    };

    let use_case = CreatePipelineUseCase::new(
        state.repository_settings.clone(),
        state.system_settings.clone(),
        state.pipelines.clone(),
        state.jobs.clone(),
        state.pipeline_file_reader.clone(),
        state.pipeline_events.clone(),
        state.job_execution.clone(),
        state.repositories.clone(),
        state.users.clone(),
        state.notifications.clone(),
        state.webhooks.clone(),
    );
    if let Err(err) = use_case
        .execute(repo.id, &repo.disk_path, &commit_sha, triggered_by)
        .await
    {
        tracing::error!(error = %err, repository_id = %repo.id, commit_sha, "failed to create pipeline after push");
    }
}

/// The journal keeps the last-seen tip per source branch: a first sighting only stores it,
/// a changed tip records the move. Failures are logged and swallowed.
async fn record_pushed_commits(
    state: &AppState,
    repo: &ferrisgit_domain::repository::Repository,
    actor: Uuid,
) {
    let branches = match state.branch_reader.list_branches(&repo.disk_path).await {
        Ok(branches) => branches,
        Err(err) => {
            tracing::error!(error = %err, repository_id = %repo.id, "failed to list branches to record pushed commits");
            return;
        }
    };
    let merge_requests = match state
        .merge_requests
        .list_for_repository_filtered(repo.id, None, None)
        .await
    {
        Ok(merge_requests) => merge_requests,
        Err(err) => {
            tracing::error!(error = %err, repository_id = %repo.id, "failed to list merge requests to record pushed commits");
            return;
        }
    };
    for merge_request in merge_requests
        .iter()
        .filter(|mr| mr.status == MergeRequestStatus::Open)
    {
        let Some(tip) = branches
            .iter()
            .find(|b| b.name == merge_request.source_branch)
            .map(|b| b.tip_sha.as_str())
        else {
            continue;
        };
        match state.merge_request_events.head_sha(merge_request.id).await {
            Ok(None) => {
                if let Err(err) = state
                    .merge_request_events
                    .set_head_sha(merge_request.id, tip)
                    .await
                {
                    tracing::error!(error = %err, merge_request_id = %merge_request.id, "failed to store the initial head of a merge request");
                }
            }
            Ok(Some(previous)) if previous != tip => {
                state
                    .merge_request_activity
                    .commits_pushed(merge_request.id, actor, &previous, tip)
                    .await;
                if let Err(err) = state
                    .merge_request_events
                    .set_head_sha(merge_request.id, tip)
                    .await
                {
                    tracing::error!(error = %err, merge_request_id = %merge_request.id, "failed to advance the head of a merge request");
                }
            }
            Ok(Some(_)) => {}
            Err(err) => {
                tracing::error!(error = %err, merge_request_id = %merge_request.id, "failed to read the head of a merge request")
            }
        }
    }
}

/// Repoints the wiki's `HEAD` if a first push landed on a branch other than `main`.
/// Failures are logged and swallowed.
async fn heal_wiki_head_after_push(state: &AppState, wiki_disk_path: &str) {
    if let Err(err) = state.wiki_writer.heal_dangling_head(wiki_disk_path).await {
        tracing::error!(error = %err, wiki_disk_path, "failed to heal a possibly-dangling wiki HEAD after push");
    }
}

/// Must run before `trigger_pipeline_creation`, which reads `HEAD`. A first push leaves it
/// dangling.
async fn heal_repo_head_after_push(state: &AppState, disk_path: &str) {
    if let Err(err) = state.git_backend.heal_dangling_head(disk_path).await {
        tracing::error!(error = %err, disk_path, "failed to heal a possibly-dangling repository HEAD after push");
    }
}

/// Infrastructure failures give a 500 without a challenge; every other variant is a uniform
/// 401 so the failing check is never revealed.
fn map_auth_error(err: &DomainError) -> Response {
    match err {
        DomainError::Infrastructure(msg) => {
            tracing::error!(error = %msg, "infrastructure error authenticating git request");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
        _ => (
            StatusCode::UNAUTHORIZED,
            [(
                axum::http::header::WWW_AUTHENTICATE,
                "Basic realm=\"FerrisGit\"",
            )],
            "unauthorized",
        )
            .into_response(),
    }
}

pub async fn git_smart_http(state: AppState, req: axum::extract::Request) -> Response {
    let (parts, body) = req.into_parts();
    let path = parts.uri.path().to_string();
    let segments: Vec<String> = path
        .split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    let segment_refs: Vec<&str> = segments.iter().map(String::as_str).collect();

    let Some(git_idx) = find_git_segment(&segment_refs) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let max_push_size_mb = match state.system_settings.get().await {
        Ok(settings) => settings.max_push_size_mb,
        Err(err) => {
            tracing::error!(error = %err, "failed to read system settings for push-size enforcement");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // The permit is held only while buffering: it caps pre-auth memory exposure, not
    // authenticated throughput.
    let permit = state.git_body_semaphore.clone().acquire_owned().await;
    let body_bytes = axum::body::to_bytes(body, MAX_BUFFERED_GIT_BODY_MB * 1024 * 1024).await;
    drop(permit);
    let Ok(body_bytes) = body_bytes else {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    };
    if exceeds_configured_limit(body_bytes.len(), max_push_size_mb) {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    }

    let mut resolve_segments = segments[..git_idx].to_vec();
    let repo_git_segment = &segments[git_idx];
    let (repo_name, is_wiki) = match repo_git_segment.strip_suffix(".wiki.git") {
        Some(name) => (name.to_string(), true),
        None => (
            repo_git_segment
                .strip_suffix(".git")
                .unwrap_or(repo_git_segment)
                .to_string(),
            false,
        ),
    };
    resolve_segments.push(repo_name);
    let rest = segments[git_idx + 1..].join("/");
    let query_string = parts.uri.query().unwrap_or("").to_string();

    let resolve_use_case = ferrisgit_application::use_cases::resolve_path::ResolvePathUseCase::new(
        state.users.clone(),
        state.groups.clone(),
        state.repositories.clone(),
    );
    let repo = match resolve_use_case.execute(&resolve_segments).await {
        Ok(ferrisgit_application::use_cases::resolve_path::ResolvedPath::PersonalRepository(
            repo,
        )) => repo,
        Ok(ferrisgit_application::use_cases::resolve_path::ResolvedPath::GroupRepository {
            repository,
            ..
        }) => repository,
        // Group paths and unresolvable paths get the same 401 as any auth failure so existence
        // is never revealed.
        _ => return map_auth_error(&DomainError::NotFound("repository".to_string())),
    };

    let access = classify_access(&rest, &query_string);
    let credentials = parse_basic_auth(&parts.headers);

    let use_case = AuthenticateGitRequestUseCase::new(
        state.api_tokens.clone(),
        state.events.clone(),
        state.runners.clone(),
        state.repository_collaborators.clone(),
        state.groups.clone(),
        state.group_membership.clone(),
    );
    let (repo, pusher_id) = match use_case.execute(credentials, repo, access).await {
        Ok(resolved) => resolved,
        Err(err) => return map_auth_error(&err),
    };

    let disk_path = if is_wiki {
        match access {
            GitAccess::Read => match state.wikis.find_by_repository_id(repo.id).await {
                Ok(Some(wiki)) => wiki.disk_path,
                Ok(None) => return StatusCode::NOT_FOUND.into_response(),
                Err(err) => {
                    tracing::error!(error = %err, "failed to look up wiki for read access");
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            },
            GitAccess::Write => {
                let wiki_disk_path = wiki_disk_path_for(&repo.disk_path);
                if let Err(err) = state
                    .wiki_writer
                    .ensure_wiki_repo_exists(&wiki_disk_path)
                    .await
                {
                    tracing::error!(error = %err, "failed to lazily initialize wiki repo for push");
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
                match state
                    .wikis
                    .find_or_create(NewWiki {
                        repository_id: repo.id,
                        disk_path: wiki_disk_path.clone(),
                    })
                    .await
                {
                    Ok(wiki) => wiki.disk_path,
                    Err(err) => {
                        tracing::error!(error = %err, "failed to record wiki row for push");
                        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                    }
                }
            }
        }
    } else {
        repo.disk_path.clone()
    };

    let content_type = parts
        .headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let smart_http_request = SmartHttpRequest {
        path_info: format!("/{rest}"),
        method: parts.method.to_string(),
        query_string,
        content_type,
        body: body_bytes.to_vec(),
    };

    match state
        .git_backend
        .handle_smart_http(&disk_path, smart_http_request)
        .await
    {
        Ok(response) => {
            if !is_wiki && should_trigger_pipeline_creation(&access, &rest, response.status) {
                heal_repo_head_after_push(&state, &disk_path).await;
                if let Some(pusher_id) = pusher_id {
                    trigger_pipeline_creation(&state, &repo, pusher_id).await;
                    record_pushed_commits(&state, &repo, pusher_id).await;
                }
            }
            // `should_trigger_pipeline_creation` here just means "successful git-receive-pack write"
            if is_wiki && should_trigger_pipeline_creation(&access, &rest, response.status) {
                heal_wiki_head_after_push(&state, &disk_path).await;
            }
            let mut builder = Response::builder()
                .status(StatusCode::from_u16(response.status).unwrap_or(StatusCode::OK));
            for (name, value) in &response.headers {
                if !name.eq_ignore_ascii_case("content-type") {
                    builder = builder.header(name, value);
                }
            }
            match builder
                .header(axum::http::header::CONTENT_TYPE, response.content_type)
                .body(axum::body::Body::from(response.body))
            {
                Ok(response) => response,
                Err(err) => {
                    tracing::error!(error = %err, "git http-backend produced an invalid response header");
                    StatusCode::INTERNAL_SERVER_ERROR.into_response()
                }
            }
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

/// Static ceiling on a buffered body, in MiB. It is the only limit applied before
/// authentication (axum buffers the whole body), so it must stay close to a realistic
/// maximum. Pushes above it get 413 regardless of `max_push_size_mb`. A Content-Length
/// pre-check would not help: git clients send large pushes chunked.
const MAX_BUFFERED_GIT_BODY_MB: usize = 600;

/// Caps concurrent pre-auth body buffers. Eight at the ceiling is about 4.7 GiB in the worst case.
pub const MAX_CONCURRENT_GIT_BODY_BUFFERS: usize = 8;

#[cfg(test)]
mod git_http_tests {
    use super::*;

    #[test]
    fn a_path_without_a_dot_git_segment_is_not_a_git_request() {
        assert!(!is_git_request_path("/repositories"));
        assert!(!is_git_request_path("/acme/backend"));
        assert!(is_git_request_path("/florian/hello.git/info/refs"));
        assert!(is_git_request_path(
            "/acme/backend/terraform-modules.git/info/refs"
        ));
    }

    #[test]
    fn find_git_segment_requires_at_least_one_segment_before_it() {
        assert_eq!(find_git_segment(&["hello.git"]), None);
        assert_eq!(find_git_segment(&["florian", "hello.git"]), Some(1));
        assert_eq!(
            find_git_segment(&["acme", "backend", "terraform-modules.git"]),
            Some(2)
        );
        assert_eq!(find_git_segment(&["repositories"]), None);
    }

    #[test]
    fn info_refs_with_receive_pack_service_query_classifies_as_write() {
        assert!(matches!(
            classify_access("info/refs", "service=git-receive-pack"),
            GitAccess::Write
        ));
        assert!(matches!(
            classify_access("info/refs", "service=git-upload-pack"),
            GitAccess::Read
        ));
        assert!(matches!(
            classify_access("git-receive-pack", ""),
            GitAccess::Write
        ));
        assert!(matches!(
            classify_access("git-upload-pack", ""),
            GitAccess::Read
        ));
    }

    #[test]
    fn infrastructure_error_maps_to_500_with_no_www_authenticate_header() {
        let response = map_auth_error(&DomainError::Infrastructure(
            "db connection lost".to_string(),
        ));

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(
            !response
                .headers()
                .contains_key(axum::http::header::WWW_AUTHENTICATE)
        );
    }

    #[test]
    fn unauthorized_error_maps_to_401_with_www_authenticate_header() {
        let response = map_auth_error(&DomainError::Unauthorized("bad credentials".to_string()));

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::WWW_AUTHENTICATE)
                .unwrap(),
            "Basic realm=\"FerrisGit\""
        );
    }

    #[test]
    fn other_domain_errors_also_map_to_401_uniformly() {
        for err in [
            DomainError::NotFound("repo".to_string()),
            DomainError::Validation("bad input".to_string()),
            DomainError::Conflict("conflict".to_string()),
        ] {
            let response = map_auth_error(&err);
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            assert!(
                response
                    .headers()
                    .contains_key(axum::http::header::WWW_AUTHENTICATE)
            );
        }
    }

    #[test]
    fn only_a_successful_write_to_the_receive_pack_rpc_endpoint_triggers_pipeline_creation() {
        assert!(should_trigger_pipeline_creation(
            &GitAccess::Write,
            "git-receive-pack",
            200
        ));
        assert!(!should_trigger_pipeline_creation(
            &GitAccess::Write,
            "git-receive-pack",
            500
        ));
        assert!(!should_trigger_pipeline_creation(
            &GitAccess::Write,
            "info/refs",
            200
        ));
        assert!(!should_trigger_pipeline_creation(
            &GitAccess::Read,
            "git-upload-pack",
            200
        ));
    }

    /// 500 MiB is the `max_push_size_mb` default.
    #[test]
    fn the_static_body_ceiling_stays_a_safety_net_just_above_the_default_configurable_limit() {
        const DEFAULT_MAX_PUSH_SIZE_MB: usize = 500;
        const {
            assert!(
                MAX_BUFFERED_GIT_BODY_MB > DEFAULT_MAX_PUSH_SIZE_MB,
                "the ceiling must sit above the configurable default, or ordinary pushes get a confusing 413"
            );
            assert!(
                MAX_BUFFERED_GIT_BODY_MB <= 2 * DEFAULT_MAX_PUSH_SIZE_MB,
                "an unauthenticated request buffers up to this much before any check runs; keep it close to a realistic maximum"
            );
        }
    }

    #[test]
    fn exceeds_configured_limit_compares_against_the_configured_megabyte_value() {
        assert!(!exceeds_configured_limit(1024 * 1024, 1));
        assert!(exceeds_configured_limit(2 * 1024 * 1024, 1));
        assert!(!exceeds_configured_limit(500 * 1024 * 1024, 500));
        assert!(exceeds_configured_limit(500 * 1024 * 1024 + 1, 500));
    }
}
