use std::collections::HashSet;
use std::sync::Arc;

use serde_json::json;
use uuid::Uuid;

use ferrisgit_domain::label::Label;
use ferrisgit_domain::merge_request::{MergeRequest, ReviewDecision};
use ferrisgit_domain::merge_request_event::{
    MergeRequestEventKind, MergeRequestEventPort, NewMergeRequestEvent,
};

/// Best-effort recorder of merge request activity for the timeline. Every method swallows
/// (and logs) a failure: a broken journal must never fail the user action that triggered it.
pub struct MergeRequestActivity {
    events: Arc<dyn MergeRequestEventPort>,
}

impl MergeRequestActivity {
    pub fn new(events: Arc<dyn MergeRequestEventPort>) -> Self {
        Self { events }
    }

    pub async fn review_submitted(&self, mr_id: Uuid, actor: Uuid, decision: ReviewDecision) {
        let decision = match decision {
            ReviewDecision::Approved => "approved",
            ReviewDecision::ChangesRequested => "changes_requested",
        };
        self.record(
            mr_id,
            Some(actor),
            MergeRequestEventKind::ReviewSubmitted,
            json!({ "decision": decision }),
        )
        .await;
    }

    pub async fn labels_changed(
        &self,
        mr_id: Uuid,
        actor: Uuid,
        before: &[Label],
        after: &[Label],
    ) {
        let before_ids: HashSet<Uuid> = before.iter().map(|l| l.id).collect();
        let after_ids: HashSet<Uuid> = after.iter().map(|l| l.id).collect();
        if before_ids == after_ids {
            return;
        }
        let entry =
            |label: &Label| json!({ "id": label.id, "name": label.name, "color": label.color });
        let added: Vec<_> = after
            .iter()
            .filter(|l| !before_ids.contains(&l.id))
            .map(entry)
            .collect();
        let removed: Vec<_> = before
            .iter()
            .filter(|l| !after_ids.contains(&l.id))
            .map(entry)
            .collect();
        self.record(
            mr_id,
            Some(actor),
            MergeRequestEventKind::LabelsChanged,
            json!({ "added": added, "removed": removed }),
        )
        .await;
    }

    /// `milestones` is the (before, after) pair of milestone titles, or `None` when they could not be looked up: an
    /// unknown milestone is never recorded as a change.
    pub async fn edited(
        &self,
        mr_id: Uuid,
        actor: Uuid,
        before: &MergeRequest,
        after: &MergeRequest,
        milestones: Option<(Option<String>, Option<String>)>,
    ) {
        if before.title != after.title {
            self.record(
                mr_id,
                Some(actor),
                MergeRequestEventKind::TitleChanged,
                json!({ "from": before.title, "to": after.title }),
            )
            .await;
        }
        if let Some((before_milestone, after_milestone)) = milestones
            && before_milestone != after_milestone
        {
            self.record(
                mr_id,
                Some(actor),
                MergeRequestEventKind::MilestoneChanged,
                json!({ "from": before_milestone, "to": after_milestone }),
            )
            .await;
        }
    }

    pub async fn merged(&self, mr_id: Uuid, actor: Uuid, merge_commit_sha: Option<&str>) {
        self.record(
            mr_id,
            Some(actor),
            MergeRequestEventKind::Merged,
            json!({ "mergeCommitSha": merge_commit_sha }),
        )
        .await;
    }

    pub async fn closed(&self, mr_id: Uuid, actor: Uuid) {
        self.record(mr_id, Some(actor), MergeRequestEventKind::Closed, json!({}))
            .await;
    }

    pub async fn thread_resolved(
        &self,
        mr_id: Uuid,
        actor: Uuid,
        comment_id: Uuid,
        resolved: bool,
    ) {
        let kind = if resolved {
            MergeRequestEventKind::ThreadResolved
        } else {
            MergeRequestEventKind::ThreadReopened
        };
        self.record(mr_id, Some(actor), kind, json!({ "commentId": comment_id }))
            .await;
    }

    pub async fn commits_pushed(&self, mr_id: Uuid, actor: Uuid, from_sha: &str, to_sha: &str) {
        self.record(
            mr_id,
            Some(actor),
            MergeRequestEventKind::CommitsPushed,
            json!({ "fromSha": from_sha, "toSha": to_sha }),
        )
        .await;
    }

