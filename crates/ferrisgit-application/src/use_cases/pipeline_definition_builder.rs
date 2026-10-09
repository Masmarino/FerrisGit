use ferrisgit_domain::error::DomainError;
use ferrisgit_domain::pipeline_definition::{
    PipelineDefinition, PipelineDefinitionError, PipelineDefinitionWarning,
    check_pipeline_definition, has_yaml_comments, ignored_pipeline_fields,
    pipeline_definition_warnings, read_pipeline_definition, render_pipeline_definition,
};

/// What a `.ferrisgit-ci.yml` holds, for the editor. The definition comes back as soon as the YAML is well formed, even
/// if its stages or dependencies are wrong, so that a broken file can still be opened and fixed.
#[derive(Debug, Clone, PartialEq)]
pub struct PipelineDefinitionReport {
    pub definition: Option<PipelineDefinition>,
    pub problems: Vec<PipelineDefinitionError>,
    pub warnings: Vec<PipelineDefinitionWarning>,
    /// Fields the parser does not read, so the editor cannot keep them: rewriting the file would drop them.
    pub ignored_fields: Vec<String>,
    /// Comments in the file, which a rewrite would lose.
    pub has_comments: bool,
}

#[derive(Default)]
pub struct ReadPipelineDefinitionUseCase;

impl ReadPipelineDefinitionUseCase {
    pub fn execute(&self, yaml: &str) -> PipelineDefinitionReport {
        match read_pipeline_definition(yaml) {
            Err(problem) => PipelineDefinitionReport {
                definition: None,
                problems: vec![problem],
                warnings: Vec::new(),
                ignored_fields: Vec::new(),
                has_comments: has_yaml_comments(yaml),
            },
            Ok(definition) => PipelineDefinitionReport {
                problems: check_pipeline_definition(&definition),
                warnings: pipeline_definition_warnings(&definition),
                ignored_fields: ignored_pipeline_fields(yaml),
                has_comments: has_yaml_comments(yaml),
                definition: Some(definition),
            },
        }
    }
}

/// A definition drawn in the editor, as the file it would produce, with its problems.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedPipelineDefinition {
    pub yaml: String,
    pub problems: Vec<PipelineDefinitionError>,
    pub warnings: Vec<PipelineDefinitionWarning>,
}

#[derive(Default)]
pub struct RenderPipelineDefinitionUseCase;

impl RenderPipelineDefinitionUseCase {
    /// The YAML is produced even when there are problems, so that the editor can show both.
    pub fn execute(
        &self,
        definition: &PipelineDefinition,
    ) -> Result<RenderedPipelineDefinition, DomainError> {
        let yaml = render_pipeline_definition(definition)
            .map_err(|e| DomainError::Infrastructure(e.to_string()))?;
        Ok(RenderedPipelineDefinition {
            yaml,
            problems: check_pipeline_definition(definition),
            warnings: pipeline_definition_warnings(definition),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "stages: [build, test]\njobs:\n  compile:\n    stage: build\n    image: rust:1\n    script: [cargo build]\n  check:\n    stage: test\n    image: rust:1\n    script: [cargo test]\n    needs: [compile]\n";

    #[test]
    fn a_valid_file_reads_into_its_definition_without_problems() {
        let report = ReadPipelineDefinitionUseCase.execute(FILE);

        let definition = report.definition.expect("the file is valid");
        assert_eq!(definition.stages, vec!["build", "test"]);
        assert_eq!(definition.jobs.len(), 2);
        assert!(report.problems.is_empty() && report.warnings.is_empty());
        assert!(report.ignored_fields.is_empty() && !report.has_comments);
    }

    #[test]
    fn a_file_with_wrong_stages_still_opens_and_lists_what_is_wrong() {
        let yaml = "stages: [build]\njobs:\n  compile:\n    stage: nowhere\n    image: rust:1\n    script: [x]\n";

        let report = ReadPipelineDefinitionUseCase.execute(yaml);

        assert!(
            report.definition.is_some(),
            "the builder can open it and fix it"
        );
        assert_eq!(
            report.problems,
            vec![PipelineDefinitionError::UnknownStage {
                job: "compile".into(),
                stage: "nowhere".into()
            }]
        );
    }

    #[test]
    fn malformed_yaml_gives_no_definition_and_one_problem() {
        let report = ReadPipelineDefinitionUseCase.execute("stages: [build\n# oops");

        assert!(report.definition.is_none());
        assert!(matches!(
            report.problems.as_slice(),
            [PipelineDefinitionError::InvalidYaml(_)]
        ));
        assert!(report.has_comments);
    }

    #[test]
    fn what_a_rewrite_would_lose_is_reported() {
        let yaml = "# the build\nstages: [build]\ninclude: other.yml\njobs:\n  compile:\n    stage: build\n    image: rust:1\n    script: [x]\n    when: manual\n";

        let report = ReadPipelineDefinitionUseCase.execute(yaml);

        assert!(report.has_comments);
        assert_eq!(report.ignored_fields, vec!["include", "jobs.compile.when"]);
    }

    #[test]
    fn rendering_returns_the_yaml_with_the_problems_and_warnings() {
        let definition = ReadPipelineDefinitionUseCase
            .execute("stages: [build]\njobs:\n  compile:\n    stage: nowhere\n    image: ''\n    script: []\n")
            .definition
            .unwrap();

        let rendered = RenderPipelineDefinitionUseCase
            .execute(&definition)
            .unwrap();

        assert!(rendered.yaml.contains("stage: nowhere"));
        assert_eq!(rendered.problems.len(), 1);
        assert_eq!(
            rendered.warnings.len(),
            2,
            "an empty image and an empty script"
        );
    }

    #[test]
    fn a_rendered_valid_definition_reads_back_identically() {
        let definition = ReadPipelineDefinitionUseCase
            .execute(FILE)
            .definition
            .unwrap();

        let rendered = RenderPipelineDefinitionUseCase
            .execute(&definition)
            .unwrap();

        assert!(rendered.problems.is_empty());
        let again = ReadPipelineDefinitionUseCase.execute(&rendered.yaml);
        assert_eq!(again.definition.unwrap(), definition);
    }
}
