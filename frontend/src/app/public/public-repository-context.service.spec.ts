import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { PublicRepositoryContextService } from './public-repository-context.service';

describe('PublicRepositoryContextService', () => {
  function setup() {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting(), PublicRepositoryContextService] });
    return { context: TestBed.inject(PublicRepositoryContextService), http: TestBed.inject(HttpTestingController) };
  }

  it('enters a repository with no role and fetches nothing', () => {
    const { context, http } = setup();

    context.enter('repo-1', ['alice', 'hello'], [], null);

    expect(context.current()).toEqual({ repositoryId: 'repo-1', path: ['alice', 'hello'], role: null, ancestors: [], groupId: null });
    http.verify();
  });

  it('keeps the same context object for the same repository, and clears it on leave', () => {
    const { context } = setup();
    context.enter('repo-1', ['alice', 'hello'], [], null);
    const first = context.current();

    context.enter('repo-1', ['alice', 'hello'], [], null);
    expect(context.current()).toBe(first);

    context.leave();
    expect(context.current()).toBeNull();
  });
});
