//! The REST reference in `docs/api/` and the router must list the same paths: a route added without documentation, or a
//! documented route that no longer exists, fails here. The check is on paths; the methods of each path are written
//! in the docs by hand.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Everything after the last `#[cfg(test)]` is test code, which may build throwaway routers.
fn production_code(source: &str) -> &str {
    source.split("#[cfg(test)]").next().unwrap_or(source)
}

/// The path literal of every `.route("…", …)` call, whether or not the call is spread over several lines.
fn route_paths(source: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut rest = production_code(source);
    while let Some(at) = rest.find(".route(") {
        rest = &rest[at + ".route(".len()..];
        let literal = rest.trim_start();
        if let Some(after_quote) = literal.strip_prefix('"')
            && let Some(end) = after_quote.find('"')
        {
            paths.push(after_quote[..end].to_string());
        }
    }
    paths
}

fn rust_files(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push(path);
        }
    }
}

/// Routes under `routes/` live below the `/api` nest; `lib.rs` declares the few root routes (`/health`, `/robots.txt`).
fn router_paths() -> BTreeSet<String> {
    let src = repo_root().join("crates/ferrisgit-api/src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);

    let mut paths = BTreeSet::new();
    for file in files {
        let relative = file.strip_prefix(&src).unwrap();
        let prefix = if relative.starts_with("routes") {
            "/api"
        } else if relative == Path::new("lib.rs") {
            ""
        } else {
            continue;
        };
        for path in route_paths(&fs::read_to_string(&file).unwrap()) {
            paths.insert(format!("{prefix}{path}"));
        }
    }
    paths
}

/// The path of every `### `METHOD /path`` heading of the API reference.
fn documented_paths(markdown: &str) -> Vec<String> {
    markdown
        .lines()
        .filter_map(|line| line.strip_prefix("### `")?.strip_suffix('`'))
        .filter_map(|heading| heading.split_once(' '))
        .map(|(_, path)| path.to_string())
        .collect()
}

fn documented_api_paths() -> BTreeSet<String> {
    let dir = repo_root().join("docs/api");
    let mut paths = BTreeSet::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|ext| ext == "md") {
            paths.extend(documented_paths(&fs::read_to_string(path).unwrap()));
        }
    }
    paths
}

#[test]
fn the_api_reference_and_the_router_list_the_same_paths() {
    let router = router_paths();
    let documented = documented_api_paths();

    let undocumented: Vec<_> = router.difference(&documented).collect();
    let unknown: Vec<_> = documented.difference(&router).collect();

    assert!(
        undocumented.is_empty(),
        "routes missing from docs/api (add a `### `METHOD path`` heading):\n{}",
        undocumented
            .iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        unknown.is_empty(),
        "docs/api documents paths the router does not declare:\n{}",
        unknown
            .iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        router.len() > 100,
        "found only {} routes: the extraction broke",
        router.len()
    );
}

#[test]
fn the_route_extraction_handles_multi_line_calls() {
    let source = r#"
        Router::new()
            .route("/a", get(a))
            .route(
                "/b/{id}",
                get(b).post(c),
            )
        #[cfg(test)]
        mod tests { fn t() { Router::new().route("/ignored", get(x)); } }
    "#;

    assert_eq!(route_paths(source), vec!["/a", "/b/{id}"]);
}

#[test]
fn the_heading_extraction_reads_the_path_after_the_method() {
    let markdown =
        "# Titre\n\n### `GET /api/repositories/{id}`\n\ntexte\n\n### `POST /api/x`\n\n## Autre\n";

    assert_eq!(
        documented_paths(markdown),
        vec!["/api/repositories/{id}", "/api/x"]
    );
}
