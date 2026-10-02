import { shortSha } from '../repositories/commit-format';
import { TimelineEvent } from './merge-requests.service';

export interface LabelRef {
  name: string;
  color?: string;
}

/** One piece of an event sentence, so the system note can render values distinctly while `describeEvent` produces the same plain sentence from them. */
export type EventSegment = { kind: 'text'; text: string } | { kind: 'labels'; labels: LabelRef[] } | { kind: 'quote'; text: string } | { kind: 'sha'; sha: string };

export type EventTone = 'neutral' | 'success' | 'error';

const text = (value: string): EventSegment => ({ kind: 'text', text: value });
const quote = (value: unknown): EventSegment => ({ kind: 'quote', text: String(value) });

function labelSegments(verb: string, labels: LabelRef[]): EventSegment[] {
  const noun = labels.length === 1 ? 'le label' : 'les labels';
  return [text(`${verb} ${noun}`), { kind: 'labels', labels }];
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
        return [text(hasActor ? 'a approuvé cette demande de fusion' : 'Revue : approuvée')];
      }
      return [text(hasActor ? 'a demandé des changements' : 'Revue : changements demandés')];
    case 'labels_changed': {
      const added = (payload['added'] as LabelRef[] | undefined) ?? [];
      const removed = (payload['removed'] as LabelRef[] | undefined) ?? [];
      const segments: EventSegment[] = [];
      if (added.length) {
        segments.push(...labelSegments('a ajouté', added));
      }
      if (removed.length) {
        segments.push(...labelSegments(added.length ? 'et retiré' : 'a retiré', removed));
      }
      return segments;
    }
    case 'milestone_changed': {
      const from = payload['from'] as string | null;
      const to = payload['to'] as string | null;
      if (from === null || from === undefined) {
        return [text('a défini le milestone'), quote(to)];
      }
      if (to === null || to === undefined) {
        return [text('a retiré le milestone'), quote(from)];
      }
      return [text('a changé le milestone de'), quote(from), text('à'), quote(to)];
    }
    case 'title_changed':
      return [text('a renommé la demande de'), quote(payload['from']), text('en'), quote(payload['to'])];
    case 'commits_pushed':
      return [text('a poussé de nouveaux commits'), ...shaSegments(payload['toSha'])];
    case 'merged':
      return [text(hasActor ? 'a fusionné cette demande de fusion' : 'Fusionnée'), ...shaSegments(payload['mergeCommitSha'])];
    case 'closed':
      return [text(hasActor ? 'a fermé cette demande de fusion' : 'Fermée')];
    default:
      return [text('a effectué une action')];
  }
}

function segmentText(segment: EventSegment): string {
  switch (segment.kind) {
    case 'text':
      return segment.text;
    case 'labels':
      return segment.labels.map((label) => label.name).join(', ');
    case 'quote':
      return `« ${segment.text} »`;
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
