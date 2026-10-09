use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct PipelineDefinition {
    pub stages: Vec<String>,
    pub jobs: BTreeMap<String, JobDefinition>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct JobDefinition {
    pub stage: String,
    pub image: String,
    pub script: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cache: Vec<String>,
}

#[derive(Debug, Clone, thiserror::Error, PartialEq)]
pub enum PipelineDefinitionError {
    #[error("invalid YAML: {0}")]
    InvalidYaml(String),
    #[error("job '{job}' references stage '{stage}' which is not declared in `stages`")]
    UnknownStage { job: String, stage: String },
    #[error("job '{job}' has a `needs` entry '{dependency}' which is not a declared job")]
    UnknownDependency { job: String, dependency: String },
    #[error(
        "job '{job}' needs '{dependency}', but that job's stage does not come before '{job}''s own stage"
    )]
    NeedsMustPrecedeOwnStage { job: String, dependency: String },
    #[error("`needs` form a cycle: {}", describe_cycle(.jobs))]
    NeedsCycle { jobs: Vec<String> },
    #[error(
        "job '{job}' declares cache key '{key}', which is not valid: cache keys must be lowercase alphanumeric characters and hyphens only (matching `^[a-z0-9-]+$`)"
    )]
    InvalidCacheKey { job: String, key: String },
}

/// Renders a cycle as `a -> b -> a`.
fn describe_cycle(jobs: &[String]) -> String {
    let mut path: Vec<&str> = jobs.iter().map(String::as_str).collect();
    path.extend(jobs.first().map(String::as_str));
    path.join(" -> ")
}

/// The first `needs` cycle found, self-references included, as the jobs on it in `needs` order. Jobs are visited by
/// name so the result is stable. Dependencies are known to exist by then.
fn find_needs_cycle(jobs: &BTreeMap<String, JobDefinition>) -> Option<Vec<String>> {
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        InProgress,
        Done,
    }

    fn visit<'a>(
        name: &'a str,
        jobs: &'a BTreeMap<String, JobDefinition>,
        marks: &mut BTreeMap<&'a str, Mark>,
        path: &mut Vec<&'a str>,
    ) -> Option<Vec<String>> {
        marks.insert(name, Mark::InProgress);
        path.push(name);
        for dependency in &jobs[name].needs {
            match marks.get(dependency.as_str()) {
                Some(Mark::InProgress) => {
                    let start = path.iter().position(|n| n == &dependency.as_str())?;
                    return Some(path[start..].iter().map(|n| n.to_string()).collect());
                }
                Some(Mark::Done) => {}
                None => {
                    if let Some(cycle) = visit(dependency, jobs, marks, path) {
                        return Some(cycle);
                    }
                }
            }
        }
        path.pop();
        marks.insert(name, Mark::Done);
        None
    }

    let mut marks = BTreeMap::new();
    for name in jobs.keys() {
        if !marks.contains_key(name.as_str())
            && let Some(cycle) = visit(name, jobs, &mut marks, &mut Vec::new())
        {
            return Some(cycle);
        }
    }
    None
}

