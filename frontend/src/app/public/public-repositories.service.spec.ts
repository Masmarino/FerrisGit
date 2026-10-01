import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { PublicBranchesService, PublicCatalogPage, PublicRepositoriesService } from './public-repositories.service';

describe('PublicRepositoriesService', () => {
  function setup() {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting()] });
    return { service: TestBed.inject(PublicRepositoriesService), branches: TestBed.inject(PublicBranchesService), http: TestBed.inject(HttpTestingController) };
  }

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  describe('search', () => {
    it('asks the catalog for a page of 20, sorted, with the text', () => {
      const { service, http } = setup();
      const page: PublicCatalogPage = { items: [], total: 0, page: 2, perPage: 20 };
      let result: PublicCatalogPage | undefined;

      service.search({ q: 'rust', sort: 'name', page: 2 }).subscribe((r) => (result = r));

      const req = http.expectOne((r) => r.url === '/api/public/repositories');
      expect(req.request.method).toBe('GET');
      expect(req.request.params.get('q')).toBe('rust');
      expect(req.request.params.get('sort')).toBe('name');
      expect(req.request.params.get('page')).toBe('2');
      expect(req.request.params.get('perPage')).toBe('20');
      req.flush(page);
      expect(result).toEqual(page);
    });

    it('leaves q out when there is no text', () => {
      const { service, http } = setup();
      service.search({ q: '', sort: 'stars', page: 1 }).subscribe();
      const req = http.expectOne((r) => r.url === '/api/public/repositories');
      expect(req.request.params.has('q')).toBe(false);
      req.flush({ items: [], total: 0, page: 1, perPage: 20 });
    });
  });

  it('resolves a path under /api/public/resolve, URL-encoding each segment', () => {
    const { service, http } = setup();
    service.resolve(['acme', 'a b']).subscribe();
    http.expectOne('/api/public/resolve/acme/a%20b').flush({ type: 'personalRepository', repositoryId: 'repo-1' });
  });

  it('reads a repository and its content under /api/public/repositories/by-id', () => {
    const { service, http } = setup();

    service.getById('repo-1').subscribe();
    service.treeAt('repo-1', 'main', []).subscribe();
    service.treeAt('repo-1', 'v1/rc', ['src', 'a b'], { lastCommit: false }).subscribe();
    service.blobAt('repo-1', 'main', ['src', 'main.rs']).subscribe();
    service.readmeAt('repo-1', 'HEAD').subscribe();
    service.listContributors('repo-1', 'main').subscribe();
    service.getLanguages('repo-1', 'main').subscribe();

    http.expectOne('/api/public/repositories/by-id/repo-1').flush({});
    http.expectOne('/api/public/repositories/by-id/repo-1/tree/main').flush([]);
    const namesOnly = http.expectOne((r) => r.url === '/api/public/repositories/by-id/repo-1/tree/v1%2Frc/src/a%20b');
    expect(namesOnly.request.params.get('lastCommit')).toBe('false');
    namesOnly.flush([]);
    http.expectOne('/api/public/repositories/by-id/repo-1/blob/main/src/main.rs').flush({ sha: 'x', size: 1, isBinary: false, content: 'a' });
    http.expectOne('/api/public/repositories/by-id/repo-1/readme/HEAD').flush({ content: null });
    http.expectOne('/api/public/repositories/by-id/repo-1/contributors/main').flush([]);
    http.expectOne('/api/public/repositories/by-id/repo-1/languages/main').flush({ languages: [] });
  });

  it('lists commits, at a ref when one is given', () => {
    const { service, http } = setup();

    service.commitsById('repo-1').subscribe();
    service.commitsById('repo-1', 'develop').subscribe();

    const all = http.match((r) => r.url === '/api/public/repositories/by-id/repo-1/commits');
    expect(all.map((req) => req.request.params.get('ref'))).toEqual([null, 'develop']);
    all.forEach((req) => req.flush([]));
  });

  it('builds the same clone URL as the signed-in pages', () => {
    const { service } = setup();
    expect(service.cloneUrl(['acme', 'tools', 'cli'])).toBe(`${location.origin}/acme/tools/cli.git`);
  });

  it('lists branches under /api/public/repositories/{id}/branches', () => {
    const { branches, http } = setup();
    branches.listBranches('repo-1').subscribe();
    http.expectOne('/api/public/repositories/repo-1/branches').flush([]);
  });
});
