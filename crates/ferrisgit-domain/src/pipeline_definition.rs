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
    #[error(
        "job '{job}' declares cache key '{key}', which is not valid: cache keys must be lowercase alphanumeric characters and hyphens only (matching `^[a-z0-9-]+$`)"
    )]
    InvalidCacheKey { job: String, key: String },
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
        // Job 'a' depends on job 'z' (later alphabetically), whose stage is not declared: the error must be
        // attributed to 'z' rather than panicking.
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
        // These names sanitize to the same PVC name (`cache::pvc_name`) but mount at different paths: two caches would
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