fn is_valid_cache_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Every problem of a definition whose YAML is well formed, in the order `parse_pipeline_definition` would find them,
/// so the first one is the error the parser reports. The editor shows them all at once.
pub fn check_pipeline_definition(definition: &PipelineDefinition) -> Vec<PipelineDefinitionError> {
    let mut problems: Vec<PipelineDefinitionError> = Vec::new();
    let mut report = |problem: PipelineDefinitionError| {
        // A dependency in an undeclared stage is also reported for that job, and once is enough.
        if !problems.contains(&problem) {
            problems.push(problem);
        }
    };

    for (job_name, job) in &definition.jobs {
        let job_stage_index = definition.stages.iter().position(|s| s == &job.stage);
        if job_stage_index.is_none() {
            report(PipelineDefinitionError::UnknownStage {
                job: job_name.clone(),
                stage: job.stage.clone(),
            });
        }
        for dependency in &job.needs {
            let Some(dependency_job) = definition.jobs.get(dependency) else {
                report(PipelineDefinitionError::UnknownDependency {
                    job: job_name.clone(),
                    dependency: dependency.clone(),
                });
                continue;
            };
            let Some(dependency_stage_index) = definition
                .stages
                .iter()
                .position(|s| s == &dependency_job.stage)
            else {
                report(PipelineDefinitionError::UnknownStage {
                    job: dependency.clone(),
                    stage: dependency_job.stage.clone(),
                });
                continue;
            };
            if job_stage_index.is_some_and(|index| dependency_stage_index > index) {
                report(PipelineDefinitionError::NeedsMustPrecedeOwnStage {
                    job: job_name.clone(),
                    dependency: dependency.clone(),
                });
            }
        }
        for cache_key in &job.cache {
            if !is_valid_cache_key(cache_key) {
                report(PipelineDefinitionError::InvalidCacheKey {
                    job: job_name.clone(),
                    key: cache_key.clone(),
                });
            }
        }
    }

    // The cycle search follows `needs`, so it needs every dependency to exist.
    let all_dependencies_exist = !problems
        .iter()
        .any(|p| matches!(p, PipelineDefinitionError::UnknownDependency { .. }));
    if all_dependencies_exist && let Some(jobs) = find_needs_cycle(&definition.jobs) {
        problems.push(PipelineDefinitionError::NeedsCycle { jobs });
    }
    problems
}

/// Deeper `[`/`{` nesting than any pipeline needs. The YAML parser takes a time that grows with the square of it (about
/// twenty seconds for a 256 KiB file of nested brackets), so a file past it is refused before being parsed.
pub const MAX_FLOW_NESTING: usize = 64;

/// The deepest `[`/`{` nesting of a YAML text, ignoring quoted strings and comments. Inside a flow collection a bracket
/// is always structure (a plain scalar cannot contain one there). Outside, it only counts when it opens a value, so
/// that `echo [x]` in a command is not mistaken for nesting.
pub fn flow_nesting(yaml: &str) -> usize {
    let mut depth = 0usize;
    let mut deepest = 0usize;
    let mut quote: Option<char> = None;
    // At the start of a value: a line's first token, or after `- `, `? `, `: `, or `[`, `{`, `,` inside a collection.
    let mut value_start = true;
    // An indicator (`-`, `?`, `:`) just read: a space after it starts a value.
    let mut indicator = false;
    let mut previous = '\n';
    let mut chars = yaml.chars().peekable();
    while let Some(c) = chars.next() {
        if let Some(open) = quote {
            match c {
                // '' is an escaped quote inside a single-quoted string.
                '\'' if open == '\'' && chars.peek() == Some(&'\'') => {
                    chars.next();
                }
                '\\' if open == '"' => {
                    chars.next();
                }
                _ if c == open => quote = None,
                _ => {}
            }
            previous = c;
            continue;
        }
        match c {
            '\n' => {
                // A new line in block context starts a value again; inside a collection, the line break changes nothing.
                if depth == 0 {
                    value_start = true;
                }
                indicator = false;
            }
            ' ' | '\t' => {
                if indicator {
                    value_start = true;
                    indicator = false;
                }
            }
            '#' if previous.is_whitespace() => {
                while chars.peek().is_some_and(|&next| next != '\n') {
                    chars.next();
                }
            }
            '\'' | '"' if value_start => {
                quote = Some(c);
                value_start = false;
            }
            '[' | '{' if depth > 0 || value_start => {
                depth += 1;
                deepest = deepest.max(depth);
                value_start = true;
            }
            ']' | '}' if depth > 0 => {
                depth -= 1;
                value_start = false;
            }
            ',' if depth > 0 => value_start = true,
            ':' => indicator = true,
            '-' | '?' if value_start => indicator = true,
            _ => {
                value_start = false;
                indicator = false;
            }
        }
        previous = c;
    }
    deepest
}

