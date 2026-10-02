// Data builders shared by the merge request specs and stories.
import { Label } from '../labels/labels.service';
import { Milestone } from '../milestones/milestones.service';
import { RepositoryRole } from '../repositories/repositories.service';
import { Comment, MergeRequestSummary, TimelineEvent } from './merge-requests.service';

export const ALICE = { id: 'u1', username: 'alice' };
export const BOB = { id: 'u2', username: 'bob' };
export const CAROL = { id: 'u3', username: 'carol' };

export const label = (id: string, name: string, color: string): Label => ({ id, name, color, repositoryId: 'repo-1', groupId: null, createdAt: '2026-01-01T00:00:00Z' });

export const milestone = (fields: Pick<Milestone, 'id' | 'title'> & Partial<Milestone>): Milestone => ({
  description: '',
  dueDate: null,
  state: 'open',
  repositoryId: 'repo-1',
  groupId: null,
  createdAt: '2026-01-01T00:00:00Z',
  ...fields,
});

let nextMergeRequestId = 0;

export function mergeRequestFixture(fields: Pick<MergeRequestSummary, 'title' | 'sourceBranch'> & Partial<MergeRequestSummary>): MergeRequestSummary {
  return {
    id: `mr-${++nextMergeRequestId}`,
    targetBranch: 'main',
    description: '',
    status: 'open',
    mergeCommitSha: fields.status === 'merged' ? 'abc1234' : null,
    createdAt: '2026-01-01T00:00:00Z',
    closedAt: null,
    milestoneId: null,
    labels: [],
    author: ALICE,
    commentCount: 0,
    ...fields,
  };
}

/** A general comment; pass filePath, lineNumber and side for an inline one. */
export function commentFixture(fields: Pick<Comment, 'id' | 'author' | 'body' | 'createdAt'> & Partial<Comment>): Comment {
  return {
    authorId: fields.author?.id ?? 'gone',
    replyToId: null,
    filePath: null,
    lineNumber: null,
    endLine: null,
    side: null,
    outdated: false,
    resolved: false,
    suggestedContent: null,
    appliedAt: null,
    appliedCommitSha: null,
    ...fields,
  };
}

export function eventFixture(fields: Pick<TimelineEvent, 'id' | 'createdAt' | 'kind'> & Partial<TimelineEvent>): TimelineEvent {
  return { type: 'event', actor: ALICE, payload: {}, ...fields };
}

export function fakeRepositoryContext(role: RepositoryRole | null, path: string[]) {
  return { current: () => ({ repositoryId: 'repo-1', path, role, ancestors: [], groupId: null }) };
}
