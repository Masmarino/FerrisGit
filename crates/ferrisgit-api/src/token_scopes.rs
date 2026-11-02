//! What an API token needs for each route. Routes are named by their template (`MatchedPath`), so a repository called
//! `issues` can't change which rule applies. Anything this table doesn't name is refused to tokens: a new route stays
//! session-only until it is given a scope here.

use axum::http::Method;
use ferrisgit_domain::access_token::{ScopeArea, ScopeLevel, TokenScope};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenAccess {
    /// Says who the token belongs to or what the instance allows, nothing more.
    AnyToken,
    /// Every one of these. More than one for a route whose answer spans several areas.
    Requires(Vec<TokenScope>),
    SessionOnly,
}

fn scoped(area: ScopeArea, level: ScopeLevel) -> TokenAccess {
    TokenAccess::Requires(vec![TokenScope::new(area, level)])
}

fn level_of(method: &Method) -> ScopeLevel {
    if method == Method::GET || method == Method::HEAD {
        ScopeLevel::Read
    } else {
        ScopeLevel::Write
    }
}

/// `template` is the matched route with its `/api` prefix.
pub fn required_access(method: &Method, template: &str) -> TokenAccess {
    let Some(path) = template.strip_prefix("/api/") else {
        return TokenAccess::SessionOnly;
    };
    let segments: Vec<&str> = path.split('/').collect();
    let level = level_of(method);

    match segments.as_slice() {
        ["auth", "me"] if method == Method::GET => TokenAccess::AnyToken,
        ["settings", "public"] => TokenAccess::AnyToken,

        ["admin", ..] => scoped(ScopeArea::Admin, level),

        // Their answers mix repositories, issues and merge requests, so a token must be able to read all three.
        ["search"] | ["dashboard"] | ["notifications", ..] => TokenAccess::Requires(vec![
            TokenScope::new(ScopeArea::Repositories, level),
            TokenScope::new(ScopeArea::Issues, level),
            TokenScope::new(ScopeArea::MergeRequests, level),
        ]),

        ["resolve", ..] => scoped(ScopeArea::Repositories, level),

        ["merge-requests", ..] => scoped(ScopeArea::MergeRequests, level),
        ["pipelines", ..] => scoped(ScopeArea::Pipelines, level),
        // These two only compute from their body: they are POSTs that change nothing.
        ["pipeline-definitions", ..] => scoped(ScopeArea::Pipelines, ScopeLevel::Read),
        ["labels", ..] | ["milestones", ..] => scoped(ScopeArea::Issues, level),

        ["groups", _, "labels" | "milestones", ..] => scoped(ScopeArea::Issues, level),
        ["groups", ..] => scoped(ScopeArea::Repositories, level),

        ["repositories", id, area, ..] if id.starts_with('{') => repository_area(area, level),
        ["repositories", ..] => scoped(ScopeArea::Repositories, level),

        _ => TokenAccess::SessionOnly,
    }
}

