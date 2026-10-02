import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { RepositoryContextService } from './repository-context.service';
import { PageTitleService } from '../shell/page-title.service';
import { repositoryFixture } from './repository-fixtures';

describe('RepositoryContextService', () => {
  function setup() {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting()] });
    const service = TestBed.inject(RepositoryContextService);
    const http = TestBed.inject(HttpTestingController);
    return { service, http };
  }

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  it('sets repositoryId, path, ancestors and groupId synchronously, and role once the fetch resolves', () => {
    const { service, http } = setup();

    service.enter('repo-1', ['acme', 'widget'], [], null);
    expect(service.current()).toEqual({ repositoryId: 'repo-1', path: ['acme', 'widget'], role: null, ancestors: [], groupId: null });

    http.expectOne('/api/repositories/by-id/repo-1').flush(repositoryFixture({ role: 'maintainer', path: ['acme', 'widget'] }));

    expect(service.current()).toEqual({ repositoryId: 'repo-1', path: ['acme', 'widget'], role: 'maintainer', ancestors: [], groupId: null });
  });

  it('stores ancestors and groupId as given, for a group-owned repository', () => {
    const { service, http } = setup();
    const ancestors = [{ label: 'acme', link: ['/repositories', 'acme'] }];

    service.enter('repo-1', ['acme', 'widget'], ancestors, 'group-1');
    expect(service.current()).toEqual({ repositoryId: 'repo-1', path: ['acme', 'widget'], role: null, ancestors, groupId: 'group-1' });

    http.expectOne('/api/repositories/by-id/repo-1').flush(repositoryFixture({ role: 'reader', path: ['acme', 'widget'] }));
  });

  it('does not re-fetch when entering the same repository again', () => {
    const { service, http } = setup();

    service.enter('repo-1', ['acme', 'widget'], [], null);
    http.expectOne('/api/repositories/by-id/repo-1').flush(repositoryFixture({ role: 'reader', path: ['acme', 'widget'] }));

    service.enter('repo-1', ['acme', 'widget'], [], null);

    http.expectNone('/api/repositories/by-id/repo-1');
  });

  it('discards a stale role response after switching to a different repository', () => {
    const { service, http } = setup();

    service.enter('repo-1', ['acme', 'widget'], [], null);
    const firstReq = http.expectOne('/api/repositories/by-id/repo-1');

    service.enter('repo-2', ['acme', 'other'], [], null);
    http.expectOne('/api/repositories/by-id/repo-2').flush(repositoryFixture({ id: 'repo-2', role: 'owner', path: ['acme', 'other'] }));

    // The first repository's fetch resolves after the switch and must not overwrite repo-2's context.
    firstReq.flush(repositoryFixture({ role: 'maintainer', path: ['acme', 'widget'] }));

    expect(service.current()).toEqual({ repositoryId: 'repo-2', path: ['acme', 'other'], role: 'owner', ancestors: [], groupId: null });
  });

  it('does not throw and keeps role null when the role fetch errors', () => {
    const { service, http } = setup();

    service.enter('repo-1', ['acme', 'widget'], [], null);
    expect(() => {
      http.expectOne('/api/repositories/by-id/repo-1').flush(null, { status: 500, statusText: 'Server Error' });
    }).not.toThrow();

    expect(service.current()?.role).toBeNull();
  });

  it('clears the context on leave', () => {
    const { service, http } = setup();

    service.enter('repo-1', ['acme', 'widget'], [], null);
    http.expectOne('/api/repositories/by-id/repo-1').flush(repositoryFixture({ role: 'owner', path: ['acme', 'widget'] }));

    service.leave();

    expect(service.current()).toBeNull();
  });

  it('resets the page title when entering a different repository, but not when re-entering the same one', () => {
    const { service, http } = setup();
    const pageTitle = TestBed.inject(PageTitleService);
    pageTitle.set('Issues');

    service.enter('repo-1', ['acme', 'widget'], [], null);
    expect(pageTitle.title()).toBe('');
    http.expectOne('/api/repositories/by-id/repo-1').flush(repositoryFixture({ role: 'reader', path: ['acme', 'widget'] }));

    pageTitle.set('Pipelines');
    service.enter('repo-1', ['acme', 'widget'], [], null);
    expect(pageTitle.title()).toBe('Pipelines');

    service.enter('repo-2', ['acme', 'other'], [], null);
    expect(pageTitle.title()).toBe('');
    http.expectOne('/api/repositories/by-id/repo-2').flush(repositoryFixture({ id: 'repo-2', role: 'reader', path: ['acme', 'other'] }));
  });
});
