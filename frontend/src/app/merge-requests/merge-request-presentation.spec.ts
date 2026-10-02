import { authorName, mergeRequestEnd } from './merge-request-presentation';
import { MergeRequestSummary } from './merge-requests.service';

function mergeRequest(fields: Partial<MergeRequestSummary>): MergeRequestSummary {
  return {
    id: 'mr-1',
    sourceBranch: 'feature',
    targetBranch: 'main',
    title: 'Add a feature',
    description: '',
    status: 'open',
    mergeCommitSha: null,
    createdAt: '2026-01-01T00:00:00Z',
    closedAt: null,
    milestoneId: null,
    labels: [],
    author: null,
    commentCount: 0,
    ...fields,
  };
}

describe('authorName', () => {
  it('names the author, or a deleted account', () => {
    expect(authorName({ id: 'u1', username: 'alice' })).toBe('alice');
    expect(authorName(null)).toBe('Utilisateur supprimé');
  });
});

describe('mergeRequestEnd', () => {
  it('is null while the merge request is open', () => {
    expect(mergeRequestEnd(mergeRequest({}))).toBeNull();
    expect(mergeRequestEnd(mergeRequest({ closedAt: '2026-01-02T00:00:00Z' }))).toBeNull();
  });

  it('says how and when it ended', () => {
    expect(mergeRequestEnd(mergeRequest({ status: 'merged', closedAt: '2026-01-02T00:00:00Z' }))).toEqual({ verb: 'fusionnée', at: '2026-01-02T00:00:00Z' });
    expect(mergeRequestEnd(mergeRequest({ status: 'closed', closedAt: '2026-01-03T00:00:00Z' }))).toEqual({ verb: 'fermée', at: '2026-01-03T00:00:00Z' });
  });

  it('is null for a finished merge request without a closing date', () => {
    expect(mergeRequestEnd(mergeRequest({ status: 'closed' }))).toBeNull();
  });
});
