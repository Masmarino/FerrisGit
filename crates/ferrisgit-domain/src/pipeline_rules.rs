//! The `rules` of a job: which pipelines it belongs to.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::pipeline_definition::{JobDefinition, PipelineDefinition};
use crate::protected_branch::{check_ref_pattern, pattern_matches};

/// When a job runs. `branches`, `tags` and `merge_requests` name the pipelines it belongs to, any one of them being
/// enough; with none of them it belongs to all. `changes` also asks for a changed file to match. The lists are optional
/// because a key left out is no condition, while an empty list would be one that nothing meets: that one is refused.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JobRules {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branches: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_requests: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changes: Option<Vec<String>>,
}

/// What a pipeline is being created for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleEvent<'a> {
    Branch(&'a str),
    Tag(&'a str),
    MergeRequest,
}

impl JobRules {
    /// What is wrong with these rules, as the end of a sentence about the job.
    pub fn check(&self) -> Result<(), String> {
        if self == &Self::default() {
            return Err(
                "it sets none of `branches`, `tags`, `merge_requests` and `changes`".into(),
            );
        }
        if self.merge_requests == Some(false) {
            return Err(
                "`merge_requests` can only be true: to leave merge requests out, list `branches` or `tags`"
                    .into(),
            );
        }
        for (key, patterns) in [("branches", &self.branches), ("tags", &self.tags)] {
            check_list(key, patterns, check_ref_pattern)?;
        }
        check_list("changes", &self.changes, check_path_pattern)
    }

    /// `changed_paths` is `None` when what changed can't be told, as for a new branch or a tag: `changes` is then met,
    /// since running a job for nothing costs less than silently skipping one that was needed.
    pub fn apply_to(&self, event: RuleEvent, changed_paths: Option<&[String]>) -> bool {
        let names_pipelines =
            self.branches.is_some() || self.tags.is_some() || self.merge_requests.is_some();
        let any_matches = |patterns: &Option<Vec<String>>, name: &str| {
            patterns
                .iter()
                .flatten()
                .any(|pattern| pattern_matches(pattern, name))
        };
        let belongs = !names_pipelines
            || match event {
                RuleEvent::Branch(name) => any_matches(&self.branches, name),
                RuleEvent::Tag(name) => any_matches(&self.tags, name),
                RuleEvent::MergeRequest => self.merge_requests == Some(true),
            };
        let changed = match (&self.changes, changed_paths) {
            (Some(patterns), Some(paths)) => paths.iter().any(|path| {
                patterns
                    .iter()
                    .any(|pattern| path_pattern_matches(pattern, path))
            }),
            _ => true,
        };
        belongs && changed
    }
}

fn check_list(
    key: &str,
    patterns: &Option<Vec<String>>,
    check_pattern: impl Fn(&str) -> Result<(), &'static str>,
) -> Result<(), String> {
    let Some(patterns) = patterns else {
        return Ok(());
    };
    if patterns.is_empty() {
        return Err(format!("`{key}` is empty"));
    }
    for pattern in patterns {
        check_pattern(pattern).map_err(|reason| format!("`{key}` entry '{pattern}': {reason}"))?;
    }
    Ok(())
}

/// Refuses what could never match a repository-relative path, or what looks like a syntax that isn't supported.
fn check_path_pattern(pattern: &str) -> Result<(), &'static str> {
    if pattern.trim().is_empty() {
        return Err("it is empty");
    }
    if pattern.starts_with('/') || pattern.ends_with('/') || pattern.starts_with("./") {
        return Err("paths start at the root of the repository, without / or ./, and name files");
    }
    if pattern.starts_with('!') {
        return Err("exclusions are not supported");
    }
    Ok(())
}

