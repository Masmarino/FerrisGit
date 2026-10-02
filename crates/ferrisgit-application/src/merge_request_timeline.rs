use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use uuid::Uuid;

use ferrisgit_domain::diff::{DiffLineKind, DiffSide, FileDiff, resolve_anchor_content};
use ferrisgit_domain::merge_request_comment::MergeRequestComment;
use ferrisgit_domain::merge_request_event::{MergeRequestEvent, MergeRequestEventKind};

/// Number of diff lines shown above a thread: the anchor line plus the two before it.
const EXCERPT_LINES: usize = 3;

#[derive(Debug, Clone, PartialEq)]
pub struct ExcerptLine {
    pub line: Option<u32>,
    pub kind: DiffLineKind,
    pub content: String,
}

pub struct TimelineThread {
    pub root: MergeRequestComment,
    pub replies: Vec<MergeRequestComment>,
    pub outdated: bool,
    pub resolved_by: Option<Uuid>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub excerpt: Vec<ExcerptLine>,
}

pub enum TimelineItem {
    Comment(MergeRequestComment),
    Thread(TimelineThread),
    Event(MergeRequestEvent),
}

impl TimelineItem {
    pub fn created_at(&self) -> DateTime<Utc> {
        match self {
            TimelineItem::Comment(comment) => comment.created_at,
            TimelineItem::Thread(thread) => thread.root.created_at,
            TimelineItem::Event(event) => event.created_at,
        }
    }

    pub fn id(&self) -> Uuid {
        match self {
            TimelineItem::Comment(comment) => comment.id,
            TimelineItem::Thread(thread) => thread.root.id,
            TimelineItem::Event(event) => event.id,
        }
    }
}

/// Whether the live diff no longer holds the content a comment was anchored to.
pub fn is_outdated(diffs: &[FileDiff], comment: &MergeRequestComment) -> bool {
    let (Some(file_path), Some(line_number), Some(side)) = (
        comment.file_path.as_deref(),
        comment.line_number,
        comment.side,
    ) else {
        return false;
    };
    resolve_anchor_content(diffs, file_path, line_number, comment.end_line, side).as_deref()
        != comment.anchor_content.as_deref()
}

/// Up to three diff lines ending at `end_line` (the anchor plus the two lines before it),
/// numbered on `side`. Empty when the file or the line is not part of the diff.
fn excerpt_for(
    diffs: &[FileDiff],
    file_path: &str,
    end_line: u32,
    side: DiffSide,
) -> Vec<ExcerptLine> {
    let Some(file) = diffs.iter().find(|f| f.path == file_path) else {
        return Vec::new();
    };
    let lines: Vec<_> = file.hunks.iter().flat_map(|h| h.lines.iter()).collect();
    let number_on_side = |line: &&ferrisgit_domain::diff::DiffLine| match side {
        DiffSide::Old => line.old_line,
        DiffSide::New => line.new_line,
    };
    let Some(anchor) = lines
        .iter()
        .position(|line| number_on_side(line) == Some(end_line))
    else {
        return Vec::new();
    };
    let start = (anchor + 1).saturating_sub(EXCERPT_LINES);
    lines[start..=anchor]
        .iter()
        .map(|line| ExcerptLine {
            line: number_on_side(line),
            kind: line.kind,
            content: line.content.clone(),
        })
        .collect()
}

