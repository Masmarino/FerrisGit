mod common;

use common::USER_PASSWORD;
use common::http::{create_user, get, get_json, login, post};

use serde_json::json;
use sqlx::PgPool;

#[sqlx::test]
async fn nested_group_repository_access_inherits_across_two_levels_and_enforces_namespace_uniqueness(
    pool: PgPool,
) {
    let addr = common::spawn_app(pool).await.addr;

    let client = reqwest::Client::new();

    let admin_jwt = login(&client, addr, "admin", "adminpassword123").await;

    for username in ["owner", "alice", "stranger"] {
        create_user(&client, addr, &admin_jwt, username).await;
    }

    let owner_jwt = login(&client, addr, "owner", USER_PASSWORD).await;
    let alice_jwt = login(&client, addr, "alice", USER_PASSWORD).await;
    let stranger_jwt = login(&client, addr, "stranger", USER_PASSWORD).await;

    let acme_res = post(
        &client,
        addr,
        &owner_jwt,
        "/groups",
        &json!({ "name": "acme", "description": "Acme Corp" }),
    )
    .await;
    assert_eq!(acme_res.status(), 200);
    let acme_body: serde_json::Value = acme_res.json().await.unwrap();
    let acme_id = acme_body["id"].as_str().unwrap();

    let acme_members: serde_json::Value = get_json(
        &client,
        addr,
        &owner_jwt,
        &format!("/groups/{acme_id}/members"),
    )
    .await;
    let acme_members = acme_members.as_array().unwrap();
    let owner_member = acme_members
        .iter()
        .find(|m| m["username"] == "owner")
        .expect("owner must be listed as a member of the group they created");
    assert_eq!(
        owner_member["role"], "maintainer",
        "the creator of a root group must be auto-added as maintainer"
    );

    let backend_res = post(
        &client,
        addr,
        &owner_jwt,
        &format!("/groups/{acme_id}/subgroups"),
        &json!({ "name": "backend", "description": "Backend team" }),
    )
    .await;
    assert_eq!(backend_res.status(), 200);
    let backend_body: serde_json::Value = backend_res.json().await.unwrap();
    let backend_id = backend_body["id"].as_str().unwrap();
    assert_eq!(backend_body["parentGroupId"], acme_id);

    let add_alice_status = post(
        &client,
        addr,
        &owner_jwt,
        &format!("/groups/{backend_id}/members"),
        &json!({ "username": "alice", "role": "contributor" }),
    )
    .await
    .status();
    assert_eq!(add_alice_status, 200);

    let create_repo_res = post(&client, addr, &owner_jwt, "/repositories", &json!({ "name": "terraform-modules", "visibility": "private", "groupPath": "acme/backend" })).await;
    assert_eq!(create_repo_res.status(), 200);

    let resolve_res: serde_json::Value = get_json(
        &client,
        addr,
        &alice_jwt,
        "/resolve/acme/backend/terraform-modules",
    )
    .await;
    assert_eq!(resolve_res["type"], "groupRepository");
    let repository_id = resolve_res["repositoryId"].as_str().unwrap().to_string();

    let alice_read_res = get(
        &client,
        addr,
        &alice_jwt,
        &format!("/repositories/by-id/{repository_id}"),
    )
    .await;
    assert_eq!(
        alice_read_res.status(),
        200,
        "alice's contributor grant on the ancestor `backend` group must inherit down to read access on the repository"
    );
    let alice_read_body: serde_json::Value = alice_read_res.json().await.unwrap();
    assert_eq!(
        alice_read_body["role"], "contributor",
        "alice's resolved role on the repository must reflect her group-inherited role, not a default"
    );
    assert_eq!(
        alice_read_body["path"],
        json!(["acme", "backend", "terraform-modules"]),
        "a group repository's resolvable path is [...ancestor_group_names, name] — {{owner}}/{{name}} can never resolve it"
    );

    let stranger_read_status = get(
        &client,
        addr,
        &stranger_jwt,
        &format!("/repositories/by-id/{repository_id}"),
    )
    .await
    .status();
    assert_eq!(
        stranger_read_status, 404,
        "a caller with no access anywhere in the group chain must see 404, not 401 — no existence leak"
    );

    // The group listing used to hardcode the role "member", it has to report the caller's resolved one.

    let alice_list_res = get(&client, addr, &alice_jwt, "/repositories").await;
    assert_eq!(alice_list_res.status(), 200);
    let alice_list_body: serde_json::Value = alice_list_res.json().await.unwrap();
    let alice_list = alice_list_body.as_array().unwrap();
    let alice_listed_repo = alice_list.iter().find(|r| r["id"] == repository_id).expect("alice's GET /repositories must include a repository reachable only through group membership");
    assert_eq!(
        alice_listed_repo["role"], "contributor",
        "a group-inherited repository listing must report the caller's real resolved role, not a hardcoded placeholder"
    );
    assert_eq!(
        alice_list
            .iter()
            .filter(|r| r["id"] == repository_id)
            .count(),
        1,
        "the group-inherited repository must appear exactly once, not duplicated"
    );
    assert_eq!(
        alice_listed_repo["path"],
        json!(["acme", "backend", "terraform-modules"]),
        "GET /repositories must also carry the resolvable path for a group repository, not just {{owner}}/{{name}}"
    );

    let backend_repos_res = get(
        &client,
        addr,
        &owner_jwt,
        &format!("/groups/{backend_id}/repositories"),
    )
    .await;
    assert_eq!(backend_repos_res.status(), 200);
    let backend_repos_body: serde_json::Value = backend_repos_res.json().await.unwrap();
    let backend_repos = backend_repos_body.as_array().unwrap();
    let backend_listed_repo = backend_repos
        .iter()
        .find(|r| r["id"] == repository_id)
        .expect("the group repository must be listed under its own group");
    assert_eq!(
        backend_listed_repo["path"],
        json!(["acme", "backend", "terraform-modules"])
    );

    let alice_member_groups_res = get(&client, addr, &alice_jwt, "/groups/member").await;
    assert_eq!(alice_member_groups_res.status(), 200);
    let alice_member_groups_body: serde_json::Value = alice_member_groups_res.json().await.unwrap();
    let alice_member_groups = alice_member_groups_body.as_array().unwrap();
    assert_eq!(
        alice_member_groups.len(),
        1,
        "alice should see exactly the one group she's a direct member of, got {alice_member_groups:?}"
    );
    assert_eq!(alice_member_groups[0]["id"], backend_id);
    assert_eq!(alice_member_groups[0]["path"], "acme/backend");

    let register_acme_status = post(
        &client,
        addr,
        &admin_jwt,
        "/admin/users",
        &json!({ "username": "acme", "email": "acme@example.com", "password": "password12345" }),
    )
    .await
    .status();
    assert_eq!(
        register_acme_status, 409,
        "a username colliding with an existing root group must be rejected as a conflict"
    );

    let create_group_named_alice_status = post(
        &client,
        addr,
        &owner_jwt,
        "/groups",
        &json!({ "name": "alice", "description": "" }),
    )
    .await
    .status();
    assert_eq!(
        create_group_named_alice_status, 409,
        "a root group name colliding with an existing username must be rejected as a conflict"
    );
}

