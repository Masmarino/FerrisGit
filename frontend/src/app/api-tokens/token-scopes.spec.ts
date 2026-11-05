import { describeScope, SCOPE_AREAS, ScopeArea, ScopeChoice, scopesFromChoices } from './token-scopes';

const NONE: Record<ScopeArea, ScopeChoice> = { repositories: 'none', issues: 'none', 'merge-requests': 'none', pipelines: 'none', releases: 'none', admin: 'none' };

describe('scopesFromChoices', () => {
  it('sends one scope per chosen area, as the server spells them, and nothing for "none"', () => {
    expect(scopesFromChoices(SCOPE_AREAS, { ...NONE, issues: 'read', 'merge-requests': 'write' })).toEqual(['issues:read', 'merge-requests:write']);
  });

  it('leaves out an area the form does not offer, whatever was chosen for it', () => {
    expect(scopesFromChoices(['issues'], { ...NONE, issues: 'read', admin: 'write' })).toEqual(['issues:read']);
  });

  it('gives nothing when nothing is chosen', () => {
    expect(scopesFromChoices(SCOPE_AREAS, NONE)).toEqual([]);
  });
});

describe('describeScope', () => {
  it('says what a read scope and a write scope allow, in French', () => {
    expect(describeScope('issues:read').label).toBe('Tickets : lecture');
    expect(describeScope('merge-requests:write').label).toBe('Demandes de fusion : lecture et écriture');
    expect(describeScope('repositories:read').hint).toContain('Git');
  });

  it.each(['deploy:write', 'issues:admin', 'issues', '__proto__:read'])('keeps %s as written rather than hiding it', (scope) => {
    expect(describeScope(scope)).toEqual({ label: scope, hint: null });
  });
});
