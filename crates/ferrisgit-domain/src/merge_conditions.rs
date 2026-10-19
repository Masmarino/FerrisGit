//! What a merge request needs before it can be merged, condition by condition, so that both the merge and the interface
//! say which one is missing.

use std::collections::BTreeMap;

use serde::Serialize;

/// Where a check stands on a merge request's current source commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Pending,
    Running,
    Success,
    Failed,
    Canceled,
    Skipped,
}

impl CheckStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            CheckStatus::Pending => "pending",
            CheckStatus::Running => "running",
            CheckStatus::Success => "success",
            CheckStatus::Failed => "failed",
            CheckStatus::Canceled => "canceled",
            CheckStatus::Skipped => "skipped",
        }
    }
}

/// The check every merge request pipeline reports, with the status of the whole pipeline.
pub const PIPELINE_CHECK: &str = "pipeline";

/// The check a job of the merge request pipeline reports: `pipeline/<job name>`.
pub fn job_check_name(job_name: &str) -> String {
    format!("{PIPELINE_CHECK}/{job_name}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeCondition {
    /// Approvals given on the current source commit, against those required.
    Approvals { required: i32, given: i32 },
    /// Nobody asks for changes on the current source commit. Only a condition when approvals are required.
    NoChangesRequested { requested_by: Vec<String> },
    /// A named check has succeeded on the current source commit. `None` when it has not run there.
    Check {
        name: String,
        status: Option<CheckStatus>,
    },
}

impl MergeCondition {
    pub fn is_met(&self) -> bool {
        match self {
            MergeCondition::Approvals { required, given } => given >= required,
            MergeCondition::NoChangesRequested { requested_by } => requested_by.is_empty(),
            MergeCondition::Check { status, .. } => *status == Some(CheckStatus::Success),
        }
    }

    /// What is missing, in the words of the API.
    pub fn problem(&self) -> String {
        match self {
            MergeCondition::Approvals { required, given } => {
                format!("{given}/{required} approvals")
            }
            MergeCondition::NoChangesRequested { requested_by } => {
                format!("changes requested by {}", requested_by.join(", "))
            }
            MergeCondition::Check { name, status } => match status {
                None => {
                    format!("check '{name}' has not run on the latest commit of the source branch")
                }
                Some(CheckStatus::Pending | CheckStatus::Running) => {
                    format!("check '{name}' has not finished")
                }
                Some(status) => format!("check '{name}' is {}", status.as_str()),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Mergeability {
    pub conditions: Vec<MergeCondition>,
}

impl Mergeability {
    pub fn is_mergeable(&self) -> bool {
        self.conditions.iter().all(MergeCondition::is_met)
    }

    /// Every condition that is not met, in one sentence; `None` when the merge can go ahead.
    pub fn refusal(&self) -> Option<String> {
        let problems: Vec<String> = self
            .conditions
            .iter()
            .filter(|condition| !condition.is_met())
            .map(MergeCondition::problem)
            .collect();
        (!problems.is_empty()).then(|| problems.join("; "))
    }
}

/// The conditions of a merge: `required_approvals` (none when 0), with `approved_by` and `changes_requested_by` the
/// reviewers of the current source commit, and each of `required_checks` looked up in `checks`.
pub fn evaluate_mergeability(
    required_approvals: i32,
    approved_by: usize,
    changes_requested_by: Vec<String>,
    required_checks: &[String],
    checks: &BTreeMap<String, CheckStatus>,
) -> Mergeability {
    let mut conditions = Vec::new();
    if required_approvals > 0 {
        conditions.push(MergeCondition::NoChangesRequested {
            requested_by: changes_requested_by,
        });
        conditions.push(MergeCondition::Approvals {
            required: required_approvals,
            given: approved_by as i32,
        });
    }
    for name in required_checks {
        conditions.push(MergeCondition::Check {
            name: name.clone(),
            status: checks.get(name).copied(),
        });
    }
    Mergeability { conditions }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checks(entries: &[(&str, CheckStatus)]) -> BTreeMap<String, CheckStatus> {
        entries
            .iter()
            .map(|(name, status)| (name.to_string(), *status))
            .collect()
    }

    #[test]
    fn nothing_required_means_mergeable() {
        let mergeability = evaluate_mergeability(0, 0, vec!["bob".to_string()], &[], &checks(&[]));
        assert!(mergeability.conditions.is_empty());
        assert!(mergeability.is_mergeable());
        assert_eq!(mergeability.refusal(), None);
    }

    #[test]
    fn missing_approvals_and_requested_changes_are_both_reported() {
        let mergeability = evaluate_mergeability(2, 1, vec!["bob".to_string()], &[], &checks(&[]));

        assert!(!mergeability.is_mergeable());
        assert_eq!(
            mergeability.refusal().unwrap(),
            "changes requested by bob; 1/2 approvals"
        );
        let enough = evaluate_mergeability(2, 2, vec![], &[], &checks(&[]));
        assert!(enough.is_mergeable());
    }

    #[test]
    fn a_required_check_must_have_succeeded() {
        let required = vec![
            "pipeline".to_string(),
            "pipeline/lint".to_string(),
            "pipeline/test".to_string(),
            "quality-gate".to_string(),
        ];
        let mergeability = evaluate_mergeability(
            0,
            0,
            vec![],
            &required,
            &checks(&[
                ("pipeline", CheckStatus::Running),
                ("pipeline/lint", CheckStatus::Success),
                ("pipeline/test", CheckStatus::Failed),
            ]),
        );

        let met: Vec<bool> = mergeability
            .conditions
            .iter()
            .map(MergeCondition::is_met)
            .collect();
        assert_eq!(met, vec![false, true, false, false]);
        assert_eq!(
            mergeability.refusal().unwrap(),
            "check 'pipeline' has not finished; check 'pipeline/test' is failed; check 'quality-gate' has not run on the latest commit of the source branch"
        );
    }

    #[test]
    fn a_job_check_is_named_after_the_pipeline() {
        assert_eq!(job_check_name("unit"), "pipeline/unit");
    }
}