/// Group listings used to hardcode the role "member" instead of the caller's resolved role.
#[sqlx::test]
async fn a_group_maintainer_and_a_group_reader_see_their_real_resolved_role_on_the_same_group_repository_listing(
    pool: PgPool,
) {
    let addr = common::spawn_app(pool).await.addr;

    let client = reqwest::Client::new();

    let admin_jwt = login(&client, addr, "admin", "adminpassword123").await;

    for username in ["owner", "maintainer", "reader"] {
        create_user(&client, addr, &admin_jwt, username).await;
    }

    let owner_jwt = login(&client, addr, "owner", USER_PASSWORD).await;
    let maintainer_jwt = login(&client, addr, "maintainer", USER_PASSWORD).await;
    let reader_jwt = login(&client, addr, "reader", USER_PASSWORD).await;

    let acme_res = post(
        &client,
        addr,
        &owner_jwt,
        "/groups",
        &json!({ "name": "acme", "description": "" }),
    )
    .await;
    assert_eq!(acme_res.status(), 200);
    let acme_body: serde_json::Value = acme_res.json().await.unwrap();
    let acme_id = acme_body["id"].as_str().unwrap();

    let create_repo_res = post(
        &client,
        addr,
        &owner_jwt,
        "/repositories",
        &json!({ "name": "widget", "visibility": "private", "groupPath": "acme" }),
    )
    .await;
    assert_eq!(create_repo_res.status(), 200);
    let repository_id = create_repo_res.json::<serde_json::Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let add_maintainer_status = post(
        &client,
        addr,
        &owner_jwt,
        &format!("/groups/{acme_id}/members"),
        &json!({ "username": "maintainer", "role": "maintainer" }),
    )
    .await
    .status();
    assert_eq!(add_maintainer_status, 200);

    let add_reader_status = post(
        &client,
        addr,
        &owner_jwt,
        &format!("/groups/{acme_id}/members"),
        &json!({ "username": "reader", "role": "reader" }),
    )
    .await
    .status();
    assert_eq!(add_reader_status, 200);

    let maintainer_list: serde_json::Value =
        get_json(&client, addr, &maintainer_jwt, "/repositories").await;
    let maintainer_entry = maintainer_list
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == repository_id)
        .expect(
            "the group Maintainer must see the repository in their own GET /repositories listing",
        );
    assert_eq!(
        maintainer_entry["role"], "maintainer",
        "a group Maintainer must see their real resolved role, not a hardcoded placeholder"
    );

    let reader_list: serde_json::Value =
        get_json(&client, addr, &reader_jwt, "/repositories").await;
    let reader_entry = reader_list
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == repository_id)
        .expect("the group Reader must see the repository in their own GET /repositories listing");
    assert_eq!(
        reader_entry["role"], "reader",
        "a group Reader must see their real resolved role, not the Maintainer's role and not a hardcoded placeholder"
    );

    let maintainer_group_list: serde_json::Value = get_json(
        &client,
        addr,
        &maintainer_jwt,
        &format!("/groups/{acme_id}/repositories"),
    )
    .await;
    let maintainer_group_entry = maintainer_group_list
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == repository_id)
        .expect("the repository must be listed under its own group");
    assert_eq!(
        maintainer_group_entry["role"], "maintainer",
        "GET /groups/{{id}}/repositories must also report the caller's real resolved role"
    );

    let reader_group_list: serde_json::Value = get_json(
        &client,
        addr,
        &reader_jwt,
        &format!("/groups/{acme_id}/repositories"),
    )
    .await;
    let reader_group_entry = reader_group_list
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == repository_id)
        .expect("the repository must be listed under its own group");
    assert_eq!(
        reader_group_entry["role"], "reader",
        "GET /groups/{{id}}/repositories must also report the caller's real resolved role, not the Maintainer's"
    );
}
