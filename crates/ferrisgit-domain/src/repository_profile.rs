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

/// Lists the files of a commit, without checking it out. Paths are repo-relative and `/`-separated; folders are listed
/// too, with a trailing `/`, the skipped ones included.
#[async_trait]
pub trait RepositoryFileListerPort: Send + Sync {
    /// Every file and folder at most `max_depth` folders down, not entering those named in `skip`, stopping after `limit`.
    async fn list_files_at_revision(
        &self,
        repository_disk_path: &str,
        revision: &str,
        max_depth: usize,
        skip: &[&str],
        limit: usize,
    ) -> Result<Vec<String>, DomainError>;
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
        /// A `.sqlx` folder: queries checked against saved metadata, so the build needs no database.
        sqlx_offline: bool,
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
        .cloned()
        .collect()
}

/// What the repository holds, from its file list and the contents of `files_to_read` (those that could be read).
pub fn detect(paths: &[String], contents: &BTreeMap<String, String>) -> RepositoryProfile {
    let present: BTreeSet<&str> = paths.iter().map(String::as_str).collect();
    let has = |dir: &str, name: &str| present.contains(join(dir, name).as_str());
    let read = |dir: &str, name: &str| contents.get(&join(dir, name)).map(String::as_str);
    let dirs_with = |name: &str| -> Vec<String> {
        let mut dirs: Vec<String> = paths
            .iter()
            .filter(|path| file_name(path) == name)
            .map(|path| parent(path).to_string())
            .collect();
        dirs.sort_by(|a, b| depth(a).cmp(&depth(b)).then_with(|| a.cmp(b)));
        dirs
    };

    let mut projects = Vec::new();

    // A crate inside a workspace is part of it, not a project of its own.
    let mut rust_roots: Vec<String> = Vec::new();
    for dir in dirs_with("Cargo.toml") {
        if rust_roots.iter().any(|root| is_inside(&dir, root)) {
            continue;
        }
        let manifest = read(&dir, "Cargo.toml").unwrap_or_default();
        let workspace = toml_has_table(manifest, "workspace");
        let toolchain = read(&dir, "rust-toolchain.toml")
            .and_then(|text| toml_string(text, "channel"))
            .or_else(|| {
                read(&dir, "rust-toolchain")
                    .map(|text| text.trim().to_string())
                    .filter(|text| !text.is_empty() && !text.contains('['))
            })
            .or_else(|| toml_string(manifest, "rust-version"))
            .filter(|version| version.chars().next().is_some_and(|c| c.is_ascii_digit()));
        let mut evidence = vec![join(&dir, "Cargo.toml")];
        evidence.extend(
            ["rust-toolchain.toml", "rust-toolchain"]
                .iter()
                .filter(|name| has(&dir, name))
                .map(|name| join(&dir, name)),
        );
        let sqlx_offline = paths
            .iter()
            .any(|path| path.starts_with(&join(&dir, ".sqlx/")));
        projects.push(DetectedProject {
            dir: dir.clone(),
            evidence,
            kind: ProjectKind::Rust {
                workspace,
                toolchain,
                sqlx_offline,
            },
        });
        rust_roots.push(dir);
    }

    // A package of a workspace (`packages/*`) is built by the workspace's root.
    let mut node_roots: Vec<String> = Vec::new();
    for dir in dirs_with("package.json") {
        if node_roots.iter().any(|root| is_inside(&dir, root)) {
            continue;
        }
        let Some(manifest) = read(&dir, "package.json")
            .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
        else {
            continue;
        };
        let scripts: BTreeMap<String, String> = manifest
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
            .unwrap_or_default();
        let dependencies: BTreeSet<String> = ["dependencies", "devDependencies"]
            .iter()
            .filter_map(|key| manifest.get(*key).and_then(|deps| deps.as_object()))
            .flat_map(|deps| deps.keys().cloned())
            .collect();
        let depends = |name: &str| dependencies.contains(name);
        let declared_manager = manifest
            .get("packageManager")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let package_manager =
            if has(&dir, "pnpm-lock.yaml") || declared_manager.starts_with("pnpm@") {
                PackageManager::Pnpm
            } else if has(&dir, "bun.lockb")
                || has(&dir, "bun.lock")
                || declared_manager.starts_with("bun@")
            {
                PackageManager::Bun
            } else if has(&dir, "yarn.lock") || declared_manager.starts_with("yarn@") {
                if declared_manager.starts_with("yarn@1")
                    || (declared_manager.is_empty() && !has(&dir, ".yarnrc.yml"))
                {
                    PackageManager::YarnClassic
                } else {
                    PackageManager::Yarn
                }
            } else {
                PackageManager::Npm
            };
        let node_version = read(&dir, ".nvmrc")
            .or_else(|| read(&dir, ".node-version"))
            .and_then(major_version)
            .or_else(|| {
                manifest
                    .pointer("/engines/node")
                    .and_then(|value| value.as_str())
                    .and_then(major_version)
            });
        let framework = if has(&dir, "angular.json") || depends("@angular/core") {
            Some(NodeFramework::Angular)
        } else if depends("next") {
            Some(NodeFramework::Next)
        } else if depends("svelte") {
            Some(NodeFramework::Svelte)
        } else if depends("vue") {
            Some(NodeFramework::Vue)
        } else if depends("react") {
            Some(NodeFramework::React)
        } else {
            None
        };
        let test_runner = if depends("vitest") {
            Some(TestRunner::Vitest)
        } else if depends("jest") {
            Some(TestRunner::Jest)
        } else if depends("karma") {
            Some(TestRunner::Karma)
        } else if depends("@playwright/test") {
            Some(TestRunner::Playwright)
        } else {
            None
        };
        let mut evidence = vec![join(&dir, "package.json")];
        evidence.extend(
            [
                "package-lock.json",
                "pnpm-lock.yaml",
                "yarn.lock",
                "bun.lockb",
                "bun.lock",
                ".nvmrc",
                ".node-version",
                "angular.json",
            ]
            .iter()
            .filter(|name| has(&dir, name))
            .map(|name| join(&dir, name)),
        );
        projects.push(DetectedProject {
            dir: dir.clone(),
            evidence,
            kind: ProjectKind::Node {
                package_manager,
                node_version,
                scripts,
                framework,
                test_runner,
            },
        });
        if manifest.get("workspaces").is_some() || has(&dir, "pnpm-workspace.yaml") {
            node_roots.push(dir);
        }
    }

    for dir in dirs_with("go.mod") {
        let go_version = read(&dir, "go.mod").and_then(|text| {
            text.lines().find_map(|line| {
                line.trim()
                    .strip_prefix("go ")
                    .map(str::trim)
                    .and_then(major_minor)
            })
        });
        projects.push(DetectedProject {
            dir: dir.clone(),
            evidence: vec![join(&dir, "go.mod")],
            kind: ProjectKind::Go { go_version },
        });
    }

    let mut python_dirs: Vec<String> = dirs_with("pyproject.toml");
    for dir in dirs_with("requirements.txt") {
        if !python_dirs.contains(&dir) {
            python_dirs.push(dir);
        }
    }
    for dir in python_dirs {
        let pyproject = read(&dir, "pyproject.toml").unwrap_or_default();
        let requirements_present = has(&dir, "requirements.txt");
        let tool = if has(&dir, "uv.lock") {
            PythonTool::Uv
        } else if has(&dir, "poetry.lock") || toml_has_table(pyproject, "tool.poetry") {
            PythonTool::Poetry
        } else {
            PythonTool::Pip
        };
        let python_version = read(&dir, ".python-version")
            .and_then(major_minor_in)
            .or_else(|| {
                toml_string(pyproject, "requires-python").and_then(|range| major_minor_in(&range))
            });
        let mentions = |word: &str| pyproject.contains(word);
        let pytest = mentions("pytest")
            || paths.iter().any(|path| {
                path.starts_with(&join(&dir, "tests/")) && file_name(path).starts_with("test_")
            });
        let ruff = mentions("ruff") || has(&dir, "ruff.toml") || has(&dir, ".ruff.toml");
        let mut evidence: Vec<String> = [
            "pyproject.toml",
            "requirements.txt",
            "uv.lock",
            "poetry.lock",
        ]
        .iter()
        .filter(|name| has(&dir, name))
        .map(|name| join(&dir, name))
        .collect();
        if evidence.is_empty() && requirements_present {
            evidence.push(join(&dir, "requirements.txt"));
        }
        projects.push(DetectedProject {
            dir,
            evidence,
            kind: ProjectKind::Python {
                tool,
                python_version,
                pytest,
                ruff,
            },
        });
    }

    RepositoryProfile {
        projects,
        dockerfiles: dirs_with("Dockerfile"),
        helm_charts: dirs_with("Chart.yaml"),
    }
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
                "[workspace]\nmembers = [\"crates/*\"]\n\n[workspace.package]\nrust-version = \"1.98\"\n",
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
                    sqlx_offline: true
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
    fn takes_the_rust_version_when_no_toolchain_is_pinned_and_ignores_a_named_channel() {
        let profile = detect(
            &files(&["Cargo.toml"]),
            &contents(&[(
                "Cargo.toml",
                "[package]\nname = \"x\"\nrust-version = \"1.80\"\n",
            )]),
        );
        assert!(
            matches!(&profile.projects[0].kind, ProjectKind::Rust { workspace: false, toolchain: Some(version), sqlx_offline: false } if version == "1.80")
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
}