/// Reads the YAML into a definition without any check, so that a file with wrong stages or dependencies can still be
/// opened and fixed in the editor.
pub fn read_pipeline_definition(yaml: &str) -> Result<PipelineDefinition, PipelineDefinitionError> {
    if flow_nesting(yaml) > MAX_FLOW_NESTING {
        return Err(PipelineDefinitionError::InvalidYaml(format!(
            "lists and maps are nested more than {MAX_FLOW_NESTING} levels deep"
        )));
    }
    serde_yaml_ng::from_str(yaml).map_err(|e| PipelineDefinitionError::InvalidYaml(e.to_string()))
}

pub fn parse_pipeline_definition(
    yaml: &str,
) -> Result<PipelineDefinition, PipelineDefinitionError> {
    let definition = read_pipeline_definition(yaml)?;
    match check_pipeline_definition(&definition).into_iter().next() {
        Some(problem) => Err(problem),
        None => Ok(definition),
    }
}

/// Things the server accepts but that are very likely mistakes. They are shown next to the problems and never block
/// anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineDefinitionWarning {
    EmptyImage { job: String },
    EmptyScript { job: String },
    DuplicateStage { stage: String },
}

pub fn pipeline_definition_warnings(
    definition: &PipelineDefinition,
) -> Vec<PipelineDefinitionWarning> {
    let mut warnings = Vec::new();
    for (index, stage) in definition.stages.iter().enumerate() {
        if definition.stages[..index].contains(stage)
            && !warnings.contains(&PipelineDefinitionWarning::DuplicateStage {
                stage: stage.clone(),
            })
        {
            warnings.push(PipelineDefinitionWarning::DuplicateStage {
                stage: stage.clone(),
            });
        }
    }
    for (name, job) in &definition.jobs {
        if job.image.trim().is_empty() {
            warnings.push(PipelineDefinitionWarning::EmptyImage { job: name.clone() });
        }
        if job.script.iter().all(|line| line.trim().is_empty()) {
            warnings.push(PipelineDefinitionWarning::EmptyScript { job: name.clone() });
        }
    }
    warnings
}

/// The definition as YAML, jobs grouped by stage in the order of `stages` and by name within a stage, with empty
/// optional fields left out. Comments, anchors and formatting of an earlier file are lost: only what the parser reads
/// is written.
pub fn render_pipeline_definition(
    definition: &PipelineDefinition,
) -> Result<String, PipelineDefinitionError> {
    let invalid = |e: serde_yaml_ng::Error| PipelineDefinitionError::InvalidYaml(e.to_string());
    let stage_rank = |stage: &str| {
        definition
            .stages
            .iter()
            .position(|s| s == stage)
            .unwrap_or(usize::MAX)
    };
    let mut names: Vec<&String> = definition.jobs.keys().collect();
    names.sort_by_key(|name| (stage_rank(&definition.jobs[*name].stage), name.as_str()));

    let mut jobs = serde_yaml_ng::Mapping::new();
    for name in names {
        jobs.insert(
            serde_yaml_ng::Value::String(name.clone()),
            serde_yaml_ng::to_value(&definition.jobs[name]).map_err(invalid)?,
        );
    }
    let mut root = serde_yaml_ng::Mapping::new();
    root.insert(
        serde_yaml_ng::Value::String("stages".to_string()),
        serde_yaml_ng::to_value(&definition.stages).map_err(invalid)?,
    );
    root.insert(
        serde_yaml_ng::Value::String("jobs".to_string()),
        serde_yaml_ng::Value::Mapping(jobs),
    );
    serde_yaml_ng::to_string(&serde_yaml_ng::Value::Mapping(root)).map_err(invalid)
}

const TOP_LEVEL_FIELDS: [&str; 2] = ["stages", "jobs"];
const JOB_FIELDS: [&str; 7] = [
    "stage",
    "image",
    "script",
    "variables",
    "needs",
    "tags",
    "cache",
];

