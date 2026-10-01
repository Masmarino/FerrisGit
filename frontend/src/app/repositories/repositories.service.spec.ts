import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { RepositoriesService } from './repositories.service';

describe('RepositoriesService', () => {
  function setup() {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting()] });
    return { service: TestBed.inject(RepositoriesService), http: TestBed.inject(HttpTestingController) };
  }

  afterEach(() => {
    TestBed.inject(HttpTestingController)?.verify();
  });

  it('treeAt requests the root tree when path is empty', () => {
    const { service, http } = setup();
    service.treeAt('repo-1', 'main', []).subscribe();
    http.expectOne('/api/repositories/by-id/repo-1/tree/main').flush([]);
  });

  it('treeAt requests a nested path, URL-encoding each segment', () => {
    const { service, http } = setup();
    service.treeAt('repo-1', 'main', ['src', 'a b']).subscribe();
    http.expectOne('/api/repositories/by-id/repo-1/tree/main/src/a%20b').flush([]);
  });

  it('treeAt asks for each entry’s last commit by default (no lastCommit param)', () => {
    const { service, http } = setup();
    service.treeAt('repo-1', 'main', ['src']).subscribe();
    const req = http.expectOne((r) => r.url === '/api/repositories/by-id/repo-1/tree/main/src');
    expect(req.request.params.has('lastCommit')).toBe(false);
    req.flush([]);
  });

  it('treeAt sends lastCommit=false for a names-only listing', () => {
    const { service, http } = setup();
    service.treeAt('repo-1', 'main', [], { lastCommit: false }).subscribe();
    const req = http.expectOne((r) => r.url === '/api/repositories/by-id/repo-1/tree/main');
    expect(req.request.params.get('lastCommit')).toBe('false');
    req.flush([]);
  });

  it('treeAt leaves lastCommit out when it is explicitly true', () => {
    const { service, http } = setup();
    service.treeAt('repo-1', 'main', [], { lastCommit: true }).subscribe();
    const req = http.expectOne((r) => r.url === '/api/repositories/by-id/repo-1/tree/main');
    expect(req.request.params.has('lastCommit')).toBe(false);
    req.flush([]);
  });

  it('blobAt requests the blob endpoint with the full path', () => {
    const { service, http } = setup();
    service.blobAt('repo-1', 'main', ['src', 'main.rs']).subscribe();
    http.expectOne('/api/repositories/by-id/repo-1/blob/main/src/main.rs').flush({ sha: 'x', size: 1, isBinary: false, content: 'a' });
  });

  it('readmeAt requests the readme endpoint for the given ref', () => {
    const { service, http } = setup();
    service.readmeAt('repo-1', 'main').subscribe();
    http.expectOne('/api/repositories/by-id/repo-1/readme/main').flush({ content: null });
  });

  it('commitsById requests the commits endpoint without params when no ref is given', () => {
    const { service, http } = setup();
    service.commitsById('r1').subscribe();
    const req = http.expectOne((r) => r.url === '/api/repositories/by-id/r1/commits');
    expect(req.request.params.has('ref')).toBe(false);
    req.flush([]);
  });

  it('commitsById sends the ref as a query parameter', () => {
    const { service, http } = setup();
    service.commitsById('r1', 'feature/x').subscribe();
    const req = http.expectOne((r) => r.url === '/api/repositories/by-id/r1/commits');
    expect(req.request.params.get('ref')).toBe('feature/x');
    req.flush([]);
  });

  it('lists repositories from the API', () => {
    const { service, http } = setup();

    let result: unknown;
    service.list().subscribe((repos) => (result = repos));
    http.expectOne('/api/repositories').flush([{ id: '1', name: 'hello', visibility: 'private', createdAt: '2026-01-01T00:00:00Z' }]);

    expect(result).toEqual([{ id: '1', name: 'hello', visibility: 'private', createdAt: '2026-01-01T00:00:00Z' }]);
  });

  it('returns both owned and collaborated-on repositories with their distinct roles', () => {
    const { service, http } = setup();

    let result: unknown;
    service.list().subscribe((repos) => (result = repos));
    http.expectOne('/api/repositories').flush([
      { id: '1', name: 'mine', owner: 'alice', role: 'owner', visibility: 'private', createdAt: '2026-01-01T00:00:00Z' },
      { id: '2', name: 'shared', owner: 'bob', role: 'contributor', visibility: 'private', createdAt: '2026-01-02T00:00:00Z' },
    ]);

    expect(result).toEqual([
      { id: '1', name: 'mine', owner: 'alice', role: 'owner', visibility: 'private', createdAt: '2026-01-01T00:00:00Z' },
      { id: '2', name: 'shared', owner: 'bob', role: 'contributor', visibility: 'private', createdAt: '2026-01-02T00:00:00Z' },
    ]);
  });

  it('star posts to the star endpoint and returns the fresh count', () => {
    const { service, http } = setup();
    service.star('repo-1').subscribe();
    http.expectOne({ url: '/api/repositories/by-id/repo-1/star', method: 'POST' }).flush({ starCount: 1, isStarred: true });
  });

  it('unstar deletes the star endpoint and returns the fresh count', () => {
    const { service, http } = setup();
    service.unstar('repo-1').subscribe();
    http.expectOne({ url: '/api/repositories/by-id/repo-1/star', method: 'DELETE' }).flush({ starCount: 0, isStarred: false });
  });

  it('listContributors requests the contributors endpoint for the given ref', () => {
    const { service, http } = setup();
    service.listContributors('repo-1', 'main').subscribe();
    http.expectOne('/api/repositories/by-id/repo-1/contributors/main').flush([]);
  });

  it('getLanguages requests the languages endpoint for the given ref', () => {
    const { service, http } = setup();
    service.getLanguages('repo-1', 'main').subscribe();
    http.expectOne('/api/repositories/by-id/repo-1/languages/main').flush({ languages: [] });
  });
});
