mod common;

use std::net::SocketAddr;

use reqwest::{StatusCode, redirect::Policy};
use sqlx::PgPool;

/// The built app as the server finds it: the shell, and the documentation copied under `docs/` as static files.
async fn spawn_server(pool: PgPool) -> SocketAddr {
    let app = common::spawn_app(pool).await;
    let static_dir = &app.static_dir;
    std::fs::create_dir_all(static_dir.join("docs/demarrer")).unwrap();
    std::fs::write(static_dir.join("docs/index.json"), r#"{"sections":[]}"#).unwrap();
    std::fs::write(
        static_dir.join("docs/demarrer/presentation.md"),
        "# Présentation\n",
    )
    .unwrap();
    app.addr
}

/// A redirect would be followed silently by a default client: this one shows it.
fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(Policy::none())
        .build()
        .unwrap()
}

/// `/docs/...` is the documentation's static files and its SPA pages at once. The files must come back as themselves
/// (not the shell, not a git request), and the pages as the shell.
#[sqlx::test]
async fn the_documentation_files_are_served_as_files_and_its_pages_as_the_app(pool: PgPool) {
    let addr = spawn_server(pool).await;
    let client = client();

    let index = client
        .get(format!("http://{addr}/docs/index.json"))
        .send()
        .await
        .unwrap();
    assert_eq!(index.status(), StatusCode::OK);
    assert!(
        index.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("application/json")
    );
    assert_eq!(
        index.headers()["cache-control"],
        "no-cache",
        "the index has no content hash: it must be revalidated after an upgrade"
    );
    assert_eq!(index.text().await.unwrap(), r#"{"sections":[]}"#);

    let page = client
        .get(format!("http://{addr}/docs/demarrer/presentation.md"))
        .send()
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    assert!(
        page.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/markdown")
    );
    assert_eq!(page.headers()["cache-control"], "no-cache");
    assert_eq!(page.text().await.unwrap(), "# Présentation\n");

    // The SPA routes, including the two that are also folders of the build: the shell, never a redirect to `/docs/`.
    for route in [
        "/docs",
        "/docs/demarrer",
        "/docs/ci-cd/reference-yaml",
        "/docs/demarrer/presentation",
    ] {
        let res = client
            .get(format!("http://{addr}{route}"))
            .send()
            .await
            .unwrap();
        assert!(
            !res.status().is_redirection(),
            "{route} answered {}",
            res.status()
        );
        assert_eq!(res.headers()["cache-control"], "no-cache", "{route}");
        assert_eq!(res.text().await.unwrap(), common::SHELL, "{route}");
    }

    // A missing page is not served as Markdown: the app (`DocsService`) recognises the shell as "not found".
    let missing = client
        .get(format!("http://{addr}/docs/demarrer/absente.md"))
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert_eq!(missing.text().await.unwrap(), common::SHELL);

    // The site root still serves the shell as before.
    let root = client.get(format!("http://{addr}/")).send().await.unwrap();
    assert_eq!(root.status(), StatusCode::OK);
    assert_eq!(root.text().await.unwrap(), common::SHELL);
}