/// The fields of a pipeline file that the parser does not read (`jobs.build.when`, `include`...), as dotted paths. An
/// editor that rewrites the file would drop them, so it has to warn first. Empty when the YAML does not parse.
pub fn ignored_pipeline_fields(yaml: &str) -> Vec<String> {
    if flow_nesting(yaml) > MAX_FLOW_NESTING {
        return Vec::new();
    }
    let Ok(serde_yaml_ng::Value::Mapping(root)) = serde_yaml_ng::from_str(yaml) else {
        return Vec::new();
    };
    let key_name = |key: &serde_yaml_ng::Value| key.as_str().unwrap_or("?").to_string();
    let mut ignored = Vec::new();
    for (key, value) in &root {
        let name = key_name(key);
        if !TOP_LEVEL_FIELDS.contains(&name.as_str()) {
            ignored.push(name);
        } else if name == "jobs"
            && let serde_yaml_ng::Value::Mapping(jobs) = value
        {
            for (job_key, job) in jobs {
                if let serde_yaml_ng::Value::Mapping(fields) = job {
                    for field in fields.keys() {
                        let field = key_name(field);
                        if !JOB_FIELDS.contains(&field.as_str()) {
                            ignored.push(format!("jobs.{}.{field}", key_name(job_key)));
                        }
                    }
                }
            }
        }
    }
    ignored
}

