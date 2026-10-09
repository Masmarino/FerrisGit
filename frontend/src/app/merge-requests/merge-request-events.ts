import { shortSha } from '../repositories/commit-format';
import { TimelineEvent } from './merge-requests.service';
import { t, tn } from '../shared/i18n/translator';

export interface LabelRef {
  name: string;
  color?: string;
}

/** A piece of an event sentence: the system note styles values apart, `describeEvent` joins them into plain text. */
export type EventSegment = { kind: 'text'; text: string } | { kind: 'labels'; labels: LabelRef[] } | { kind: 'quote'; text: string } | { kind: 'sha'; sha: string };

export type EventTone = 'neutral' | 'success' | 'error';

const text = (value: string): EventSegment => ({ kind: 'text', text: value });
const quote = (value: unknown): EventSegment => ({ kind: 'quote', text: String(value) });

/** `key` is a plural pair: « a ajouté le label » for one, « a ajouté les labels » for several. */
function labelSegments(key: string, labels: LabelRef[]): EventSegment[] {
  return [text(tn(key, labels.length)), { kind: 'labels', labels }];
}

function shaSegments(sha: unknown): EventSegment[] {
  return typeof sha === 'string' && sha ? [{ kind: 'sha', sha: shortSha(sha) }] : [];
}

export function eventSegments(event: TimelineEvent): EventSegment[] {
  const payload = event.payload;
  const hasActor = event.actor !== null;

  switch (event.kind) {
    case 'review_submitted':
      if (payload['decision'] === 'approved') {
        return [text(hasActor ? t('mergeRequests.events.approved') : t('mergeRequests.events.approvedNoActor'))];
      }
      return [text(hasActor ? t('mergeRequests.events.changesRequested') : t('mergeRequests.events.changesRequestedNoActor'))];
    case 'labels_changed': {
      const added = (payload['added'] as LabelRef[] | undefined) ?? [];
      const removed = (payload['removed'] as LabelRef[] | undefined) ?? [];
      const segments: EventSegment[] = [];
      if (added.length) {
        segments.push(...labelSegments('mergeRequests.events.addedLabels', added));
      }
      if (removed.length) {
        segments.push(...labelSegments(added.length ? 'mergeRequests.events.andRemovedLabels' : 'mergeRequests.events.removedLabels', removed));
      }
      return segments;
    }
    case 'milestone_changed': {
      const from = payload['from'] as string | null;
      const to = payload['to'] as string | null;
      if (from === null || from === undefined) {
        return [text(t('mergeRequests.events.milestoneSet')), quote(to)];
      }
      if (to === null || to === undefined) {
        return [text(t('mergeRequests.events.milestoneRemoved')), quote(from)];
      }
      return [text(t('mergeRequests.events.milestoneChanged')), quote(from), text(t('mergeRequests.events.to')), quote(to)];
    }
    case 'title_changed':
      return [text(t('mergeRequests.events.renamed')), quote(payload['from']), text(t('mergeRequests.events.renamedTo')), quote(payload['to'])];
    case 'commits_pushed':
      return [text(t('mergeRequests.events.pushed')), ...shaSegments(payload['toSha'])];
    case 'merged':
      return [text(hasActor ? t('mergeRequests.events.merged') : t('mergeRequests.events.mergedNoActor')), ...shaSegments(payload['mergeCommitSha'])];
    case 'closed':
      return [text(hasActor ? t('mergeRequests.events.closed') : t('mergeRequests.events.closedNoActor'))];
    default:
      return [text(t('mergeRequests.events.other'))];
  }
}

function segmentText(segment: EventSegment): string {
  switch (segment.kind) {
    case 'text':
      return segment.text;
    case 'labels':
      return segment.labels.map((label) => label.name).join(', ');
    case 'quote':
      return t('common.quoted', { text: segment.text });
    case 'sha':
      return `(${segment.sha})`;
  }
}

export function describeEvent(event: TimelineEvent): string {
  return eventSegments(event).map(segmentText).join(' ');
}

export function eventMarker(event: TimelineEvent): { icon: string; tone: EventTone } {
  switch (event.kind) {
    case 'review_submitted':
      return event.payload['decision'] === 'approved' ? { icon: 'check', tone: 'success' } : { icon: 'alert-circle', tone: 'error' };
    case 'merged':
      return { icon: 'git-merge', tone: 'success' };
    case 'labels_changed':
      return { icon: 'tag', tone: 'neutral' };
    case 'milestone_changed':
      return { icon: 'flag', tone: 'neutral' };
    case 'title_changed':
      return { icon: 'pencil', tone: 'neutral' };
    case 'commits_pushed':
      return { icon: 'git-commit', tone: 'neutral' };
    case 'closed':
      return { icon: 'x', tone: 'neutral' };
    default:
      return { icon: 'circle-dot', tone: 'neutral' };
  }
}
