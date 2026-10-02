use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PipelineDefinition {
    pub stages: Vec<String>,
    pub jobs: BTreeMap<String, JobDefinition>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct JobDefinition {
    pub stage: String,
    pub image: String,
    pub script: Vec<String>,
    #[serde(default)]
    pub variables: BTreeMap<String, String>,
    #[serde(default)]
    pub needs: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub cache: Vec<String>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
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

pub fn parse_pipeline_definition(
    yaml: &str,
) -> Result<PipelineDefinition, PipelineDefinitionError> {
    let definition: PipelineDefinition = serde_yaml_ng::from_str(yaml)
        .map_err(|e| PipelineDefinitionError::InvalidYaml(e.to_string()))?;

    for (job_name, job) in &definition.jobs {
        let Some(job_stage_index) = definition.stages.iter().position(|s| s == &job.stage) else {
            return Err(PipelineDefinitionError::UnknownStage {
                job: job_name.clone(),
                stage: job.stage.clone(),
            });
        };
        for dependency in &job.needs {
            let Some(dependency_job) = definition.jobs.get(dependency) else {
                return Err(PipelineDefinitionError::UnknownDependency {
                    job: job_name.clone(),
                    dependency: dependency.clone(),
                });
            };
            let Some(dependency_stage_index) = definition
                .stages
                .iter()
                .position(|s| s == &dependency_job.stage)
            else {
                return Err(PipelineDefinitionError::UnknownStage {
                    job: dependency.clone(),
                    stage: dependency_job.stage.clone(),
                });
            };
            if dependency_stage_index > job_stage_index {
                return Err(PipelineDefinitionError::NeedsMustPrecedeOwnStage {
                    job: job_name.clone(),
                    dependency: dependency.clone(),
                });
            }
        }
        for cache_key in &job.cache {
            if !is_valid_cache_key(cache_key) {
                return Err(PipelineDefinitionError::InvalidCacheKey {
                    job: job_name.clone(),
                    key: cache_key.clone(),
                });
            }
        }
    }

    if let Some(jobs) = find_needs_cycle(&definition.jobs) {
        return Err(PipelineDefinitionError::NeedsCycle { jobs });
    }

    Ok(definition)
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
        assert_eq!(definition.stages, vec!["prepare", "check", "report"]);
        assert_eq!(definition.jobs.len(), 5);
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
}