/// Merges comments and journal events into one chronological stream. Pure: the caller
/// fetches everything (and the live diff, when it could be computed) and passes it in.
pub fn assemble_timeline(
    comments: Vec<MergeRequestComment>,
    events: Vec<MergeRequestEvent>,
    diffs: Option<&[FileDiff]>,
) -> Vec<TimelineItem> {
    let is_thread_root = |c: &MergeRequestComment| {
        c.reply_to_id.is_none()
            && c.file_path.is_some()
            && c.line_number.is_some()
            && c.side.is_some()
    };
    let thread_root_ids: HashSet<Uuid> = comments
        .iter()
        .filter(|c| is_thread_root(c))
        .map(|c| c.id)
        .collect();
    let comment_count = comments.len();
    let parent_of: HashMap<Uuid, Uuid> = comments
        .iter()
        .filter_map(|c| c.reply_to_id.map(|parent| (c.id, parent)))
        .collect();

    // Walks up the reply chain to the thread root it hangs off, if any. The hop bound is only there in case of a cyclic
    // chain, which the store never produces.
    let thread_root_of = |comment: &MergeRequestComment| -> Option<Uuid> {
        let mut current = comment.reply_to_id?;
        for _ in 0..comment_count {
            if thread_root_ids.contains(&current) {
                return Some(current);
            }
            current = *parent_of.get(&current)?;
        }
        None
    };

    let mut items = Vec::new();
    let mut roots = Vec::new();
    let mut replies_by_root: HashMap<Uuid, Vec<MergeRequestComment>> = HashMap::new();
    for comment in comments {
        if is_thread_root(&comment) {
            roots.push(comment);
        } else if let Some(root_id) = thread_root_of(&comment) {
            replies_by_root.entry(root_id).or_default().push(comment);
        } else {
            items.push(TimelineItem::Comment(comment));
        }
    }

    // The latest resolve/reopen event per thread decides who resolved it and when.
    let mut latest_resolution: HashMap<Uuid, &MergeRequestEvent> = HashMap::new();
    for event in events.iter().filter(|e| {
        matches!(
            e.kind,
            MergeRequestEventKind::ThreadResolved | MergeRequestEventKind::ThreadReopened
        )
    }) {
        let Some(comment_id) = event
            .payload
            .get("commentId")
            .and_then(|v| v.as_str())
            .and_then(|v| Uuid::parse_str(v).ok())
        else {
            continue;
        };
        let is_newer = latest_resolution
            .get(&comment_id)
            .is_none_or(|current| (event.created_at, event.id) > (current.created_at, current.id));
        if is_newer {
            latest_resolution.insert(comment_id, event);
        }
    }

    for root in roots {
        let mut replies = replies_by_root.remove(&root.id).unwrap_or_default();
        replies.sort_by_key(|reply| (reply.created_at, reply.id));
        let resolution = latest_resolution
            .get(&root.id)
            .filter(|event| root.resolved && event.kind == MergeRequestEventKind::ThreadResolved);
        let (outdated, excerpt) = match (
            diffs,
            root.file_path.as_deref(),
            root.line_number,
            root.side,
        ) {
            (Some(diffs), Some(file_path), Some(line_number), Some(side)) => {
                let end_line = root.end_line.unwrap_or(line_number);
                let outdated = is_outdated(diffs, &root);
                // An outdated anchor no longer matches what is on that line: showing the code there now would
                // mislead.
                let excerpt = if outdated {
                    Vec::new()
                } else {
                    excerpt_for(diffs, file_path, end_line.max(0) as u32, side)
                };
                (outdated, excerpt)
            }
            _ => (false, Vec::new()),
        };
        items.push(TimelineItem::Thread(TimelineThread {
            resolved_by: resolution.and_then(|event| event.actor_id),
            resolved_at: resolution.map(|event| event.created_at),
            outdated,
            excerpt,
            replies,
            root,
        }));
    }

    items.extend(
        events
            .into_iter()
            .filter(|e| {
                !matches!(
                    e.kind,
                    MergeRequestEventKind::ThreadResolved | MergeRequestEventKind::ThreadReopened
                )
            })
            .map(TimelineItem::Event),
    );
    items.sort_by_key(|item| (item.created_at(), item.id()));
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use ferrisgit_domain::diff::{DiffLine, FileChangeKind, Hunk};
    use serde_json::json;

    fn at(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_700_000_000 + secs, 0).unwrap()
    }

    fn comment(
        id: Uuid,
        created_at: DateTime<Utc>,
        reply_to: Option<Uuid>,
        file: Option<&str>,
        line: Option<i32>,
        side: Option<DiffSide>,
        resolved: bool,
    ) -> MergeRequestComment {
        MergeRequestComment {
            id,
            merge_request_id: Uuid::nil(),
            author_id: Some(Uuid::new_v4()),
            body: "body".to_string(),
            created_at,
            reply_to_id: reply_to,
            file_path: file.map(str::to_string),
            line_number: line,
            side,
            anchor_content: file.map(|_| "anchor\n".to_string()),
            resolved,
            end_line: None,
            suggested_content: None,
            applied_at: None,
            applied_commit_sha: None,
        }
    }

    fn general(id: Uuid, created_at: DateTime<Utc>) -> MergeRequestComment {
        comment(id, created_at, None, None, None, None, false)
    }

    fn inline_root(id: Uuid, created_at: DateTime<Utc>, resolved: bool) -> MergeRequestComment {
        comment(
            id,
            created_at,
            None,
            Some("a.rs"),
            Some(2),
            Some(DiffSide::New),
            resolved,
        )
    }

    fn event(
        kind: MergeRequestEventKind,
        created_at: DateTime<Utc>,
        actor: Option<Uuid>,
        payload: serde_json::Value,
    ) -> MergeRequestEvent {
        MergeRequestEvent {
            id: Uuid::new_v4(),
            merge_request_id: Uuid::nil(),
            actor_id: actor,
            kind,
            payload,
            created_at,
        }
    }

    fn diff_line(
        kind: DiffLineKind,
        content: &str,
        old_line: Option<u32>,
        new_line: Option<u32>,
    ) -> DiffLine {
        DiffLine {
            kind,
            content: content.to_string(),
            old_line,
            new_line,
        }
    }

    fn sample_diffs() -> Vec<FileDiff> {
        vec![FileDiff {
            path: "a.rs".to_string(),
            change: FileChangeKind::Modified,
            hunks: vec![Hunk {
                lines: vec![
                    diff_line(DiffLineKind::Context, "one\n", Some(1), Some(1)),
                    diff_line(DiffLineKind::Removed, "two\n", Some(2), None),
                    diff_line(DiffLineKind::Added, "anchor\n", None, Some(2)),
                    diff_line(DiffLineKind::Context, "four\n", Some(3), Some(3)),
                ],
            }],
        }]
    }

    fn ids(items: &[TimelineItem]) -> Vec<Uuid> {
        items.iter().map(TimelineItem::id).collect()
    }

    #[test]
    fn items_are_sorted_by_created_at_with_ties_broken_by_id() {
        let (low, high) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let late = general(Uuid::from_u128(9), at(30));
        let tie_high = general(high, at(10));
        let tie_low = general(low, at(10));
        let timeline = assemble_timeline(vec![late, tie_high, tie_low], vec![], None);
        assert_eq!(ids(&timeline), vec![low, high, Uuid::from_u128(9)]);
    }

    #[test]
    fn an_anchored_root_becomes_a_thread_positioned_by_the_root_with_sorted_replies() {
        let root_id = Uuid::new_v4();
        let (early_reply, late_reply) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let comments = vec![
            comment(late_reply, at(50), Some(root_id), None, None, None, false),
            inline_root(root_id, at(10), false),
            comment(early_reply, at(20), Some(root_id), None, None, None, false),
        ];
        let between = general(Uuid::new_v4(), at(15));
        let mut all = comments;
        all.push(between.clone());
        let timeline = assemble_timeline(all, vec![], None);
        assert_eq!(timeline.len(), 2);
        assert_eq!(ids(&timeline), vec![root_id, between.id]);
        let TimelineItem::Thread(thread) = &timeline[0] else {
            panic!("expected a thread")
        };
        assert_eq!(thread.root.id, root_id);
        assert_eq!(
            thread.replies.iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![early_reply, late_reply]
        );
        assert_eq!(timeline[0].created_at(), at(10));
    }

    #[test]
    fn a_comment_without_an_anchor_is_a_plain_comment() {
        let timeline = assemble_timeline(vec![general(Uuid::new_v4(), at(1))], vec![], None);
        assert!(matches!(timeline[0], TimelineItem::Comment(_)));
    }

    #[test]
    fn a_reply_to_a_general_comment_is_a_plain_comment_item() {
        let parent = general(Uuid::new_v4(), at(1));
        let reply = comment(
            Uuid::new_v4(),
            at(2),
            Some(parent.id),
            None,
            None,
            None,
            false,
        );
        let timeline = assemble_timeline(vec![parent, reply], vec![], None);
        assert_eq!(timeline.len(), 2);
        assert!(
            timeline
                .iter()
                .all(|item| matches!(item, TimelineItem::Comment(_)))
        );
    }

    #[test]
    fn thread_resolution_events_never_appear_as_items() {
        let root = inline_root(Uuid::new_v4(), at(1), true);
        let events = vec![
            event(
                MergeRequestEventKind::ThreadResolved,
                at(2),
                Some(Uuid::new_v4()),
                json!({"commentId": root.id}),
            ),
            event(
                MergeRequestEventKind::ThreadReopened,
                at(3),
                Some(Uuid::new_v4()),
                json!({"commentId": root.id}),
            ),
        ];
        let timeline = assemble_timeline(vec![root], events, None);
        assert_eq!(timeline.len(), 1);
        assert!(matches!(timeline[0], TimelineItem::Thread(_)));
    }

    #[test]
    fn a_resolved_thread_takes_resolver_and_time_from_its_latest_resolved_event() {
        let root = inline_root(Uuid::new_v4(), at(1), true);
        let other_thread = Uuid::new_v4();
        let resolver = Uuid::new_v4();
        let events = vec![
            event(
                MergeRequestEventKind::ThreadResolved,
                at(5),
                Some(Uuid::new_v4()),
                json!({"commentId": root.id}),
            ),
            event(
                MergeRequestEventKind::ThreadReopened,
                at(6),
                Some(Uuid::new_v4()),
                json!({"commentId": root.id}),
            ),
            event(
                MergeRequestEventKind::ThreadResolved,
                at(7),
                Some(resolver),
                json!({"commentId": root.id}),
            ),
            event(
                MergeRequestEventKind::ThreadResolved,
                at(9),
                Some(Uuid::new_v4()),
                json!({"commentId": other_thread}),
            ),
        ];
        let timeline = assemble_timeline(vec![root], events, None);
        let TimelineItem::Thread(thread) = &timeline[0] else {
            panic!("expected a thread")
        };
        assert_eq!(thread.resolved_by, Some(resolver));
        assert_eq!(thread.resolved_at, Some(at(7)));
    }

    #[test]
    fn a_reopened_thread_has_no_resolver_even_if_the_flag_is_stale() {
        let root = inline_root(Uuid::new_v4(), at(1), true);
        let events = vec![
            event(
                MergeRequestEventKind::ThreadResolved,
                at(5),
                Some(Uuid::new_v4()),
                json!({"commentId": root.id}),
            ),
            event(
                MergeRequestEventKind::ThreadReopened,
                at(6),
                Some(Uuid::new_v4()),
                json!({"commentId": root.id}),
            ),
        ];
        let timeline = assemble_timeline(vec![root], events, None);
        let TimelineItem::Thread(thread) = &timeline[0] else {
            panic!("expected a thread")
        };
        assert_eq!((thread.resolved_by, thread.resolved_at), (None, None));
    }

    #[test]
    fn an_unresolved_root_ignores_resolution_events() {
        let root = inline_root(Uuid::new_v4(), at(1), false);
        let events = vec![event(
            MergeRequestEventKind::ThreadResolved,
            at(5),
            Some(Uuid::new_v4()),
            json!({"commentId": root.id}),
        )];
        let timeline = assemble_timeline(vec![root], events, None);
        let TimelineItem::Thread(thread) = &timeline[0] else {
            panic!("expected a thread")
        };
        assert_eq!((thread.resolved_by, thread.resolved_at), (None, None));
    }

    #[test]
    fn a_resolved_root_with_no_event_has_no_resolver() {
        let timeline =
            assemble_timeline(vec![inline_root(Uuid::new_v4(), at(1), true)], vec![], None);
        let TimelineItem::Thread(thread) = &timeline[0] else {
            panic!("expected a thread")
        };
        assert_eq!((thread.resolved_by, thread.resolved_at), (None, None));
    }

    #[test]
    fn other_events_become_event_items_in_chronological_position() {
        let labels = event(
            MergeRequestEventKind::LabelsChanged,
            at(5),
            None,
            json!({"added": [], "removed": []}),
        );
        let merged = event(
            MergeRequestEventKind::Merged,
            at(20),
            None,
            json!({"mergeCommitSha": null}),
        );
        let timeline = assemble_timeline(
            vec![general(Uuid::new_v4(), at(10))],
            vec![merged.clone(), labels.clone()],
            None,
        );
        assert_eq!(timeline.len(), 3);
        assert_eq!(ids(&timeline)[0], labels.id);
        assert_eq!(ids(&timeline)[2], merged.id);
        assert!(matches!(timeline[0], TimelineItem::Event(_)));
    }

    #[test]
    fn without_diffs_a_thread_is_current_and_has_no_excerpt() {
        let timeline = assemble_timeline(
            vec![inline_root(Uuid::new_v4(), at(1), false)],
            vec![],
            None,
        );
        let TimelineItem::Thread(thread) = &timeline[0] else {
            panic!("expected a thread")
        };
        assert!(!thread.outdated);
        assert!(thread.excerpt.is_empty());
    }

    #[test]
    fn with_diffs_a_thread_gets_an_excerpt_and_an_outdated_flag() {
        let diffs = sample_diffs();
        let current = inline_root(Uuid::from_u128(1), at(1), false);
        let mut stale = inline_root(Uuid::from_u128(2), at(2), false);
        stale.anchor_content = Some("something else\n".to_string());
        let timeline = assemble_timeline(vec![current, stale], vec![], Some(&diffs));
        let TimelineItem::Thread(current) = &timeline[0] else {
            panic!("expected a thread")
        };
        let TimelineItem::Thread(stale) = &timeline[1] else {
            panic!("expected a thread")
        };
        assert!(!current.outdated);
        assert!(stale.outdated);
        assert_eq!(current.excerpt.len(), 3);
    }

    #[test]
    fn an_outdated_thread_has_no_excerpt_because_the_line_now_holds_other_code() {
        let diffs = sample_diffs();
        let mut stale = inline_root(Uuid::new_v4(), at(1), false);
        stale.anchor_content = Some("something else\n".to_string());
        let timeline = assemble_timeline(vec![stale], vec![], Some(&diffs));
        let TimelineItem::Thread(stale) = &timeline[0] else {
            panic!("expected a thread")
        };
        assert!(stale.outdated);
        assert!(
            stale.excerpt.is_empty(),
            "an outdated anchor must not show whatever code sits at that line number now"
        );
    }

    #[test]
    fn a_range_thread_takes_its_excerpt_from_the_end_line() {
        let diffs = sample_diffs();
        let mut root = inline_root(Uuid::new_v4(), at(1), false);
        root.line_number = Some(1);
        root.end_line = Some(3);
        root.anchor_content = Some("one\nanchor\nfour\n".to_string());
        let timeline = assemble_timeline(vec![root], vec![], Some(&diffs));
        let TimelineItem::Thread(thread) = &timeline[0] else {
            panic!("expected a thread")
        };
        assert!(!thread.outdated);
        assert_eq!(thread.excerpt.last().unwrap().line, Some(3));
    }

    #[test]
    fn is_outdated_mirrors_the_anchor_comparison() {
        let diffs = sample_diffs();
        let current = inline_root(Uuid::new_v4(), at(1), false);
        assert!(!is_outdated(&diffs, &current));
        let mut moved = current.clone();
        moved.line_number = Some(99);
        assert!(is_outdated(&diffs, &moved));
        let mut edited = current;
        edited.anchor_content = Some("old text\n".to_string());
        assert!(is_outdated(&diffs, &edited));
        assert!(!is_outdated(&diffs, &general(Uuid::new_v4(), at(1))));
    }

    #[test]
    fn excerpt_for_returns_the_anchor_and_up_to_two_lines_before_it() {
        let excerpt = excerpt_for(&sample_diffs(), "a.rs", 2, DiffSide::New);
        assert_eq!(
            excerpt,
            vec![
                ExcerptLine {
                    line: Some(1),
                    kind: DiffLineKind::Context,
                    content: "one\n".to_string()
                },
                ExcerptLine {
                    line: None,
                    kind: DiffLineKind::Removed,
                    content: "two\n".to_string()
                },
                ExcerptLine {
                    line: Some(2),
                    kind: DiffLineKind::Added,
                    content: "anchor\n".to_string()
                },
            ]
        );
    }

    #[test]
    fn excerpt_for_is_shorter_near_the_start_of_the_diff_and_uses_the_old_side_numbers() {
        let first = excerpt_for(&sample_diffs(), "a.rs", 1, DiffSide::New);
        assert_eq!(first.len(), 1);
        let old = excerpt_for(&sample_diffs(), "a.rs", 2, DiffSide::Old);
        assert_eq!(old.len(), 2);
        assert_eq!(old.last().unwrap().line, Some(2));
        assert_eq!(old.last().unwrap().kind, DiffLineKind::Removed);
    }

    #[test]
    fn excerpt_for_is_empty_for_an_unknown_path_or_line() {
        assert!(excerpt_for(&sample_diffs(), "missing.rs", 2, DiffSide::New).is_empty());
        assert!(excerpt_for(&sample_diffs(), "a.rs", 99, DiffSide::New).is_empty());
    }
}
