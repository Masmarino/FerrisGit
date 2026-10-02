import { Provider } from '@angular/core';
import { RepositoriesService } from '../repositories/repositories.service';
import { RepositoryContextService } from '../repositories/repository-context.service';
import { READ_ONLY_REPOSITORY } from '../repositories/read-only-repository';
import { ReleasesService } from '../releases/releases.service';
import { MergeRequestsService } from '../merge-requests/merge-requests.service';
import { PublicBranchesService, PublicRepositoriesService } from './public-repositories.service';
import { PublicReleasesService } from './public-releases.service';
import { PublicRepositoryContextService } from './public-repository-context.service';

/**
 * Points the reused repository pages at the anonymous API. Each stand-in covers only the methods those pages call
 * (a `Pick<…>` of the real service), and the context has a `null` role so no write action shows.
 */
export function providePublicRepositoryData(): Provider[] {
  return [
    { provide: RepositoriesService, useExisting: PublicRepositoriesService },
    { provide: ReleasesService, useExisting: PublicReleasesService },
    { provide: MergeRequestsService, useExisting: PublicBranchesService },
    { provide: RepositoryContextService, useClass: PublicRepositoryContextService },
    { provide: READ_ONLY_REPOSITORY, useValue: true },
  ];
}