    async fn record(
        &self,
        mr_id: Uuid,
        actor: Option<Uuid>,
        kind: MergeRequestEventKind,
        payload: serde_json::Value,
    ) {
        if let Err(err) = self
            .events
            .record(NewMergeRequestEvent {
                merge_request_id: mr_id,
                actor_id: actor,
                kind,
                payload,
            })
            .await
        {
            tracing::warn!(error = %err, merge_request_id = %mr_id, kind = kind.as_str(), "failed to record merge request event");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeMergeRequestEvents;
    use chrono::Utc;
    use ferrisgit_domain::merge_request::MergeRequestStatus;

    fn label(name: &str) -> Label {
        Label {
            id: Uuid::new_v4(),
            name: name.to_string(),
            color: "#ff0000".to_string(),
            repository_id: Some(Uuid::new_v4()),
            group_id: None,
            created_at: Utc::now(),
        }
    }

    fn merge_request(title: &str) -> MergeRequest {
        MergeRequest {
            id: Uuid::new_v4(),
            repository_id: Uuid::new_v4(),
            author_id: Some(Uuid::new_v4()),
            source_branch: "feature".to_string(),
            target_branch: "main".to_string(),
            title: title.to_string(),
            description: String::new(),
            status: MergeRequestStatus::Open,
            merge_commit_sha: None,
            milestone_id: None,
            created_at: Utc::now(),
            closed_at: None,
        }
    }

    fn setup() -> (Arc<FakeMergeRequestEvents>, MergeRequestActivity) {
        let fake = Arc::new(FakeMergeRequestEvents::new());
        let activity = MergeRequestActivity::new(fake.clone());
        (fake, activity)
    }

    #[tokio::test]
    async fn review_submitted_records_the_decision_with_the_actor() {
        let (fake, activity) = setup();
        let (mr, actor) = (Uuid::new_v4(), Uuid::new_v4());
        activity
            .review_submitted(mr, actor, ReviewDecision::Approved)
            .await;
        activity
            .review_submitted(mr, actor, ReviewDecision::ChangesRequested)
            .await;
        let events = fake.snapshot();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, MergeRequestEventKind::ReviewSubmitted);
        assert_eq!(events[0].merge_request_id, mr);
        assert_eq!(events[0].actor_id, Some(actor));
        assert_eq!(events[0].payload, json!({"decision": "approved"}));
        assert_eq!(events[1].payload, json!({"decision": "changes_requested"}));
    }

    #[tokio::test]
    async fn labels_changed_records_added_and_removed_by_id() {
        let (fake, activity) = setup();
        let (kept, dropped, added) = (label("kept"), label("dropped"), label("added"));
        activity
            .labels_changed(
                Uuid::new_v4(),
                Uuid::new_v4(),
                &[kept.clone(), dropped.clone()],
                &[kept, added.clone()],
            )
            .await;
        let events = fake.snapshot();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, MergeRequestEventKind::LabelsChanged);
        assert_eq!(
            events[0].payload,
            json!({
                "added": [{"id": added.id, "name": "added", "color": "#ff0000"}],
                "removed": [{"id": dropped.id, "name": "dropped", "color": "#ff0000"}],
            })
        );
    }

    #[tokio::test]
    async fn labels_changed_records_nothing_when_the_id_sets_are_equal() {
        let (fake, activity) = setup();
        let (a, b) = (label("a"), label("b"));
        activity
            .labels_changed(
                Uuid::new_v4(),
                Uuid::new_v4(),
                &[a.clone(), b.clone()],
                &[b, a],
            )
            .await;
        activity
            .labels_changed(Uuid::new_v4(), Uuid::new_v4(), &[], &[])
            .await;
        assert!(fake.snapshot().is_empty());
    }

