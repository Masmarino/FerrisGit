import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { BreadcrumbSwitcherService } from './breadcrumb-switcher.service';

describe('BreadcrumbSwitcherService', () => {
  function setup() {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting()] });
    const service = TestBed.inject(BreadcrumbSwitcherService);
    const http = TestBed.inject(HttpTestingController);
    return { service, http };
  }

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  it('is empty and not loading before anything is loaded', () => {
    const { service } = setup();
    expect(service.groups()).toEqual([]);
    expect(service.repositories()).toEqual([]);
    expect(service.loading()).toBe(false);
  });

  it('loads subgroups and repositories for a group, tracking loading until both resolve', () => {
    const { service, http } = setup();

    service.loadForGroup('group-1');
    expect(service.loading()).toBe(true);

    const childrenReq = http.expectOne('/api/groups/group-1/children');
    const reposReq = http.expectOne('/api/groups/group-1/repositories');
    childrenReq.flush([{ id: 'group-2', parentGroupId: 'group-1', name: 'backend', description: '', createdAt: '2026-01-01' }]);
    // forkJoin only emits once both sources have emitted.
    expect(service.loading()).toBe(true);
    reposReq.flush([
      { id: 'repo-1', name: 'widget', description: '', owner: 'acme', role: 'reader', visibility: 'private', createdAt: '2026-01-01', path: ['acme', 'widget'] },
    ]);

    expect(service.loading()).toBe(false);
    expect(service.groups()).toEqual([{ id: 'group-2', name: 'backend' }]);
    expect(service.repositories()).toEqual([{ id: 'repo-1', path: ['acme', 'widget'] }]);
  });

  it('tracks loading for loadForOwner too', () => {
    const { service, http } = setup();

    service.loadForOwner('florian');
    expect(service.loading()).toBe(true);

    http.expectOne('/api/repositories').flush([]);

    expect(service.loading()).toBe(false);
  });

  it('does not re-fetch when loading the same group again', () => {
    const { service, http } = setup();

    service.loadForGroup('group-1');
    http.expectOne('/api/groups/group-1/children').flush([]);
    http.expectOne('/api/groups/group-1/repositories').flush([]);

    service.loadForGroup('group-1');

    http.expectNone('/api/groups/group-1/children');
    http.expectNone('/api/groups/group-1/repositories');
  });

  it('clears stale group/repository data synchronously when switching to a different group, before the new fetch resolves', () => {
    const { service, http } = setup();

    service.loadForGroup('group-1');
    http.expectOne('/api/groups/group-1/children').flush([
      { id: 'group-2', parentGroupId: 'group-1', name: 'backend', description: '', createdAt: '2026-01-01' },
    ]);
    http.expectOne('/api/groups/group-1/repositories').flush([
      { id: 'repo-1', name: 'widget', description: '', owner: 'acme', role: 'reader', visibility: 'private', createdAt: '2026-01-01', path: ['acme', 'widget'] },
    ]);
    expect(service.groups().length).toBe(1);
    expect(service.repositories().length).toBe(1);

    service.loadForGroup('group-3');
    // Cleared synchronously. This singleton lives across navigations, so a different group's switcher must not show the previous group's siblings.
    expect(service.groups()).toEqual([]);
    expect(service.repositories()).toEqual([]);

    http.expectOne('/api/groups/group-3/children').flush([]);
    http.expectOne('/api/groups/group-3/repositories').flush([]);
  });

  it("clears a previous group's repositories synchronously when switching to a personal owner", () => {
    const { service, http } = setup();

    service.loadForGroup('group-1');
    http.expectOne('/api/groups/group-1/children').flush([]);
    http.expectOne('/api/groups/group-1/repositories').flush([
      { id: 'repo-1', name: 'widget', description: '', owner: 'acme', role: 'reader', visibility: 'private', createdAt: '2026-01-01', path: ['acme', 'widget'] },
    ]);
    expect(service.repositories().length).toBe(1);

    service.loadForOwner('florian');
    expect(service.repositories()).toEqual([]);

    http.expectOne('/api/repositories').flush([]);
  });

  it("loads the owner's other personal repositories, filtering out group repositories and other owners' repositories", () => {
    const { service, http } = setup();

    service.loadForOwner('florian');

    http.expectOne('/api/repositories').flush([
      { id: 'repo-1', name: 'mine', description: '', owner: 'florian', role: 'owner', visibility: 'private', createdAt: '2026-01-01', path: ['florian', 'mine'] },
      { id: 'repo-2', name: 'other-owner', description: '', owner: 'someone-else', role: 'reader', visibility: 'public', createdAt: '2026-01-01', path: ['someone-else', 'other-owner'] },
      { id: 'repo-3', name: 'group-repo', description: '', owner: 'florian', role: 'owner', visibility: 'private', createdAt: '2026-01-01', path: ['acme', 'group-repo'] },
    ]);

    expect(service.groups()).toEqual([]);
    expect(service.repositories()).toEqual([{ id: 'repo-1', path: ['florian', 'mine'] }]);
  });

  it('does not re-fetch when loading the same owner again', () => {
    const { service, http } = setup();

    service.loadForOwner('florian');
    http.expectOne('/api/repositories').flush([]);

    service.loadForOwner('florian');

    http.expectNone('/api/repositories');
  });

  it('resets loading and loadedKey on error for loadForGroup, allowing retry', () => {
    const { service, http } = setup();

    service.loadForGroup('g1');
    expect(service.loading()).toBe(true);

    // forkJoin cancels the second request.
    const childrenReq = http.expectOne('/api/groups/g1/children');
    childrenReq.flush('error', { status: 500, statusText: 'Server Error' });

    expect(service.loading()).toBe(false);
    expect(service.groups()).toEqual([]);
    expect(service.repositories()).toEqual([]);

    service.loadForGroup('g1');
    expect(service.loading()).toBe(true);

    const retryChildrenReq = http.expectOne('/api/groups/g1/children');

    // Two repositories requests are pending (the failed load and the retry): flush the second.
    const reposRequests = http.match('/api/groups/g1/repositories');
    expect(reposRequests.length).toBe(2);
    const retryReposReq = reposRequests[1];

    retryChildrenReq.flush([{ id: 'group-2', parentGroupId: 'g1', name: 'subgroup', description: '', createdAt: '2026-01-01' }]);
    retryReposReq.flush([
      { id: 'repo-1', name: 'widget', description: '', owner: 'acme', role: 'reader', visibility: 'private', createdAt: '2026-01-01', path: ['acme', 'widget'] },
    ]);

    expect(service.loading()).toBe(false);
    expect(service.groups()).toEqual([{ id: 'group-2', name: 'subgroup' }]);
    expect(service.repositories()).toEqual([{ id: 'repo-1', path: ['acme', 'widget'] }]);
  });

  it('resets loading and loadedKey on error for loadForOwner, allowing retry', () => {
    const { service, http } = setup();

    service.loadForOwner('florian');
    expect(service.loading()).toBe(true);

    const reposReq = http.expectOne('/api/repositories');
    reposReq.flush('error', { status: 500, statusText: 'Server Error' });

    expect(service.loading()).toBe(false);
    expect(service.repositories()).toEqual([]);

    service.loadForOwner('florian');
    expect(service.loading()).toBe(true);

    const retryReposReq = http.expectOne('/api/repositories');

    retryReposReq.flush([
      { id: 'repo-1', name: 'mine', description: '', owner: 'florian', role: 'owner', visibility: 'private', createdAt: '2026-01-01', path: ['florian', 'mine'] },
    ]);

    expect(service.loading()).toBe(false);
    expect(service.repositories()).toEqual([{ id: 'repo-1', path: ['florian', 'mine'] }]);
  });
});
