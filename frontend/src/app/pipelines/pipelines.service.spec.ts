import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { PipelineDetail, PipelinesService, PipelineSummary } from './pipelines.service';

describe('PipelinesService', () => {
  function setup() {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting(), PipelinesService] });
    return { service: TestBed.inject(PipelinesService), http: TestBed.inject(HttpTestingController) };
  }

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  it('lists pipelines for a repository, with who triggered them and their commit message', () => {
    const { service, http } = setup();
    let result: PipelineSummary[] | undefined;
    service.listForRepository('repo-1').subscribe((p) => (result = p));
    const pipelines: PipelineSummary[] = [
      {
        id: 'pipeline-1',
        commitSha: 'deadbeef',
        status: 'success',
        createdAt: '2026-01-01T00:00:00Z',
        finishedAt: '2026-01-01T00:04:10Z',
        triggeredBy: { id: 'user-1', username: 'alice' },
        commitMessage: 'Paginer la liste des tickets',
        error: null,
      },
      { id: 'pipeline-0', commitSha: 'cafebabe', status: 'running', createdAt: '2025-01-01T00:00:00Z', finishedAt: null, triggeredBy: null, commitMessage: null, error: null },
    ];
    http.expectOne({ url: '/api/repositories/repo-1/pipelines', method: 'GET' }).flush(pipelines);
    expect(result).toEqual(pipelines);
    expect(result?.[0].triggeredBy?.username).toBe('alice');
    expect(result?.[1].commitMessage).toBeNull();
  });

  it('fetches pipeline detail including jobs', () => {
    const { service, http } = setup();
    let result: PipelineDetail | undefined;
    service.detail('pipeline-1').subscribe((p) => (result = p));
    const detail: PipelineDetail = {
      id: 'pipeline-1',
      commitSha: 'deadbeef',
      status: 'running',
      createdAt: '2026-01-01T00:00:00Z',
      finishedAt: null,
      triggeredBy: { id: 'user-1', username: 'alice' },
      commitMessage: 'Paginer la liste des tickets',
      error: null,
      jobs: [
        {
          id: 'job-1',
          stage: 'build',
          name: 'build',
          status: 'running',
          needs: [],
          tags: [],
          logs: '',
          createdAt: '2026-01-01T00:00:00Z',
          startedAt: '2026-01-01T00:00:05Z',
          finishedAt: null,
        },
      ],
    };
    http.expectOne({ url: '/api/pipelines/pipeline-1', method: 'GET' }).flush(detail);
    expect(result).toEqual(detail);
  });

  it('cancels a pipeline', () => {
    const { service, http } = setup();
    service.cancel('pipeline-1').subscribe();
    const req = http.expectOne({ url: '/api/pipelines/pipeline-1/cancel', method: 'POST' });
    expect(req.request.body).toEqual({});
    req.flush(null);
  });
});
