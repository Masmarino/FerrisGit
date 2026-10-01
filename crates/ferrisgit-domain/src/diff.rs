use async_trait::async_trait;
use serde::Serialize;

use crate::error::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileChangeKind {
    Added,
    Modified,
    Deleted,
    Binary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffSide {
    Old,
    New,
}

impl DiffSide {
    pub fn as_str(&self) -> &'static str {
        match self {
            DiffSide::Old => "old",
            DiffSide::New => "new",
        }
    }

    pub fn parse(value: &str) -> Result<Self, DomainError> {
        match value {
            "old" => Ok(DiffSide::Old),
            "new" => Ok(DiffSide::New),
            other => Err(DomainError::Validation(format!(
                "unknown diff side: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffLineKind {
    Context,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub content: String,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Hunk {
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FileDiff {
    pub path: String,
    pub change: FileChangeKind,
    pub hunks: Vec<Hunk>,
}

/// Content of a diff line by file path, line number (as on `side`) and side. Validates a new comment's anchor and
/// decides whether an existing one is outdated (`MergeRequestComment::anchor_content`).
pub fn find_line<'a>(
    diffs: &'a [FileDiff],
    file_path: &str,
    line_number: i32,
    side: DiffSide,
) -> Option<&'a str> {
    let file = diffs.iter().find(|f| f.path == file_path)?;
    for hunk in &file.hunks {
        for line in &hunk.lines {
            let matches = match side {
                DiffSide::Old => line.old_line == Some(line_number as u32),
                DiffSide::New => line.new_line == Some(line_number as u32),
            };
            if matches {
                return Some(line.content.as_str());
            }
        }
    }
    None
}

/// Like `find_line` for an inclusive range: every line in `start_line..=end_line` must exist on that side without gaps.
/// A gap (for example across a hunk boundary) returns `None`.
pub fn find_lines(
    diffs: &[FileDiff],
    file_path: &str,
    start_line: i32,
    end_line: i32,
    side: DiffSide,
) -> Option<String> {
    let mut content = String::new();
    for line_number in start_line..=end_line {
        content.push_str(find_line(diffs, file_path, line_number, side)?);
    }
    Some(content)
}

/// Picks the single-line or the range lookup. Comparing the result with a comment's `anchor_content` decides
/// "outdated". `None` means the anchor no longer exists in the diff (file gone or a line removed), which is as outdated
/// as changed content.
pub fn resolve_anchor_content(
    diffs: &[FileDiff],
    file_path: &str,
    line_number: i32,
    end_line: Option<i32>,
    side: DiffSide,
) -> Option<String> {
    match end_line {
        Some(end_line) => find_lines(diffs, file_path, line_number, end_line, side),
        None => find_line(diffs, file_path, line_number, side).map(str::to_string),
    }
}

/// Diffs merge-base(`source_branch`, `target_branch`) to `source_branch`'s tip. Nothing is cached.
#[async_trait]
pub trait DiffReaderPort: Send + Sync {
    async fn diff_branches(
        &self,
        repository_disk_path: &str,
        source_branch: &str,
        target_branch: &str,
    ) -> Result<Vec<FileDiff>, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct FakeReader;
    #[async_trait]
    impl DiffReaderPort for FakeReader {
        async fn diff_branches(
            &self,
            _repository_disk_path: &str,
            _source_branch: &str,
            _target_branch: &str,
        ) -> Result<Vec<FileDiff>, DomainError> {
            Ok(vec![FileDiff {
                path: "README.md".to_string(),
                change: FileChangeKind::Modified,
                hunks: vec![Hunk {
                    lines: vec![DiffLine {
                        kind: DiffLineKind::Added,
                        content: "hello".to_string(),
                        old_line: None,
                        new_line: Some(1),
                    }],
                }],
            }])
        }
    }

    #[tokio::test]
    async fn a_port_implementation_can_be_exercised_through_the_trait_object() {
        let reader: Arc<dyn DiffReaderPort> = Arc::new(FakeReader);
        let diffs = reader
            .diff_branches("path", "feature", "main")
            .await
            .unwrap();
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].path, "README.md");
    }

    #[test]
    fn diff_side_round_trips_through_its_string_form() {
        assert_eq!(DiffSide::parse("old").unwrap(), DiffSide::Old);
        assert_eq!(DiffSide::parse("new").unwrap(), DiffSide::New);
        assert_eq!(DiffSide::Old.as_str(), "old");
        assert_eq!(DiffSide::New.as_str(), "new");
    }

    #[test]
    fn parsing_an_unknown_diff_side_is_a_validation_error() {
        assert!(matches!(
            DiffSide::parse("left"),
            Err(DomainError::Validation(_))
        ));
    }

    fn sample_diffs() -> Vec<FileDiff> {
        vec![FileDiff {
            path: "README.md".to_string(),
            change: FileChangeKind::Modified,
            hunks: vec![Hunk {
                lines: vec![
                    DiffLine {
                        kind: DiffLineKind::Context,
                        content: "line one\n".to_string(),
                        old_line: Some(1),
                        new_line: Some(1),
                    },
                    DiffLine {
                        kind: DiffLineKind::Removed,
                        content: "line two\n".to_string(),
                        old_line: Some(2),
                        new_line: None,
                    },
                    DiffLine {
                        kind: DiffLineKind::Added,
                        content: "line 2\n".to_string(),
                        old_line: None,
                        new_line: Some(2),
                    },
                ],
            }],
        }]
    }

    #[test]
    fn find_line_locates_a_context_line_on_either_side() {
        let diffs = sample_diffs();
        assert_eq!(
            find_line(&diffs, "README.md", 1, DiffSide::Old),
            Some("line one\n")
        );
        assert_eq!(
            find_line(&diffs, "README.md", 1, DiffSide::New),
            Some("line one\n")
        );
    }

    #[test]
    fn find_line_locates_a_removed_line_only_on_the_old_side() {
        let diffs = sample_diffs();
        assert_eq!(
            find_line(&diffs, "README.md", 2, DiffSide::Old),
            Some("line two\n")
        );
        assert_eq!(
            find_line(&diffs, "README.md", 2, DiffSide::New),
            Some("line 2\n")
        );
    }

    #[test]
    fn find_line_returns_none_for_a_missing_file() {
        let diffs = sample_diffs();
        assert_eq!(find_line(&diffs, "nope.md", 1, DiffSide::New), None);
    }

    #[test]
    fn find_line_returns_none_for_a_line_number_that_does_not_exist_on_that_side() {
        let diffs = sample_diffs();
        assert_eq!(find_line(&diffs, "README.md", 99, DiffSide::New), None);
    }

    #[test]
    fn find_lines_concatenates_a_contiguous_range_in_order() {
        let diffs = sample_diffs();
        assert_eq!(
            find_lines(&diffs, "README.md", 1, 2, DiffSide::New),
            Some("line one\nline 2\n".to_string())
        );
        assert_eq!(
            find_lines(&diffs, "README.md", 1, 2, DiffSide::Old),
            Some("line one\nline two\n".to_string())
        );
    }

    #[test]
    fn find_lines_returns_none_when_any_line_in_the_range_is_missing() {
        let diffs = sample_diffs();
        assert_eq!(find_lines(&diffs, "README.md", 1, 99, DiffSide::New), None);
    }

    #[test]
    fn resolve_anchor_content_dispatches_to_find_line_when_there_is_no_end_line() {
        let diffs = sample_diffs();
        assert_eq!(
            resolve_anchor_content(&diffs, "README.md", 1, None, DiffSide::New),
            Some("line one\n".to_string())
        );
    }

    #[test]
    fn resolve_anchor_content_dispatches_to_find_lines_when_an_end_line_is_given() {
        let diffs = sample_diffs();
        assert_eq!(
            resolve_anchor_content(&diffs, "README.md", 1, Some(2), DiffSide::New),
            Some("line one\nline 2\n".to_string())
        );
    }

    #[test]
    fn resolve_anchor_content_is_none_once_a_single_anchored_line_leaves_the_diff() {
        let diffs = sample_diffs();
        assert_eq!(
            resolve_anchor_content(&diffs, "README.md", 99, None, DiffSide::New),
            None
        );
    }

    #[test]
    fn resolve_anchor_content_is_none_when_any_line_in_an_anchored_range_leaves_the_diff() {
        let diffs = sample_diffs();
        assert_eq!(
            resolve_anchor_content(&diffs, "README.md", 1, Some(99), DiffSide::New),
            None
        );
    }
}