    #[tokio::test]
    async fn edited_records_a_title_change_only_when_titles_differ() {
        let (fake, activity) = setup();
        activity
            .edited(
                Uuid::new_v4(),
                Uuid::new_v4(),
                &merge_request("Old"),
                &merge_request("New"),
                Some((None, None)),
            )
            .await;
        let events = fake.snapshot();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, MergeRequestEventKind::TitleChanged);
        assert_eq!(events[0].payload, json!({"from": "Old", "to": "New"}));
    }

    #[tokio::test]
    async fn edited_records_a_milestone_change_only_when_milestone_titles_differ() {
        let (fake, activity) = setup();
        activity
            .edited(
                Uuid::new_v4(),
                Uuid::new_v4(),
                &merge_request("Same"),
                &merge_request("Same"),
                Some((None, Some("v1".to_string()))),
            )
            .await;
        activity
            .edited(
                Uuid::new_v4(),
                Uuid::new_v4(),
                &merge_request("Same"),
                &merge_request("Same"),
                Some((Some("v1".to_string()), None)),
            )
            .await;
        let events = fake.snapshot();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, MergeRequestEventKind::MilestoneChanged);
        assert_eq!(events[0].payload, json!({"from": null, "to": "v1"}));
        assert_eq!(events[1].payload, json!({"from": "v1", "to": null}));
    }

    #[tokio::test]
    async fn edited_records_both_events_when_both_differ_and_none_when_neither() {
        let (fake, activity) = setup();
        activity
            .edited(
                Uuid::new_v4(),
                Uuid::new_v4(),
                &merge_request("Same"),
                &merge_request("Same"),
                Some((Some("v1".to_string()), Some("v1".to_string()))),
            )
            .await;
        assert!(fake.snapshot().is_empty());
        activity
            .edited(
                Uuid::new_v4(),
                Uuid::new_v4(),
                &merge_request("Old"),
                &merge_request("New"),
                Some((Some("v1".to_string()), Some("v2".to_string()))),
            )
            .await;
        let kinds: Vec<_> = fake.snapshot().iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec![
                MergeRequestEventKind::TitleChanged,
                MergeRequestEventKind::MilestoneChanged
            ]
        );
    }

    #[tokio::test]
    async fn edited_skips_only_the_milestone_part_when_the_milestone_titles_are_unknown() {
        let (fake, activity) = setup();
        activity
            .edited(
                Uuid::new_v4(),
                Uuid::new_v4(),
                &merge_request("Old"),
                &merge_request("New"),
                None,
            )
            .await;
        activity
            .edited(
                Uuid::new_v4(),
                Uuid::new_v4(),
                &merge_request("Same"),
                &merge_request("Same"),
                None,
            )
            .await;
        let events = fake.snapshot();
        assert_eq!(
            events.len(),
            1,
            "an unknown milestone is not a milestone change"
        );
        assert_eq!(events[0].kind, MergeRequestEventKind::TitleChanged);
    }

    #[tokio::test]
    async fn merged_records_the_merge_commit_sha_and_tolerates_none() {
        let (fake, activity) = setup();
        activity
            .merged(Uuid::new_v4(), Uuid::new_v4(), Some("abc123"))
            .await;
        activity.merged(Uuid::new_v4(), Uuid::new_v4(), None).await;
        let events = fake.snapshot();
        assert_eq!(events[0].kind, MergeRequestEventKind::Merged);
        assert_eq!(events[0].payload, json!({"mergeCommitSha": "abc123"}));
        assert_eq!(events[1].payload, json!({"mergeCommitSha": null}));
    }

    #[tokio::test]
    async fn closed_records_an_empty_payload() {
        let (fake, activity) = setup();
        let actor = Uuid::new_v4();
        activity.closed(Uuid::new_v4(), actor).await;
        let events = fake.snapshot();
        assert_eq!(events[0].kind, MergeRequestEventKind::Closed);
        assert_eq!(events[0].actor_id, Some(actor));
        assert_eq!(events[0].payload, json!({}));
    }

    #[tokio::test]
    async fn thread_resolved_maps_the_flag_to_resolved_or_reopened() {
        let (fake, activity) = setup();
        let comment = Uuid::new_v4();
        activity
            .thread_resolved(Uuid::new_v4(), Uuid::new_v4(), comment, true)
            .await;
        activity
            .thread_resolved(Uuid::new_v4(), Uuid::new_v4(), comment, false)
            .await;
        let events = fake.snapshot();
        assert_eq!(events[0].kind, MergeRequestEventKind::ThreadResolved);
        assert_eq!(events[1].kind, MergeRequestEventKind::ThreadReopened);
        assert_eq!(events[0].payload, json!({"commentId": comment}));
        assert_eq!(events[1].payload, json!({"commentId": comment}));
    }

    #[tokio::test]
    async fn commits_pushed_records_both_shas() {
        let (fake, activity) = setup();
        activity
            .commits_pushed(Uuid::new_v4(), Uuid::new_v4(), "aaa", "bbb")
            .await;
        let events = fake.snapshot();
        assert_eq!(events[0].kind, MergeRequestEventKind::CommitsPushed);
        assert_eq!(events[0].payload, json!({"fromSha": "aaa", "toSha": "bbb"}));
    }

    #[tokio::test]
    async fn a_failing_store_never_panics_or_surfaces_an_error() {
        let activity = MergeRequestActivity::new(Arc::new(FakeMergeRequestEvents::failing()));
        let (mr, actor) = (Uuid::new_v4(), Uuid::new_v4());
        activity
            .review_submitted(mr, actor, ReviewDecision::Approved)
            .await;
        activity.labels_changed(mr, actor, &[], &[label("x")]).await;
        activity
            .edited(
                mr,
                actor,
                &merge_request("a"),
                &merge_request("b"),
                Some((None, Some("v".to_string()))),
            )
            .await;
        activity.merged(mr, actor, Some("sha")).await;
        activity.closed(mr, actor).await;
        activity
            .thread_resolved(mr, actor, Uuid::new_v4(), true)
            .await;
        activity.commits_pushed(mr, actor, "a", "b").await;
    }
}
