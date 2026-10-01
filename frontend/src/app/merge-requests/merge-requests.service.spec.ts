import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { MergeRequestsService, MergeRequestSummary } from './merge-requests.service';

describe('MergeRequestsService', () => {
  beforeEach(() => {
    TestBed.configureTestingModule({ providers: [MergeRequestsService, provideHttpClient(), provideHttpClientTesting()] });
  });

  it('lists branches for a repository', () => {
    const service = TestBed.inject(MergeRequestsService);
    const http = TestBed.inject(HttpTestingController);

    let result: unknown;
    service.listBranches('repo-1').subscribe((branches) => (result = branches));
    http.expectOne('/api/repositories/repo-1/branches').flush([{ name: 'main', tipSha: 'abc', isDefault: true }]);

    expect(result).toEqual([{ name: 'main', tipSha: 'abc', isDefault: true }]);
  });

  it('creates a merge request with the given fields', () => {
    const service = TestBed.inject(MergeRequestsService);
    const http = TestBed.inject(HttpTestingController);

    service.create('repo-1', { sourceBranch: 'feature', targetBranch: 'main', title: 'Add feature', description: 'desc' }).subscribe();
    const req = http.expectOne('/api/repositories/repo-1/merge-requests');
    expect(req.request.method).toBe('POST');
    expect(req.request.body).toEqual({ sourceBranch: 'feature', targetBranch: 'main', title: 'Add feature', description: 'desc' });
    req.flush({ id: '1', sourceBranch: 'feature', targetBranch: 'main', title: 'Add feature', description: 'desc', status: 'open', mergeCommitSha: null, createdAt: '2026-01-01T00:00:00Z', closedAt: null });
  });

  it('posts a merge attempt with no body', () => {
    const service = TestBed.inject(MergeRequestsService);
    const http = TestBed.inject(HttpTestingController);

    service.merge('1').subscribe();
    const req = http.expectOne('/api/merge-requests/1/merge');
    expect(req.request.method).toBe('POST');
    req.flush({ status: 'conflicting' });
  });

  it('lists merge requests for a repository with no filters', () => {
    const service = TestBed.inject(MergeRequestsService);
    const http = TestBed.inject(HttpTestingController);

    service.listForRepository('repo-1').subscribe();
    const req = http.expectOne((r) => r.url === '/api/repositories/repo-1/merge-requests' && r.method === 'GET');
    expect(req.request.params.keys().length).toBe(0);
    req.flush([]);
  });

  it('lists merge requests filtered by labelIds and milestoneId', () => {
    const service = TestBed.inject(MergeRequestsService);
    const http = TestBed.inject(HttpTestingController);

    service.listForRepository('repo-1', { labelIds: ['label-1'], milestoneId: 'milestone-1' }).subscribe();
    const req = http.expectOne((r) => r.url === '/api/repositories/repo-1/merge-requests' && r.method === 'GET');
    expect(req.request.params.get('labelIds')).toBe('label-1');
    expect(req.request.params.get('milestoneId')).toBe('milestone-1');
    req.flush([]);
  });

  it('updates a merge request including its milestone', () => {
    const service = TestBed.inject(MergeRequestsService);
    const http = TestBed.inject(HttpTestingController);

    service.update('mr-1', 'New title', 'New description', 'milestone-1').subscribe();
    const req = http.expectOne({ url: '/api/merge-requests/mr-1', method: 'PATCH' });
    expect(req.request.body).toEqual({ title: 'New title', description: 'New description', milestoneId: 'milestone-1' });
    req.flush({});
  });

  it('fetches the merge request timeline', () => {
    const service = TestBed.inject(MergeRequestsService);
    const http = TestBed.inject(HttpTestingController);

    let result: unknown;
    service.timeline('mr-1').subscribe((timeline) => (result = timeline));
    const req = http.expectOne('/api/merge-requests/mr-1/timeline');
    expect(req.request.method).toBe('GET');
    const body = { author: { id: 'u1', username: 'alice' }, items: [] };
    req.flush(body);

    expect(result).toEqual(body);
  });

  it('lists merge requests with their resolved author and comment count', () => {
    const service = TestBed.inject(MergeRequestsService);
    const http = TestBed.inject(HttpTestingController);
    const mergeRequests: MergeRequestSummary[] = [
      {
        id: 'mr-1',
        sourceBranch: 'feature/pagination',
        targetBranch: 'main',
        title: 'Paginer la liste des tickets',
        description: '',
        status: 'open',
        mergeCommitSha: null,
        createdAt: '2026-09-20T08:00:00Z',
        closedAt: null,
        milestoneId: null,
        labels: [],
        author: { id: 'u1', username: 'alice' },
        commentCount: 7,
      },
      {
        id: 'mr-2',
        sourceBranch: 'fix/typo',
        targetBranch: 'main',
        title: 'Corriger une coquille',
        description: '',
        status: 'merged',
        mergeCommitSha: 'abcdef1',
        createdAt: '2026-09-18T08:00:00Z',
        closedAt: '2026-09-19T08:00:00Z',
        milestoneId: null,
        labels: [],
        author: null,
        commentCount: 0,
      },
    ];

    let result: MergeRequestSummary[] | undefined;
    service.listForRepository('repo-1').subscribe((value) => (result = value));
    http.expectOne((r) => r.url === '/api/repositories/repo-1/merge-requests').flush(mergeRequests);

    expect(result?.map((mr) => [mr.author?.username ?? null, mr.commentCount])).toEqual([
      ['alice', 7],
      [null, 0],
    ]);
  });
});
