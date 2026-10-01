import { describeEvent, eventMarker, eventSegments, shortSha } from './merge-request-events';
import { MergeRequestEventKind, TimelineEvent, UserRef } from './merge-requests.service';

const alice: UserRef = { id: 'u1', username: 'alice' };

function event(kind: MergeRequestEventKind | string, payload: Record<string, unknown>, actor: UserRef | null = alice): TimelineEvent {
  return { type: 'event', id: 'e1', createdAt: '2026-01-01T00:00:00Z', actor, kind: kind as MergeRequestEventKind, payload };
}

const bug = { id: 'l1', name: 'bug', color: '#ff0000' };
const api = { id: 'l2', name: 'api', color: '#00ff00' };

describe('shortSha', () => {
  it('keeps the first 7 characters', () => {
    expect(shortSha('abcdef1234567')).toBe('abcdef1');
  });
});

describe('describeEvent', () => {
  it('describes an approval', () => {
    expect(describeEvent(event('review_submitted', { decision: 'approved' }))).toBe('a approuvé cette demande de fusion');
    expect(describeEvent(event('review_submitted', { decision: 'approved' }, null))).toBe('Revue : approuvée');
  });

  it('describes a request for changes', () => {
    expect(describeEvent(event('review_submitted', { decision: 'changes_requested' }))).toBe('a demandé des changements');
    expect(describeEvent(event('review_submitted', { decision: 'changes_requested' }, null))).toBe('Revue : changements demandés');
  });

  it('describes added labels', () => {
    expect(describeEvent(event('labels_changed', { added: [bug], removed: [] }))).toBe('a ajouté le label bug');
    expect(describeEvent(event('labels_changed', { added: [bug, api], removed: [] }))).toBe('a ajouté les labels bug, api');
  });

  it('describes removed labels', () => {
    expect(describeEvent(event('labels_changed', { added: [], removed: [bug] }))).toBe('a retiré le label bug');
  });

  it('describes added and removed labels, added first', () => {
    expect(describeEvent(event('labels_changed', { added: [api], removed: [bug] }))).toBe('a ajouté le label api et retiré le label bug');
  });

  it('describes a milestone being set', () => {
    expect(describeEvent(event('milestone_changed', { from: null, to: 'v1.0' }))).toBe('a défini le milestone « v1.0 »');
  });

  it('describes a milestone being removed', () => {
    expect(describeEvent(event('milestone_changed', { from: 'v1.0', to: null }))).toBe('a retiré le milestone « v1.0 »');
  });

  it('describes a milestone being changed', () => {
    expect(describeEvent(event('milestone_changed', { from: 'v1.0', to: 'v1.1' }))).toBe('a changé le milestone de « v1.0 » à « v1.1 »');
  });

  it('describes a rename', () => {
    expect(describeEvent(event('title_changed', { from: 'A', to: 'B' }))).toBe('a renommé la demande de « A » en « B »');
  });

  it('describes pushed commits with the short head sha', () => {
    expect(describeEvent(event('commits_pushed', { fromSha: '1111111222', toSha: 'abcdef1234567' }))).toBe('a poussé de nouveaux commits (abcdef1)');
  });

  it('describes a merge with the short merge commit sha', () => {
    expect(describeEvent(event('merged', { mergeCommitSha: 'abcdef1234567' }))).toBe('a fusionné cette demande de fusion (abcdef1)');
    expect(describeEvent(event('merged', { mergeCommitSha: 'abcdef1234567' }, null))).toBe('Fusionnée (abcdef1)');
  });

  it('omits the sha when a merge has none', () => {
    expect(describeEvent(event('merged', { mergeCommitSha: null }))).toBe('a fusionné cette demande de fusion');
    expect(describeEvent(event('merged', {}))).toBe('a fusionné cette demande de fusion');
    expect(describeEvent(event('merged', { mergeCommitSha: null }, null))).toBe('Fusionnée');
  });

  it('describes a close', () => {
    expect(describeEvent(event('closed', {}))).toBe('a fermé cette demande de fusion');
    expect(describeEvent(event('closed', {}, null))).toBe('Fermée');
  });

  it('falls back to a generic sentence for an unknown kind', () => {
    expect(describeEvent(event('something_new', {}))).toBe('a effectué une action');
  });
});

describe('eventSegments', () => {
  it('splits a label change into sentence text and label chips, added first', () => {
    expect(eventSegments(event('labels_changed', { added: [api], removed: [bug] }))).toEqual([
      { kind: 'text', text: 'a ajouté le label' },
      { kind: 'labels', labels: [api] },
      { kind: 'text', text: 'et retiré le label' },
      { kind: 'labels', labels: [bug] },
    ]);
  });

  it('isolates the milestone and title values as quoted segments', () => {
    expect(eventSegments(event('milestone_changed', { from: 'v1.0', to: 'v1.1' }))).toEqual([
      { kind: 'text', text: 'a changé le milestone de' },
      { kind: 'quote', text: 'v1.0' },
      { kind: 'text', text: 'à' },
      { kind: 'quote', text: 'v1.1' },
    ]);
    expect(eventSegments(event('title_changed', { from: 'A', to: 'B' }))).toEqual([
      { kind: 'text', text: 'a renommé la demande de' },
      { kind: 'quote', text: 'A' },
      { kind: 'text', text: 'en' },
      { kind: 'quote', text: 'B' },
    ]);
  });

  it('isolates the short sha of a push or a merge, and omits it when missing', () => {
    expect(eventSegments(event('commits_pushed', { toSha: 'abcdef1234567' }))).toEqual([
      { kind: 'text', text: 'a poussé de nouveaux commits' },
      { kind: 'sha', sha: 'abcdef1' },
    ]);
    expect(eventSegments(event('merged', { mergeCommitSha: 'abcdef1234567' }, null))).toEqual([
      { kind: 'text', text: 'Fusionnée' },
      { kind: 'sha', sha: 'abcdef1' },
    ]);
    expect(eventSegments(event('merged', {}))).toEqual([{ kind: 'text', text: 'a fusionné cette demande de fusion' }]);
  });
});

describe('eventMarker', () => {
  it('tints approvals and merges as success', () => {
    expect(eventMarker(event('review_submitted', { decision: 'approved' }))).toEqual({ icon: 'check', tone: 'success' });
    expect(eventMarker(event('merged', {}))).toEqual({ icon: 'git-merge', tone: 'success' });
  });

  it('tints a request for changes as error', () => {
    expect(eventMarker(event('review_submitted', { decision: 'changes_requested' }))).toEqual({ icon: 'alert-circle', tone: 'error' });
  });

  it('keeps every other kind neutral, with an icon per kind', () => {
    expect(eventMarker(event('labels_changed', {}))).toEqual({ icon: 'tag', tone: 'neutral' });
    expect(eventMarker(event('milestone_changed', {}))).toEqual({ icon: 'flag', tone: 'neutral' });
    expect(eventMarker(event('title_changed', {}))).toEqual({ icon: 'pencil', tone: 'neutral' });
    expect(eventMarker(event('commits_pushed', {}))).toEqual({ icon: 'git-commit', tone: 'neutral' });
    expect(eventMarker(event('closed', {}))).toEqual({ icon: 'x', tone: 'neutral' });
    expect(eventMarker(event('something_new', {}))).toEqual({ icon: 'circle-dot', tone: 'neutral' });
  });
});