/// Whether the file has YAML comments, which a rewrite would lose. It errs on the side of yes: a quoted ` #` counts
/// too, because a false alarm only costs a warning while a miss costs a comment.
pub fn has_yaml_comments(yaml: &str) -> bool {
    yaml.lines()
        .any(|line| line.trim_start().starts_with('#') || line.contains(" #"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_well_formed_pipeline_definition() {
        let yaml = r#"
stages: [build, test]
jobs:
  compile:
    stage: build
    image: rust:1.82
    script:
      - cargo build --release
    tags: [docker]
  unit-tests:
    stage: test
    image: rust:1.82
    script:
      - cargo test
    needs: [compile]
    variables:
      RUST_LOG: debug
"#;
        let definition = parse_pipeline_definition(yaml).unwrap();

        assert_eq!(definition.stages, vec!["build", "test"]);
        assert_eq!(definition.jobs.len(), 2);
        let unit_tests = &definition.jobs["unit-tests"];
        assert_eq!(unit_tests.needs, vec!["compile"]);
        assert_eq!(
            unit_tests.variables.get("RUST_LOG"),
            Some(&"debug".to_string())
        );
        assert_eq!(definition.jobs["compile"].tags, vec!["docker"]);
    }

    #[test]
    fn rejects_malformed_yaml() {
        let result = parse_pipeline_definition("not: [valid, pipeline");
        assert!(matches!(
            result,
            Err(PipelineDefinitionError::InvalidYaml(_))
        ));
    }

    #[test]
    fn rejects_a_job_whose_stage_is_not_declared() {
        let yaml = r#"
stages: [build]
jobs:
  compile:
    stage: nonexistent
    image: rust:1.82
    script: ["cargo build"]
"#;
        let result = parse_pipeline_definition(yaml);
        assert_eq!(
            result,
            Err(PipelineDefinitionError::UnknownStage {
                job: "compile".to_string(),
                stage: "nonexistent".to_string()
            })
        );
    }

    #[test]
    fn rejects_a_needs_reference_to_an_undeclared_job() {
        let yaml = r#"
stages: [build]
jobs:
  compile:
    stage: build
    image: rust:1.82
    script: ["cargo build"]
    needs: [ghost]
"#;
        let result = parse_pipeline_definition(yaml);
        assert_eq!(
            result,
            Err(PipelineDefinitionError::UnknownDependency {
                job: "compile".to_string(),
                dependency: "ghost".to_string()
            })
        );
    }

    #[test]
    fn rejects_a_needs_reference_to_a_job_in_a_later_stage() {
        let yaml = r#"
stages: [build, test]
jobs:
  compile:
    stage: build
    image: rust:1.82
    script: ["cargo build"]
    needs: [unit-tests]
  unit-tests:
    stage: test
    image: rust:1.82
    script: ["cargo test"]
"#;
        let result = parse_pipeline_definition(yaml);
        assert_eq!(
            result,
            Err(PipelineDefinitionError::NeedsMustPrecedeOwnStage {
                job: "compile".to_string(),
                dependency: "unit-tests".to_string()
            })
        );
    }

    fn pipeline_of(stages: &[&str], jobs: &[(&str, &str, &[&str])]) -> String {
        let mut yaml = format!("stages: [{}]\njobs:\n", stages.join(", "));
        for (name, stage, needs) in jobs {
            yaml.push_str(&format!(
                "  {name}:\n    stage: {stage}\n    image: alpine\n    script: [\"true\"]\n    needs: [{}]\n",
                needs.join(", ")
            ));
        }
        yaml
    }

    #[test]
    fn rejects_a_job_that_needs_itself() {
        let yaml = pipeline_of(&["build"], &[("loop", "build", &["loop"])]);
        assert_eq!(
            parse_pipeline_definition(&yaml),
            Err(PipelineDefinitionError::NeedsCycle {
                jobs: vec!["loop".to_string()]
            })
        );
    }

    #[test]
    fn rejects_two_jobs_of_the_same_stage_that_need_each_other() {
        let yaml = pipeline_of(
            &["build"],
            &[("a", "build", &["b"]), ("b", "build", &["a"])],
        );
        let error = parse_pipeline_definition(&yaml).unwrap_err();
        assert_eq!(
            error,
            PipelineDefinitionError::NeedsCycle {
                jobs: vec!["a".to_string(), "b".to_string()]
            }
        );
        assert!(error.to_string().contains("a -> b -> a"));
    }

    #[test]
    fn rejects_an_indirect_cycle_and_names_only_the_jobs_on_it() {
        // `entry` leads into the cycle b -> c -> d -> b but isn't part of it.
        let yaml = pipeline_of(
            &["build"],
            &[
                ("entry", "build", &["b"]),
                ("b", "build", &["c"]),
                ("c", "build", &["d"]),
                ("d", "build", &["b"]),
            ],
        );
        assert_eq!(
            parse_pipeline_definition(&yaml),
            Err(PipelineDefinitionError::NeedsCycle {
                jobs: vec!["b".to_string(), "c".to_string(), "d".to_string()]
            })
        );
    }

    #[test]
    fn accepts_a_diamond_of_needs_which_is_not_a_cycle() {
        let yaml = pipeline_of(
            &["build", "test"],
            &[
                ("root", "build", &[]),
                ("left", "build", &["root"]),
                ("right", "build", &["root"]),
                ("join", "test", &["left", "right"]),
            ],
        );
        assert!(parse_pipeline_definition(&yaml).is_ok());
    }

    #[test]
    fn defaults_are_applied_for_omitted_optional_fields() {
        let yaml = r#"
stages: [build]
jobs:
  compile:
    stage: build
    image: rust:1.82
    script: ["cargo build"]
"#;
        let definition = parse_pipeline_definition(yaml).unwrap();
        let job = &definition.jobs["compile"];
        assert!(job.variables.is_empty());
        assert!(job.needs.is_empty());
        assert!(job.tags.is_empty());
    }

    #[test]
    fn rejects_when_dependency_job_has_an_undeclared_stage() {
        // 'a' depends on 'z', whose stage isn't declared: the error should name 'z', not panic.
        let yaml = r#"
stages: [build]
jobs:
  a:
    stage: build
    image: rust:1.82
    script: ["cargo build"]
    needs: [z]
  z:
    stage: nonexistent
    image: rust:1.82
    script: ["cargo test"]
"#;
        let result = parse_pipeline_definition(yaml);
        assert_eq!(
            result,
            Err(PipelineDefinitionError::UnknownStage {
                job: "z".to_string(),
                stage: "nonexistent".to_string()
            })
        );
    }

    #[test]
    fn a_job_can_declare_cache_keys() {
        let yaml = r#"
stages: [build]
jobs:
  compile:
    stage: build
    image: rust:1.82
    script: ["cargo build"]
    cache: ["cargo-registry", "target-dir"]
"#;
        let definition = parse_pipeline_definition(yaml).unwrap();
        assert_eq!(
            definition.jobs["compile"].cache,
            vec!["cargo-registry".to_string(), "target-dir".to_string()]
        );
    }

    #[test]
    fn cache_defaults_to_empty_when_omitted() {
        let yaml = r#"
stages: [build]
jobs:
  compile:
    stage: build
    image: rust:1.82
    script: ["cargo build"]
"#;
        let definition = parse_pipeline_definition(yaml).unwrap();
        assert!(definition.jobs["compile"].cache.is_empty());
    }

    #[test]
    fn rejects_a_cache_key_with_characters_that_would_collide_after_pvc_name_sanitization() {
        // Both names sanitize to the same PVC name (`cache::pvc_name`) but mount at different paths, so the caches would
        // silently collide.
        let yaml = r#"
stages: [build]
jobs:
  compile:
    stage: build
    image: rust:1.82
    script: ["cargo build"]
    cache: ["Cargo Registry/v2"]
"#;
        let result = parse_pipeline_definition(yaml);
        assert_eq!(
            result,
            Err(PipelineDefinitionError::InvalidCacheKey {
                job: "compile".to_string(),
                key: "Cargo Registry/v2".to_string()
            })
        );
    }

    #[test]
    fn the_repositorys_own_ci_file_is_a_valid_pipeline_definition() {
        let definition =
            parse_pipeline_definition(include_str!("../../../.ferrisgit-ci.yml")).unwrap();
        assert_eq!(definition.stages, vec!["check", "test", "build"]);
        assert_eq!(definition.jobs.len(), 10);
        assert_eq!(
            definition.jobs["rust-test"].needs,
            vec!["rust-format", "rust-clippy"]
        );
    }

    #[test]
    fn a_valid_lowercase_alphanumeric_hyphenated_cache_key_still_parses_fine() {
        let yaml = r#"
stages: [build]
jobs:
  compile:
    stage: build
    image: rust:1.82
    script: ["cargo build"]
    cache: ["cargo-registry-v2", "target-dir"]
"#;
        let definition = parse_pipeline_definition(yaml).unwrap();
        assert_eq!(
            definition.jobs["compile"].cache,
            vec!["cargo-registry-v2".to_string(), "target-dir".to_string()]
        );
    }

    fn job(stage: &str, needs: &[&str]) -> JobDefinition {
        JobDefinition {
            stage: stage.to_string(),
            image: "rust:1".to_string(),
            script: vec!["cargo build".to_string()],
            variables: BTreeMap::new(),
            needs: needs.iter().map(|n| n.to_string()).collect(),
            tags: Vec::new(),
            cache: Vec::new(),
        }
    }

    fn definition(stages: &[&str], jobs: Vec<(&str, JobDefinition)>) -> PipelineDefinition {
        PipelineDefinition {
            stages: stages.iter().map(|s| s.to_string()).collect(),
            jobs: jobs.into_iter().map(|(n, j)| (n.to_string(), j)).collect(),
        }
    }

    #[test]
    fn check_reports_every_problem_and_the_parser_keeps_reporting_the_first() {
        let broken = definition(
            &["build", "test"],
            vec![
                ("a", job("nowhere", &[])),
                ("b", job("build", &["ghost"])),
                (
                    "c",
                    JobDefinition {
                        cache: vec!["Bad Key".to_string()],
                        ..job("build", &["d"])
                    },
                ),
                ("d", job("test", &[])),
            ],
        );

        let problems = check_pipeline_definition(&broken);

        assert_eq!(
            problems,
            vec![
                PipelineDefinitionError::UnknownStage {
                    job: "a".into(),
                    stage: "nowhere".into()
                },
                PipelineDefinitionError::UnknownDependency {
                    job: "b".into(),
                    dependency: "ghost".into()
                },
                PipelineDefinitionError::NeedsMustPrecedeOwnStage {
                    job: "c".into(),
                    dependency: "d".into()
                },
                PipelineDefinitionError::InvalidCacheKey {
                    job: "c".into(),
                    key: "Bad Key".into()
                },
            ],
            "in the order the parser would meet them, and no cycle search while a dependency is missing"
        );
        let yaml = render_pipeline_definition(&broken).unwrap();
        assert_eq!(parse_pipeline_definition(&yaml), Err(problems[0].clone()));
    }

    #[test]
    fn check_reports_a_cycle_once_the_dependencies_exist() {
        let cyclic = definition(
            &["build"],
            vec![("a", job("build", &["b"])), ("b", job("build", &["a"]))],
        );

        let problems = check_pipeline_definition(&cyclic);

        assert_eq!(
            problems,
            vec![PipelineDefinitionError::NeedsCycle {
                jobs: vec!["a".into(), "b".into()]
            }]
        );
    }

    #[test]
    fn an_undeclared_stage_of_a_dependency_is_reported_once() {
        let d = definition(
            &["build"],
            vec![("a", job("build", &["b"])), ("b", job("nowhere", &[]))],
        );

        let problems = check_pipeline_definition(&d);

        assert_eq!(
            problems,
            vec![PipelineDefinitionError::UnknownStage {
                job: "b".into(),
                stage: "nowhere".into()
            }]
        );
    }

    #[test]
    fn check_accepts_a_valid_definition() {
        let d = definition(
            &["build", "test"],
            vec![("a", job("build", &[])), ("b", job("test", &["a"]))],
        );

        assert!(check_pipeline_definition(&d).is_empty());
    }

    #[test]
    fn rendering_groups_jobs_by_stage_then_name_and_leaves_empty_fields_out() {
        let d = definition(
            &["build", "test"],
            vec![
                ("zeta", job("build", &[])),
                ("alpha", job("test", &["zeta"])),
                ("beta", job("build", &[])),
            ],
        );

        let yaml = render_pipeline_definition(&d).unwrap();

        assert_eq!(
            yaml,
            "stages:\n- build\n- test\njobs:\n  beta:\n    stage: build\n    image: rust:1\n    script:\n    - cargo build\n  zeta:\n    stage: build\n    image: rust:1\n    script:\n    - cargo build\n  alpha:\n    stage: test\n    image: rust:1\n    script:\n    - cargo build\n    needs:\n    - zeta\n"
        );
        assert!(!yaml.contains("variables") && !yaml.contains("tags") && !yaml.contains("cache"));
    }

    #[test]
    fn what_is_rendered_parses_back_to_the_same_definition() {
        let mut tricky = job("build", &[]);
        tricky.script = vec![
            "echo \"a: b # not a comment\"".to_string(),
            "- starts with a dash".to_string(),
            "true".to_string(),
            "multi word | pipe > redirect".to_string(),
        ];
        tricky.variables.insert("PLAIN".into(), "yes".into());
        tricky.variables.insert("NUMBER".into(), "007".into());
        tricky.tags = vec!["docker".into()];
        tricky.cache = vec!["cargo-registry".into()];
        let d = definition(&["build"], vec![("tricky", tricky)]);

        let yaml = render_pipeline_definition(&d).unwrap();

        assert_eq!(parse_pipeline_definition(&yaml).unwrap(), d, "{yaml}");
    }

    #[test]
    fn the_repositorys_own_ci_file_survives_a_render() {
        let original =
            parse_pipeline_definition(include_str!("../../../.ferrisgit-ci.yml")).unwrap();

        let yaml = render_pipeline_definition(&original).unwrap();

        assert_eq!(parse_pipeline_definition(&yaml).unwrap(), original);
        assert_eq!(
            render_pipeline_definition(&parse_pipeline_definition(&yaml).unwrap()).unwrap(),
            yaml,
            "rendering is stable"
        );
    }

    #[test]
    fn warnings_flag_an_empty_image_an_empty_script_and_a_repeated_stage() {
        let mut blank = job("build", &[]);
        blank.image = "  ".to_string();
        blank.script = vec![String::new(), "  ".to_string()];
        let d = definition(&["build", "test", "build"], vec![("blank", blank)]);

        let warnings = pipeline_definition_warnings(&d);

        assert_eq!(
            warnings,
            vec![
                PipelineDefinitionWarning::DuplicateStage {
                    stage: "build".into()
                },
                PipelineDefinitionWarning::EmptyImage {
                    job: "blank".into()
                },
                PipelineDefinitionWarning::EmptyScript {
                    job: "blank".into()
                },
            ]
        );
        assert!(
            pipeline_definition_warnings(&definition(&["a"], vec![("ok", job("a", &[]))]))
                .is_empty()
        );
    }

    #[test]
    fn fields_the_parser_does_not_read_are_listed_by_path() {
        let yaml = r#"
include: other.yml
stages: [build]
jobs:
  compile:
    stage: build
    image: rust:1
    script: [cargo build]
    when: manual
    retry: 2
"#;

        assert_eq!(
            ignored_pipeline_fields(yaml),
            vec![
                "include".to_string(),
                "jobs.compile.when".to_string(),
                "jobs.compile.retry".to_string()
            ]
        );
        assert!(ignored_pipeline_fields(include_str!("../../../.ferrisgit-ci.yml")).is_empty());
        assert!(ignored_pipeline_fields("not: [valid").is_empty());
    }

    #[test]
    fn comments_are_noticed_when_they_start_a_line_or_follow_a_value() {
        assert!(has_yaml_comments("# build\nstages: [a]"));
        assert!(has_yaml_comments("stages: [a] # the only one"));
        assert!(has_yaml_comments("jobs:\n    # indented\n  a: {}"));
        assert!(!has_yaml_comments(
            "stages: [a]\njobs:\n  a:\n    image: rust:1#2"
        ));
    }

    #[test]
    fn measures_the_nesting_of_lists_and_maps_and_nothing_else() {
        assert_eq!(
            flow_nesting("stages: [build, test]\njobs: {a: {b: [1]}}\n"),
            3
        );
        // Brackets in quoted strings, comments and commands are not nesting.
        assert_eq!(
            flow_nesting(
                "script:\n  - echo [x] {y}\n  - 'a [[[ b'\n  - \"c {{{ \\\" d\"\n# [[[[\n"
            ),
            0
        );
        assert_eq!(flow_nesting("a: 'it''s [' \nb: [[1]]\n"), 2);
    }

    #[test]
    fn cannot_be_fooled_by_closing_brackets_hidden_in_strings() {
        let yaml = format!("stages: {}x\n", "[\"]\", ".repeat(100));
        assert_eq!(flow_nesting(&yaml), 100);
    }

    #[test]
    fn refuses_nesting_deeper_than_any_pipeline_before_parsing_it() {
        let deep = format!(
            "stages: {}x{}\njobs: {{}}\n",
            "[".repeat(100_000),
            "]".repeat(100_000)
        );
        let started = std::time::Instant::now();

        let refused = read_pipeline_definition(&deep);

        assert!(
            matches!(refused, Err(PipelineDefinitionError::InvalidYaml(message)) if message.contains("nested more than 64"))
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        assert!(ignored_pipeline_fields(&deep).is_empty());
        let fine = format!(
            "stages: [build]\njobs:\n  a:\n    stage: build\n    image: x\n    script: [{}1{}]\n",
            "[".repeat(10),
            "]".repeat(10)
        );
        assert!(
            read_pipeline_definition(&fine)
                .is_err_and(|error| !error.to_string().contains("nested"))
        );
    }
}
