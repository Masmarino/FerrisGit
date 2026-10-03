// An instance administrator manages accounts and settings, not other people's private code. The product page and the
// documentation say so, so the rule is pinned here, through every way the code can be read.

mod common;

use common::USER_PASSWORD;
use common::http::{create_user, get, get_json, login, mint_api_token, post};

use serde_json::{Value, json};
use sqlx::PgPool;

#[sqlx::test]
async fn an_instance_admin_cannot_read_another_users_private_repository(pool: PgPool) {
    let addr = common::spawn_app(pool).await.addr;
    let client = reqwest::Client::new();
    let admin_jwt = login(&client, addr, "admin", "adminpassword123").await;
    create_user(&client, addr, &admin_jwt, "owner").await;
    let owner_jwt = login(&client, addr, "owner", USER_PASSWORD).await;
    let created = post(
        &client,
        addr,
        &owner_jwt,
        "/repositories",
        &json!({ "name": "secret", "visibility": "private" }),
    )
    .await;
    assert_eq!(created.status(), 200);

    // The owner reads it, so the checks below are about the admin and not about a broken route.
    assert_eq!(
        get(&client, addr, &owner_jwt, "/repositories/owner/secret")
            .await
            .status(),
        200
    );

    let by_name = get(&client, addr, &admin_jwt, "/repositories/owner/secret").await;
    assert_eq!(
        by_name.status(),
        404,
        "the API answers as if the repository did not exist"
    );
    let listed: Value = get_json(&client, addr, &admin_jwt, "/repositories").await;
    assert!(
        !listed.to_string().contains("secret"),
        "the admin's repository list does not include it: {listed}"
    );

    // Git over HTTP, with the admin's own token.
    let admin_token = mint_api_token(&client, addr, &admin_jwt, "git").await;
    let owner_token = mint_api_token(&client, addr, &owner_jwt, "git").await;
    let discover = |user: &'static str, token: String| {
        let client = client.clone();
        async move {
            client
                .get(format!(
                    "http://{addr}/owner/secret.git/info/refs?service=git-upload-pack"
                ))
                .basic_auth(user, Some(token))
                .send()
                .await
                .unwrap()
                .status()
                .as_u16()
        }
    };
    assert_eq!(
        discover("owner", owner_token).await,
        200,
        "the owner can clone"
    );
    let denied = discover("admin", admin_token).await;
    assert!(
        matches!(denied, 401 | 403 | 404),
        "the admin cannot clone another user's private repository, got {denied}"
    );
}
