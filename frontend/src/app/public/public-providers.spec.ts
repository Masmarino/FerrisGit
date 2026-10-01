import { Component, inject } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting } from '@angular/common/http/testing';
import { providePublicRepositoryData } from './public-providers';
import { RepositoriesService } from '../repositories/repositories.service';
import { RepositoryContextService } from '../repositories/repository-context.service';
import { READ_ONLY_REPOSITORY } from '../repositories/read-only-repository';
import { ReleasesService } from '../releases/releases.service';
import { MergeRequestsService } from '../merge-requests/merge-requests.service';
import { PublicBranchesService, PublicRepositoriesService } from './public-repositories.service';
import { PublicReleasesService } from './public-releases.service';
import { PublicRepositoryContextService } from './public-repository-context.service';

@Component({ standalone: true, template: '', providers: providePublicRepositoryData() })
class PublicHost {
  readonly repositories = inject(RepositoriesService);
  readonly releases = inject(ReleasesService);
  readonly mergeRequests = inject(MergeRequestsService);
  readonly context = inject(RepositoryContextService);
  readonly readOnly = inject(READ_ONLY_REPOSITORY);
}

describe('providePublicRepositoryData', () => {
  it('swaps the services the reused repository pages inject for their anonymous, read-only stand-ins', () => {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting()] });
    const host = TestBed.createComponent(PublicHost).componentInstance;

    expect(host.repositories).toBe(TestBed.inject(PublicRepositoriesService));
    expect(host.releases).toBe(TestBed.inject(PublicReleasesService));
    expect(host.mergeRequests).toBe(TestBed.inject(PublicBranchesService));
    expect(host.context).toBeInstanceOf(PublicRepositoryContextService);
    expect(host.readOnly).toBe(true);
  });

  it('leaves the signed-in pages on the real services', () => {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting()] });

    expect(TestBed.inject(RepositoriesService)).toBeInstanceOf(RepositoriesService);
    expect(TestBed.inject(READ_ONLY_REPOSITORY)).toBe(false);
  });
});
