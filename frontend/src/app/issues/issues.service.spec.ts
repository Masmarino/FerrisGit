import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { Issue, IssueComment, IssuesService } from './issues.service';

describe('IssuesService', () => {
  function setup() {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting(), IssuesService] });
    return { service: TestBed.inject(IssuesService), http: TestBed.inject(HttpTestingController) };
  }

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  it('lists issues for a repository with no filters', () => {
    const { service, http } = setup();
    service.list('repo-1').subscribe();
    const req = http.expectOne((r) => r.url === '/api/repositories/repo-1/issues' && r.method === 'GET');
    expect(req.request.params.keys().length).toBe(0);
    req.flush([]);
  });

  it('lists issues filtered by labelIds and milestoneId', () => {
    const { service, http } = setup();
    service.list('repo-1', { labelIds: ['label-1', 'label-2'], milestoneId: 'milestone-1' }).subscribe();
    const req = http.expectOne((r) => r.url === '/api/repositories/repo-1/issues' && r.method === 'GET');
    expect(req.request.params.get('labelIds')).toBe('label-1,label-2');
    expect(req.request.params.get('milestoneId')).toBe('milestone-1');
    req.flush([]);
  });

  it('updates an issue including its milestone', () => {
    const { service, http } = setup();
    service.update('repo-1', 3, 'New title', 'New description', 'bug', 'milestone-1').subscribe();
    const req = http.expectOne({ url: '/api/repositories/repo-1/issues/3', method: 'PATCH' });
    expect(req.request.body).toEqual({ title: 'New title', description: 'New description', kind: 'bug', milestoneId: 'milestone-1' });
    req.flush({
      id: 'i1',
      number: 3,
      authorId: 'u1',
      assigneeId: null,
      title: 'New title',
      description: 'New description',
      status: 'todo',
      kind: 'bug',
      parentIssueId: null,
      createdAt: '2026-01-01T00:00:00Z',
      closedAt: null,
      milestoneId: 'milestone-1',
      labels: [],
      author: { id: 'u1', username: 'alice' },
      assignee: null,
      commentCount: 0,
    });
  });

  it('clears an issue’s milestone by sending null', () => {
    const { service, http } = setup();
    service.update('repo-1', 3, 'Title', 'Description', 'bug', null).subscribe();
    const req = http.expectOne({ url: '/api/repositories/repo-1/issues/3', method: 'PATCH' });
    expect(req.request.body['milestoneId']).toBeNull();
    req.flush({});
  });

  it('returns the issue with its resolved author, assignee and comment count', () => {
    const { service, http } = setup();
    const issue: Issue = {
      id: 'i1',
      number: 12,
      authorId: 'u1',
      assigneeId: 'u2',
      title: 'Corriger la pagination',
      description: '',
      status: 'in_progress',
      kind: 'bug',
      parentIssueId: null,
      createdAt: '2026-09-20T08:00:00Z',
      closedAt: null,
      milestoneId: null,
      labels: [],
      author: { id: 'u1', username: 'alice' },
      assignee: { id: 'u2', username: 'bob' },
      commentCount: 4,
    };

    let result: Issue | undefined;
    service.detail('repo-1', 12).subscribe((value) => (result = value));
    const req = http.expectOne({ url: '/api/repositories/repo-1/issues/12', method: 'GET' });
    req.flush(issue);

    expect(result?.author?.username).toBe('alice');
    expect(result?.assignee?.username).toBe('bob');
    expect(result?.commentCount).toBe(4);
  });

  it('lists comments with their resolved author (null when the user no longer resolves)', () => {
    const { service, http } = setup();
    const comments: IssueComment[] = [
      { id: 'c1', authorId: 'u1', author: { id: 'u1', username: 'alice' }, body: 'Je regarde', createdAt: '2026-09-20T09:00:00Z' },
      { id: 'c2', authorId: 'u9', author: null, body: 'Ancien compte', createdAt: '2026-09-20T10:00:00Z' },
    ];

    let result: IssueComment[] | undefined;
    service.listComments('repo-1', 12).subscribe((value) => (result = value));
    http.expectOne({ url: '/api/repositories/repo-1/issues/12/comments', method: 'GET' }).flush(comments);

    expect(result?.map((comment) => comment.author?.username ?? null)).toEqual(['alice', null]);
  });
});