fn repository_area(area: &str, level: ScopeLevel) -> TokenAccess {
    match area {
        "issues" | "labels" | "milestones" => scoped(ScopeArea::Issues, level),
        "merge-requests" => scoped(ScopeArea::MergeRequests, level),
        "pipelines" | "pipeline-definition" | "ci-variables" => scoped(ScopeArea::Pipelines, level),
        "releases" | "tags" => scoped(ScopeArea::Releases, level),
        // A webhook's URL often carries a secret, and its deliveries hold whole payloads.
        "webhooks" => scoped(ScopeArea::Repositories, ScopeLevel::Write),
        _ => scoped(ScopeArea::Repositories, level),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(value: &str) -> TokenAccess {
        TokenAccess::Requires(vec![value.parse().unwrap()])
    }

    fn get(template: &str) -> TokenAccess {
        required_access(&Method::GET, template)
    }

    fn post(template: &str) -> TokenAccess {
        required_access(&Method::POST, template)
    }

    #[test]
    fn a_route_nobody_listed_is_refused_to_tokens() {
        assert_eq!(get("/api/something-new"), TokenAccess::SessionOnly);
        assert_eq!(post("/api/something-new/{id}"), TokenAccess::SessionOnly);
        assert_eq!(get("/not-the-api"), TokenAccess::SessionOnly);
    }

    #[test]
    fn what_manages_credentials_or_the_account_stays_with_sessions() {
        for template in [
            "/api/tokens",
            "/api/tokens/{id}",
            "/api/access-tokens",
            "/api/access-tokens/{id}",
            "/api/auth/device/{user_code}",
            "/api/auth/device/{user_code}/approve",
            "/api/auth/device/{user_code}/passkey",
            "/api/auth/device/{user_code}/deny",
            "/api/auth/me/password",
            "/api/auth/me/language",
            "/api/auth/logout-all",
            "/api/me/mfa",
            "/api/me/mfa/totp/disable",
            "/api/me/mfa/passkeys/{id}/delete",
        ] {
            assert_eq!(get(template), TokenAccess::SessionOnly, "GET {template}");
            assert_eq!(post(template), TokenAccess::SessionOnly, "POST {template}");
        }
    }

    #[test]
    fn identity_is_readable_by_any_token_but_not_writable() {
        assert_eq!(get("/api/auth/me"), TokenAccess::AnyToken);
        assert_eq!(
            required_access(&Method::PATCH, "/api/auth/me"),
            TokenAccess::SessionOnly
        );
    }

    #[test]
    fn reading_and_writing_ask_for_different_levels() {
        let template = "/api/repositories/{repository_id}/issues";
        assert_eq!(get(template), scope("issues:read"));
        assert_eq!(
            required_access(&Method::HEAD, template),
            scope("issues:read")
        );
        assert_eq!(post(template), scope("issues:write"));
        assert_eq!(
            required_access(&Method::DELETE, template),
            scope("issues:write")
        );
    }

    #[test]
    fn each_area_of_a_repository_has_its_own_scope() {
        for (template, expected) in [
            ("/api/repositories", "repositories:read"),
            ("/api/repositories/{owner}/{name}", "repositories:read"),
            (
                "/api/repositories/{owner}/{name}/commits",
                "repositories:read",
            ),
            ("/api/repositories/by-id/{id}", "repositories:read"),
            (
                "/api/repositories/by-id/{id}/tree/{ref}",
                "repositories:read",
            ),
            (
                "/api/repositories/{repository_id}/branches",
                "repositories:read",
            ),
            (
                "/api/repositories/{repository_id}/collaborators",
                "repositories:read",
            ),
            ("/api/resolve/{*path}", "repositories:read"),
            ("/api/groups/{id}/children", "repositories:read"),
            (
                "/api/repositories/{repository_id}/issues/{number}",
                "issues:read",
            ),
            ("/api/repositories/{repository_id}/labels", "issues:read"),
            (
                "/api/repositories/{repository_id}/milestones",
                "issues:read",
            ),
            ("/api/groups/{group_id}/labels", "issues:read"),
            ("/api/labels/{id}", "issues:read"),
            (
                "/api/repositories/{repository_id}/merge-requests",
                "merge-requests:read",
            ),
            ("/api/merge-requests/{id}/diff", "merge-requests:read"),
            (
                "/api/repositories/{repository_id}/pipelines",
                "pipelines:read",
            ),
            (
                "/api/repositories/{repository_id}/ci-variables",
                "pipelines:read",
            ),
            ("/api/pipelines/{id}", "pipelines:read"),
            (
                "/api/repositories/{repository_id}/releases/{tag_name}",
                "releases:read",
            ),
            ("/api/repositories/{repository_id}/tags", "releases:read"),
            ("/api/admin/users", "admin:read"),
            ("/api/admin/settings/smtp", "admin:read"),
        ] {
            assert_eq!(get(template), scope(expected), "{template}");
        }
    }

    #[test]
    fn a_webhook_needs_write_even_to_read_it() {
        assert_eq!(
            get("/api/repositories/{repository_id}/webhooks"),
            scope("repositories:write")
        );
    }

    #[test]
    fn a_post_that_only_computes_is_a_read() {
        assert_eq!(
            post("/api/pipeline-definitions/parse"),
            scope("pipelines:read")
        );
    }

    #[test]
    fn what_spans_several_areas_needs_all_of_them() {
        let TokenAccess::Requires(scopes) = get("/api/search") else {
            panic!("search must be scoped");
        };
        assert_eq!(scopes.len(), 3);
    }
}