/// Whether `path` is covered by `pattern`, compared one folder at a time: a `*` stays within a name, a `**` between two
/// slashes stands for any number of folders, none included. Linear in the length of both, whatever the pattern: it comes
/// from a file anyone with push access writes, and it runs on the server.
pub fn path_pattern_matches(pattern: &str, path: &str) -> bool {
    let names: Vec<&str> = path.split('/').collect();
    // reachable[i]: the part of the pattern read so far can consume exactly the first i names.
    let mut reachable = vec![false; names.len() + 1];
    reachable[0] = true;
    for segment in pattern.split('/') {
        if segment == "**" {
            let mut seen = false;
            for position in reachable.iter_mut() {
                seen |= *position;
                *position = seen;
            }
        } else {
            for index in (0..names.len()).rev() {
                reachable[index + 1] = reachable[index] && pattern_matches(segment, names[index]);
            }
            reachable[0] = false;
        }
    }
    reachable[names.len()]
}

/// Whether what changed has to be known to select the jobs, so that the trees are only compared when it does.
pub fn reads_changed_paths(definition: &PipelineDefinition) -> bool {
    definition.jobs.values().any(|job| {
        job.rules
            .as_ref()
            .is_some_and(|rules| rules.changes.is_some())
    })
}

/// The jobs of a pipeline created for `event`. A job whose rules leave it out is gone, and so is any job that `needs` it,
/// from one to the next: `needs` promises that a job only starts once what it lists has succeeded, and a deployment that
/// runs without the tests it asks for would break that silently. A job with no `needs` is not concerned: the barrier of
/// a stage only waits for the jobs that exist.
pub fn select_jobs<'a>(
    definition: &'a PipelineDefinition,
    event: RuleEvent,
    changed_paths: Option<&[String]>,
) -> BTreeMap<&'a str, &'a JobDefinition> {
    let mut selected: BTreeMap<&str, &JobDefinition> = definition
        .jobs
        .iter()
        .filter(|(_, job)| {
            job.rules
                .as_ref()
                .is_none_or(|rules| rules.apply_to(event, changed_paths))
        })
        .map(|(name, job)| (name.as_str(), job))
        .collect();
    loop {
        let before = selected.len();
        let kept: BTreeMap<&str, &JobDefinition> = selected
            .iter()
            .filter(|(_, job)| {
                job.needs
                    .iter()
                    .all(|dependency| selected.contains_key(dependency.as_str()))
            })
            .map(|(name, job)| (*name, *job))
            .collect();
        selected = kept;
        if selected.len() == before {
            return selected;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline_definition::{
        PipelineDefinitionError, ignored_pipeline_fields, parse_pipeline_definition,
        read_pipeline_definition, render_pipeline_definition,
    };

    fn with_rules(rules: &str) -> String {
        format!(
            "stages: [build]\njobs:\n  a:\n    stage: build\n    image: alpine\n    script: [\"true\"]\n    rules: {rules}\n"
        )
    }

    fn reason_of(rules: &str) -> String {
        match parse_pipeline_definition(&with_rules(rules)) {
            Err(PipelineDefinitionError::InvalidRules { job, reason }) => {
                assert_eq!(job, "a");
                reason
            }
            other => panic!("{rules} should have invalid rules, got {other:?}"),
        }
    }

    fn rules(yaml: &str) -> JobRules {
        parse_pipeline_definition(&with_rules(yaml)).unwrap().jobs["a"]
            .rules
            .clone()
            .unwrap()
    }

    fn paths(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|path| path.to_string()).collect()
    }

    #[test]
    fn reads_all_the_keys_of_rules() {
        let rules = rules(
            "{branches: [main, 'release/*'], tags: ['v*'], merge_requests: true, changes: ['src/**']}",
        );

        assert_eq!(
            rules,
            JobRules {
                branches: Some(vec!["main".into(), "release/*".into()]),
                tags: Some(vec!["v*".into()]),
                merge_requests: Some(true),
                changes: Some(vec!["src/**".into()]),
            }
        );
    }

    #[test]
    fn a_job_without_rules_has_none_and_nothing_about_them_is_written() {
        let definition = parse_pipeline_definition(
            "stages: [build]\njobs:\n  a:\n    stage: build\n    image: alpine\n    script: [\"true\"]\n",
        )
        .unwrap();

        assert_eq!(definition.jobs["a"].rules, None);
        assert!(
            !render_pipeline_definition(&definition)
                .unwrap()
                .contains("rules")
        );
    }

    #[test]
    fn an_unknown_key_in_rules_is_an_error_that_names_it() {
        for (rules, key) in [
            ("{branch: [main]}", "branch"),
            ("{branches: [main], when: manual}", "when"),
            ("{merge_request: true}", "merge_request"),
        ] {
            let error = parse_pipeline_definition(&with_rules(rules)).unwrap_err();

            assert!(
                matches!(&error, PipelineDefinitionError::InvalidYaml(message)
                    if message.contains("jobs.a.rules") && message.contains(&format!("unknown field `{key}`"))),
                "{error}"
            );
        }
    }

    #[test]
    fn rules_of_the_wrong_shape_are_errors() {
        for rules in [
            "[{if: main}]",
            "main",
            "{branches: main}",
            "{merge_requests: maybe}",
            "{changes: {src: true}}",
        ] {
            assert!(
                matches!(
                    parse_pipeline_definition(&with_rules(rules)),
                    Err(PipelineDefinitionError::InvalidYaml(_))
                ),
                "{rules}"
            );
        }
    }

    #[test]
    fn rules_that_could_never_mean_what_they_say_are_refused() {
        assert!(reason_of("{}").contains("sets none of"));
        assert!(reason_of("{merge_requests: false}").contains("can only be true"));
        assert_eq!(reason_of("{branches: []}"), "`branches` is empty");
        assert_eq!(reason_of("{tags: []}"), "`tags` is empty");
        assert_eq!(reason_of("{changes: []}"), "`changes` is empty");
        assert!(reason_of("{branches: ['refs/heads/main']}").contains("without refs/heads/"));
        assert!(reason_of("{tags: ['v 1']}").contains("`tags` entry 'v 1'"));
        assert!(reason_of("{branches: ['']}").contains("it is empty"));
        assert!(reason_of("{changes: ['']}").contains("it is empty"));
        assert!(reason_of("{changes: ['/src/**']}").contains("start at the root"));
        assert!(reason_of("{changes: ['./src']}").contains("start at the root"));
        assert!(reason_of("{changes: ['docs/']}").contains("name files"));
        assert!(reason_of("{changes: ['!docs/**']}").contains("exclusions"));
    }

    #[test]
    fn every_invalid_rule_is_a_problem_of_its_job() {
        let error = parse_pipeline_definition(&with_rules("{tags: []}")).unwrap_err();

        assert_eq!(
            error.to_string(),
            "job 'a' has invalid `rules`: `tags` is empty"
        );
    }

    #[test]
    fn a_null_rule_is_the_same_as_none() {
        let definition = parse_pipeline_definition(&with_rules("~")).unwrap();
        assert_eq!(definition.jobs["a"].rules, None);
        assert_eq!(rules("{branches: [main], tags: ~}").tags, None);
    }

    #[test]
    fn rules_are_written_back_and_are_not_a_field_the_parser_ignores() {
        let yaml = with_rules("{branches: [main], merge_requests: true, changes: ['src/**']}");
        let definition = parse_pipeline_definition(&yaml).unwrap();

        let rendered = render_pipeline_definition(&definition).unwrap();

        assert_eq!(
            parse_pipeline_definition(&rendered).unwrap(),
            definition,
            "{rendered}"
        );
        assert!(
            !rendered.contains("tags"),
            "a key left out stays out: {rendered}"
        );
        assert!(ignored_pipeline_fields(&yaml).is_empty());
    }

    #[test]
    fn a_file_with_wrong_rules_can_still_be_opened_but_not_one_with_an_unknown_key() {
        assert!(read_pipeline_definition(&with_rules("{tags: []}")).is_ok());
        assert!(read_pipeline_definition(&with_rules("{bogus: 1}")).is_err());
    }

    #[test]
    fn branches_select_branch_pipelines_only() {
        let rules = rules("{branches: [main, 'release/*']}");

        assert!(rules.apply_to(RuleEvent::Branch("main"), None));
        assert!(rules.apply_to(RuleEvent::Branch("release/1.2"), None));
        assert!(!rules.apply_to(RuleEvent::Branch("feature"), None));
        assert!(!rules.apply_to(RuleEvent::Branch("mainline"), None));
        assert!(!rules.apply_to(RuleEvent::Tag("main"), None));
        assert!(!rules.apply_to(RuleEvent::MergeRequest, None));
    }

    #[test]
    fn tags_select_tag_pipelines_only() {
        let rules = rules("{tags: ['v*']}");

        assert!(rules.apply_to(RuleEvent::Tag("v1.0.0"), None));
        assert!(!rules.apply_to(RuleEvent::Tag("nightly"), None));
        assert!(!rules.apply_to(RuleEvent::Branch("v1"), None));
        assert!(!rules.apply_to(RuleEvent::MergeRequest, None));
    }

    #[test]
    fn merge_requests_select_merge_request_pipelines_only() {
        let rules = rules("{merge_requests: true}");

        assert!(rules.apply_to(RuleEvent::MergeRequest, None));
        assert!(!rules.apply_to(RuleEvent::Branch("main"), None));
        assert!(!rules.apply_to(RuleEvent::Tag("v1"), None));
    }

    #[test]
    fn the_pipelines_a_job_belongs_to_add_up() {
        let rules = rules("{branches: [main], tags: ['v*'], merge_requests: true}");

        assert!(rules.apply_to(RuleEvent::Branch("main"), None));
        assert!(rules.apply_to(RuleEvent::Tag("v1"), None));
        assert!(rules.apply_to(RuleEvent::MergeRequest, None));
        assert!(!rules.apply_to(RuleEvent::Branch("feature"), None));
    }

    #[test]
    fn changes_alone_applies_to_every_pipeline_and_asks_for_a_changed_file() {
        let rules = rules("{changes: ['src/**', Cargo.lock]}");
        let code = paths(&["README.md", "src/main.rs"]);
        let docs = paths(&["README.md"]);

        for event in [
            RuleEvent::Branch("main"),
            RuleEvent::Tag("v1"),
            RuleEvent::MergeRequest,
        ] {
            assert!(rules.apply_to(event, Some(&code)));
            assert!(!rules.apply_to(event, Some(&docs)));
            assert!(
                !rules.apply_to(event, Some(&[])),
                "an empty commit changes nothing"
            );
            assert!(
                rules.apply_to(event, None),
                "not knowing what changed runs the job"
            );
        }
    }

    #[test]
    fn changes_comes_on_top_of_the_pipelines_a_job_belongs_to() {
        let rules = rules("{branches: [main], changes: ['src/**']}");
        let code = paths(&["src/main.rs"]);
        let docs = paths(&["README.md"]);

        assert!(rules.apply_to(RuleEvent::Branch("main"), Some(&code)));
        assert!(!rules.apply_to(RuleEvent::Branch("main"), Some(&docs)));
        assert!(!rules.apply_to(RuleEvent::Branch("feature"), Some(&code)));
    }

    #[test]
    fn a_path_pattern_stays_within_a_folder_unless_it_says_otherwise() {
        for (pattern, path, expected) in [
            ("Cargo.lock", "Cargo.lock", true),
            ("Cargo.lock", "crates/Cargo.lock", false),
            ("*.md", "README.md", true),
            ("*.md", "docs/guide.md", false),
            ("docs/*.md", "docs/guide.md", true),
            ("docs/*.md", "docs/ci/guide.md", false),
            ("src/*", "src/main.rs", true),
            ("src/*", "src/a/b.rs", false),
            ("src/**", "src/main.rs", true),
            ("src/**", "src/a/b.rs", true),
            ("src/**", "src", true),
            ("src/**", "srcs/a.rs", false),
            ("**/*.rs", "main.rs", true),
            ("**/*.rs", "a/b/main.rs", true),
            ("**/*.rs", "a/b/main.ts", false),
            ("a/**/z", "a/z", true),
            ("a/**/z", "a/b/c/z", true),
            ("a/**/z", "a/b/c/y", false),
            ("**", "anything/at/all", true),
            ("crates/*/src/**", "crates/api/src/lib.rs", true),
            ("crates/*/src/**", "crates/api/tests/flow.rs", false),
            ("s*c/m*.rs", "src/main.rs", true),
        ] {
            assert_eq!(
                path_pattern_matches(pattern, path),
                expected,
                "{pattern} on {path}"
            );
        }
    }

    #[test]
    fn a_pattern_full_of_double_stars_cannot_slow_the_server_down() {
        let pattern = "**/a/".repeat(300) + "b";
        let path = "a/".repeat(300) + "c";
        let started = std::time::Instant::now();

        assert!(!path_pattern_matches(&pattern, &path));

        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    fn selected(yaml: &str, event: RuleEvent, changed: Option<&[String]>) -> Vec<String> {
        let definition = parse_pipeline_definition(yaml).unwrap();
        select_jobs(&definition, event, changed)
            .keys()
            .map(|name| name.to_string())
            .collect()
    }

    const PIPELINE: &str = r#"
stages: [check, test, deploy]
jobs:
  lint:
    stage: check
    image: alpine
    script: ["true"]
  docs:
    stage: check
    image: alpine
    script: ["true"]
    rules:
      changes: ["docs/**"]
  unit:
    stage: test
    image: alpine
    script: ["true"]
    needs: [lint]
    rules:
      merge_requests: true
      branches: [main]
  ship:
    stage: deploy
    image: alpine
    script: ["true"]
    needs: [unit]
    rules:
      branches: [main]
      tags: ["v*"]
  announce:
    stage: deploy
    image: alpine
    script: ["true"]
    needs: [ship]
"#;

    #[test]
    fn a_job_without_rules_stays_in_every_pipeline() {
        let docs_changed = paths(&["docs/index.md"]);
        assert_eq!(
            selected(PIPELINE, RuleEvent::Branch("main"), Some(&docs_changed)),
            vec!["announce", "docs", "lint", "ship", "unit"]
        );
    }

    #[test]
    fn a_job_left_out_takes_the_jobs_that_need_it_with_it() {
        // `ship` belongs to tags, but the `unit` it needs doesn't: neither it nor `announce` can run.
        assert_eq!(
            selected(PIPELINE, RuleEvent::Tag("v1"), Some(&[])),
            vec!["lint"]
        );
        assert_eq!(
            selected(PIPELINE, RuleEvent::MergeRequest, Some(&[])),
            vec!["lint", "unit"]
        );
        assert_eq!(
            selected(PIPELINE, RuleEvent::Branch("feature"), Some(&[])),
            vec!["lint"]
        );
    }

    #[test]
    fn a_chain_of_needs_is_followed_whatever_the_order_of_the_names() {
        let yaml = r#"
stages: [a]
jobs:
  a:
    stage: a
    image: alpine
    script: ["true"]
    needs: [b]
  b:
    stage: a
    image: alpine
    script: ["true"]
    needs: [c]
  c:
    stage: a
    image: alpine
    script: ["true"]
    needs: [d]
  d:
    stage: a
    image: alpine
    script: ["true"]
    rules:
      tags: ["*"]
  other:
    stage: a
    image: alpine
    script: ["true"]
"#;
        assert_eq!(
            selected(yaml, RuleEvent::Branch("main"), None),
            vec!["other"]
        );
        assert_eq!(
            selected(yaml, RuleEvent::Tag("v1"), None),
            vec!["a", "b", "c", "d", "other"]
        );
    }

    #[test]
    fn no_job_at_all_can_remain() {
        let yaml = with_rules("{tags: ['v*']}");

        assert!(selected(&yaml, RuleEvent::Branch("main"), None).is_empty());
    }

    #[test]
    fn what_changed_only_has_to_be_known_when_a_job_asks() {
        let read = |yaml: &str| reads_changed_paths(&parse_pipeline_definition(yaml).unwrap());

        assert!(read(&with_rules("{changes: ['src/**']}")));
        assert!(!read(&with_rules("{branches: [main]}")));
        assert!(!read("stages: []\njobs: {}\n"));
    }
}
