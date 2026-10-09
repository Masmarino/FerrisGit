import { issueKindOptions, issueKindPresentation } from './issue-kind';

describe('issueKindPresentation', () => {
  it('names every issue kind in French with its icon', () => {
    expect(issueKindPresentation('bug')).toEqual({ label: 'Bug', icon: 'bug' });
    expect(issueKindPresentation('feature')).toEqual({ label: 'Fonctionnalité', icon: 'sparkles' });
    expect(issueKindPresentation('task')).toEqual({ label: 'Tâche', icon: 'square-check' });
    expect(issueKindPresentation('epic')).toEqual({ label: 'Epic', icon: 'layers' });
  });

  it('returns the same object for a known kind (safe to bind)', () => {
    expect(issueKindPresentation('bug')).toBe(issueKindPresentation('bug'));
  });

  it('shows an unknown kind as its raw text', () => {
    expect(issueKindPresentation('spike')).toEqual({ label: 'spike', icon: 'circle-dot' });
    expect(issueKindPresentation('toString').label).toBe('toString');
  });

  it('offers the three kinds a new issue can have, labelled like the rows', () => {
    expect(issueKindOptions()).toEqual([
      { value: 'bug', label: 'Bug' },
      { value: 'feature', label: 'Fonctionnalité' },
      { value: 'task', label: 'Tâche' },
    ]);
  });
});
