//! Every pipeline example in the docs (a fenced block tagged `yaml ferrisgit-ci`) has to parse with the real parser, so
//! the docs can't drift from what FerrisGit actually runs.

use std::fs;
use std::path::{Path, PathBuf};

use ferrisgit_domain::pipeline_definition::{PipelineDefinition, parse_pipeline_definition};
use ferrisgit_domain::pipeline_rules::{JobRules, RuleEvent, select_jobs};

const BLOCK_TAG: &str = "yaml ferrisgit-ci";

fn docs_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs")
}

fn markdown_files(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            markdown_files(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "md") {
            found.push(path);
        }
    }
}

/// The contents of every block opened with "```yaml ferrisgit-ci", with the line the block starts on.
fn tagged_blocks(markdown: &str) -> Vec<(usize, String)> {
    let mut blocks = Vec::new();
    let mut current: Option<(usize, String)> = None;
    for (index, line) in markdown.lines().enumerate() {
        let trimmed = line.trim_start();
        if let Some((_, body)) = current.as_mut() {
            if trimmed.starts_with("```") {
                blocks.extend(current.take());
            } else {
                body.push_str(line);
                body.push('\n');
            }
        } else if trimmed
            .strip_prefix("```")
            .is_some_and(|info| info.trim() == BLOCK_TAG)
        {
            current = Some((index + 2, String::new()));
        }
    }
    blocks
}

#[test]
fn every_pipeline_example_in_the_docs_is_accepted_by_the_parser() {
    let mut files = Vec::new();
    markdown_files(&docs_dir(), &mut files);
    files.sort();

    let mut checked = 0;
    let mut failures = Vec::new();
    for file in &files {
        let markdown = fs::read_to_string(file).unwrap();
        for (line, yaml) in tagged_blocks(&markdown) {
            checked += 1;
            if let Err(error) = parse_pipeline_definition(&yaml) {
                let name = file.strip_prefix(docs_dir()).unwrap().display();
                failures.push(format!("{name}:{line}: {error}"));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "pipeline examples the parser refuses:\n{}",
        failures.join("\n")
    );
    assert!(
        checked > 0,
        "no `{BLOCK_TAG}` block found: the docs lost their pipeline examples, or the tag changed"
    );
}

/// The examples of the reference that a reader of its `rules` section relies on, read back with the real selection.
#[test]
fn the_reference_shows_every_key_of_rules_and_means_what_it_says_about_them() {
    let markdown = fs::read_to_string(docs_dir().join("ci-cd/reference-yaml.md")).unwrap();
    let examples: Vec<PipelineDefinition> = tagged_blocks(&markdown)
        .into_iter()
        .map(|(_, yaml)| parse_pipeline_definition(&yaml).unwrap())
        .collect();
    let rules: Vec<&JobRules> = examples
        .iter()
        .flat_map(|example| example.jobs.values())
        .filter_map(|job| job.rules.as_ref())
        .collect();
    assert!(rules.iter().any(|r| r.branches.is_some()));
    assert!(rules.iter().any(|r| r.tags.is_some()));
    assert!(rules.iter().any(|r| r.merge_requests.is_some()));
    assert!(rules.iter().any(|r| r.changes.is_some()));

    let names = |example: &PipelineDefinition, event, changed: Option<&[String]>| -> Vec<String> {
        select_jobs(example, event, changed)
            .keys()
            .map(|name| name.to_string())
            .collect()
    };
    let with_job = |job: &str| {
        examples
            .iter()
            .find(|example| example.jobs.contains_key(job))
            .unwrap()
    };

    // "`publish` ne tourne que pour un tag", "`unit-tests` tourne sur `main`, sur les branches `release/…`..."
    let overview = with_job("publish");
    assert_eq!(
        names(overview, RuleEvent::Tag("v1"), None),
        ["lint", "publish", "site"]
    );
    assert_eq!(
        names(overview, RuleEvent::Branch("release/1"), Some(&[])),
        ["lint", "unit-tests"]
    );
    assert_eq!(
        names(
            overview,
            RuleEvent::MergeRequest,
            Some(&["README.md".into()])
        ),
        ["lint", "site", "unit-tests"]
    );
    assert_eq!(
        names(overview, RuleEvent::Branch("feature"), Some(&[])),
        ["lint"]
    );

    // "un push sur `main` qui ne touche pas `src/` ne déploie rien"
    let gated = with_job("deploy");
    assert!(
        names(
            gated,
            RuleEvent::Branch("main"),
            Some(&["README.md".into()])
        )
        .is_empty()
    );
    assert_eq!(
        names(
            gated,
            RuleEvent::Branch("main"),
            Some(&["src/main.rs".into()])
        ),
        ["deploy", "unit-tests"]
    );
}

#[test]
fn the_extraction_reads_only_tagged_blocks() {
    let markdown = "\
# Titre

```yaml
stages: [seul]
```

```yaml ferrisgit-ci
stages: [build]
jobs: {}
```

```bash
echo hello
```
";
    let blocks = tagged_blocks(markdown);

    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].0, 8);
    assert_eq!(blocks[0].1, "stages: [build]\njobs: {}\n");
}
