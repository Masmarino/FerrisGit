mod common;

use common::USER_PASSWORD;
use common::http::{create_user, delete, get, get_json, login, post, post_json, put};

use serde_json::json;
use sqlx::PgPool;

/// `require_role_by_id` (JSON API) and `AuthenticateGitRequestUseCase::effective_role` (git smart-HTTP) are two copies of one policy:
/// for the same hierarchy and grants, a by-id read and a git clone must give the same allow/deny outcome.
#[sqlx::test]
async fn require_role_by_id_and_git_effective_role_agree_on_every_scenario(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;

    let client = reqwest::Client::new();

    let admin_jwt = login(&client, addr, "admin", "adminpassword123").await;

    for username in ["owner", "reader", "contributor", "stranger", "colead"] {
        create_user(&client, addr, &admin_jwt, username).await;
    }

    async fn api_token(client: &reqwest::Client, addr: std::net::SocketAddr, jwt: &str) -> String {
        let res: serde_json::Value =
            post_json(client, addr, jwt, "/tokens", &json!({ "name": "ci" })).await;
        res["token"].as_str().unwrap().to_string()
    }

    let owner_jwt = login(&client, addr, "owner", USER_PASSWORD).await;
    let reader_jwt = login(&client, addr, "reader", USER_PASSWORD).await;
    let contributor_jwt = login(&client, addr, "contributor", USER_PASSWORD).await;
    let stranger_jwt = login(&client, addr, "stranger", USER_PASSWORD).await;
    let colead_jwt = login(&client, addr, "colead", USER_PASSWORD).await;

    let owner_token = api_token(&client, addr, &owner_jwt).await;
    let reader_token = api_token(&client, addr, &reader_jwt).await;
    let contributor_token = api_token(&client, addr, &contributor_jwt).await;
    let stranger_token = api_token(&client, addr, &stranger_jwt).await;

    let root_res = post(
        &client,
        addr,
        &owner_jwt,
        "/groups",
        &json!({ "name": "roleco", "description": "" }),
    )
    .await;
    assert_eq!(root_res.status(), 200);
    let root_id = root_res.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let sub_res = post(
        &client,
        addr,
        &owner_jwt,
        &format!("/groups/{root_id}/subgroups"),
        &json!({ "name": "eng", "description": "" }),
    )
    .await;
    assert_eq!(sub_res.status(), 200);
    let sub_id = sub_res.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let add_colead = post(
        &client,
        addr,
        &owner_jwt,
        &format!("/groups/{root_id}/members"),
        &json!({ "username": "colead", "role": "maintainer" }),
    )
    .await
    .status();
    assert_eq!(add_colead, 200);

    let add_reader = post(
        &client,
        addr,
        &owner_jwt,
        &format!("/groups/{sub_id}/members"),
        &json!({ "username": "reader", "role": "reader" }),
    )
    .await
    .status();
    assert_eq!(add_reader, 200);

    let add_contributor = post(
        &client,
        addr,
        &owner_jwt,
        &format!("/groups/{sub_id}/members"),
        &json!({ "username": "contributor", "role": "contributor" }),
    )
    .await
    .status();
    assert_eq!(add_contributor, 200);

    let create_repo_res = post(
        &client,
        addr,
        &owner_jwt,
        "/repositories",
        &json!({ "name": "svc", "visibility": "private", "groupPath": "roleco/eng" }),
    )
    .await;
    assert_eq!(create_repo_res.status(), 200);

    let resolve_res: serde_json::Value =
        get_json(&client, addr, &colead_jwt, "/resolve/roleco/eng/svc").await;
    let repository_id = resolve_res["repositoryId"].as_str().unwrap().to_string();

    #[allow(clippy::too_many_arguments)]
    async fn assert_parity(
        client: &reqwest::Client,
        addr: std::net::SocketAddr,
        jwt: &str,
        username: &str,
        token: &str,
        repository_id: &str,
        git_path: &str,
        expect_allowed: bool,
        scenario: &str,
    ) {
        let by_id_status = get(
            client,
            addr,
            jwt,
            &format!("/repositories/by-id/{repository_id}"),
        )
        .await
        .status();
        let by_id_allowed = by_id_status == 200;
        assert_eq!(
            by_id_allowed, expect_allowed,
            "[{scenario}] require_role_by_id (GET /api/repositories/by-id) disagreed with the expected outcome: got status {by_id_status}"
        );

        let info_refs_status = client
            .get(format!(
                "http://{addr}/{git_path}.git/info/refs?service=git-upload-pack"
            ))
            .basic_auth(username, Some(token))
            .send()
            .await
            .unwrap()
            .status();
        let git_allowed = info_refs_status == 200;
        assert_eq!(
            git_allowed, expect_allowed,
            "[{scenario}] effective_role (git info/refs) disagreed with the expected outcome: got status {info_refs_status}"
        );

        assert_eq!(
            by_id_allowed, git_allowed,
            "[{scenario}] require_role_by_id and effective_role produced DIFFERENT outcomes for the same caller and repository — the two policies have drifted out of agreement"
        );
    }

    assert_parity(
        &client,
        addr,
        &reader_jwt,
        "reader",
        &reader_token,
        &repository_id,
        "roleco/eng/svc",
        true,
        "group Reader",
    )
    .await;

    assert_parity(
        &client,
        addr,
        &contributor_jwt,
        "contributor",
        &contributor_token,
        &repository_id,
        "roleco/eng/svc",
        true,
        "group Contributor",
    )
    .await;

    assert_parity(
        &client,
        addr,
        &stranger_jwt,
        "stranger",
        &stranger_token,
        &repository_id,
        "roleco/eng/svc",
        false,
        "stranger with no grant",
    )
    .await;

    // `owner` holds two direct memberships (root and `eng`, both auto-added on group creation). Both must go for "no group role" to hold.
    let remove_owner_from_root_status = delete(
        &client,
        addr,
        &owner_jwt,
        &format!("/groups/{root_id}/members/owner"),
    )
    .await
    .status();
    assert_eq!(remove_owner_from_root_status, 200);
    let remove_owner_from_sub_status = delete(
        &client,
        addr,
        &owner_jwt,
        &format!("/groups/{sub_id}/members/owner"),
    )
    .await
    .status();
    assert_eq!(remove_owner_from_sub_status, 200);
    assert_parity(
        &client,
        addr,
        &owner_jwt,
        "owner",
        &owner_token,
        &repository_id,
        "roleco/eng/svc",
        false,
        "creator with no group role",
    )
    .await;

    // A revoked role must be denied on the very next request (no cached allow). `colead` performs the removal since `owner` has no permission left.
    let remove_reader_status = delete(
        &client,
        addr,
        &colead_jwt,
        &format!("/groups/{sub_id}/members/reader"),
    )
    .await
    .status();
    assert_eq!(remove_reader_status, 200);
    assert_parity(
        &client,
        addr,
        &reader_jwt,
        "reader",
        &reader_token,
        &repository_id,
        "roleco/eng/svc",
        false,
        "reader after role removal",
    )
    .await;

    // Public visibility: a stranger may read a public repository through both the API and git.
    let create_public_repo_res = post(
        &client,
        addr,
        &owner_jwt,
        "/repositories",
        &json!({ "name": "open-project", "visibility": "public" }),
    )
    .await;
    assert_eq!(create_public_repo_res.status(), 200);
    let public_repo_id = create_public_repo_res
        .json::<serde_json::Value>()
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    assert_parity(
        &client,
        addr,
        &stranger_jwt,
        "stranger",
        &stranger_token,
        &public_repo_id,
        "owner/open-project",
        true,
        "public repo, total stranger",
    )
    .await;

    // The public bypass is read-only: writes stay denied on both sides.
    let write_info_refs_status = client
        .get(format!(
            "http://{addr}/owner/open-project.git/info/refs?service=git-receive-pack"
        ))
        .basic_auth("stranger", Some(&stranger_token))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(
        write_info_refs_status,
        reqwest::StatusCode::UNAUTHORIZED,
        "a stranger must not gain WRITE access to a public repo over git protocol"
    );

    let write_settings_status = put(
        &client,
        addr,
        &stranger_jwt,
        &format!("/repositories/{public_repo_id}/settings"),
        &json!({ "ciEnabled": true }),
    )
    .await
    .status();
    assert_eq!(
        write_settings_status,
        reqwest::StatusCode::NOT_FOUND,
        "a stranger must not gain WRITE access to a public repo's settings via the web API (require_role_by_id masks denial as NotFound, same as every other access check in this codebase)"
    );

    // Opening an issue is a contribution, not a read: a stranger is a Reader of a public repo and gets the same masked
    // NotFound as for its settings (`issue_permissions_flow.rs` covers every issue write route).
    let create_issue_status = post(
        &client,
        addr,
        &stranger_jwt,
        &format!("/repositories/{public_repo_id}/issues"),
        &json!({ "title": "found a typo", "description": "", "kind": "bug" }),
    )
    .await
    .status();
    assert_eq!(
        create_issue_status,
        reqwest::StatusCode::NOT_FOUND,
        "Reader-level access to a public repo must not include issue creation: that takes the Contributor role"
    );
}
