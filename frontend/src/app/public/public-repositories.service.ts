import { HttpClient, HttpParams } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { BlobContent, CommitInfo, Contributor, LanguageStat, ReadmeContent, RepositoriesService, Repository, ResolvedPath, TreeEntry } from '../repositories/repositories.service';
import { BranchInfo, MergeRequestsService } from '../merge-requests/merge-requests.service';

export type PublicCatalogSort = 'stars' | 'name' | 'created';

export interface PublicCatalogQuery {
  q: string;
  sort: PublicCatalogSort;
  /** 1-based. */
  page: number;
}

export interface PublicRepositorySummary {
  id: string;
  name: string;
  path: string[];
  owner: string;
  description: string;
  stars: number;
  createdAt: string;
}

export interface PublicCatalogPage {
  items: PublicRepositorySummary[];
  total: number;
  page: number;
  perPage: number;
}

export const PUBLIC_CATALOG_PAGE_SIZE = 20;

/** Longer search texts get cut, in the quick search and in the catalog's own field. */
export const PUBLIC_CATALOG_MAX_QUERY_LENGTH = 100;

/** What the reused repository pages read, so this service can stand in for `RepositoriesService` on public routes. */
export type RepositoryReads = Pick<
  RepositoriesService,
  'getById' | 'treeAt' | 'blobAt' | 'readmeAt' | 'listContributors' | 'getLanguages' | 'commitsById' | 'resolve' | 'cloneUrl'
>;

const BASE = '/api/public/repositories';

/** The anonymous, read-only API of public repositories. Same methods and shapes as `RepositoriesService`, under `/api/public`. */
@Injectable({ providedIn: 'root' })
export class PublicRepositoriesService implements RepositoryReads {
  private http = inject(HttpClient);

  search(query: PublicCatalogQuery) {
    let params = new HttpParams().set('sort', query.sort).set('page', query.page).set('perPage', PUBLIC_CATALOG_PAGE_SIZE);
    if (query.q) {
      params = params.set('q', query.q);
    }
    return this.http.get<PublicCatalogPage>(BASE, { params });
  }

  getById(repositoryId: string) {
    return this.http.get<Repository>(`${BASE}/by-id/${repositoryId}`);
  }

  treeAt(repositoryId: string, ref: string, path: string[], options?: { lastCommit?: boolean }) {
    const suffix = path.length > 0 ? `/${path.map(encodeURIComponent).join('/')}` : '';
    const params = options?.lastCommit === false ? new HttpParams().set('lastCommit', 'false') : undefined;
    return this.http.get<TreeEntry[]>(`${BASE}/by-id/${repositoryId}/tree/${encodeURIComponent(ref)}${suffix}`, { params });
  }

  blobAt(repositoryId: string, ref: string, path: string[]) {
    return this.http.get<BlobContent>(`${BASE}/by-id/${repositoryId}/blob/${encodeURIComponent(ref)}/${path.map(encodeURIComponent).join('/')}`);
  }

  readmeAt(repositoryId: string, ref: string) {
    return this.http.get<ReadmeContent>(`${BASE}/by-id/${repositoryId}/readme/${encodeURIComponent(ref)}`);
  }

  listContributors(repositoryId: string, ref: string) {
    return this.http.get<Contributor[]>(`${BASE}/by-id/${repositoryId}/contributors/${encodeURIComponent(ref)}`);
  }

  getLanguages(repositoryId: string, ref: string) {
    return this.http.get<{ languages: LanguageStat[] }>(`${BASE}/by-id/${repositoryId}/languages/${encodeURIComponent(ref)}`);
  }

  commitsById(repositoryId: string, ref?: string) {
    const params = ref ? new HttpParams().set('ref', ref) : undefined;
    return this.http.get<CommitInfo[]>(`${BASE}/by-id/${repositoryId}/commits`, { params });
  }

  resolve(segments: string[]) {
    return this.http.get<ResolvedPath>(`/api/public/resolve/${segments.map(encodeURIComponent).join('/')}`);
  }

  cloneUrl(path: string[]): string {
    return `${location.origin}/${path.join('/')}.git`;
  }
}

/** Stands in for `MergeRequestsService` on the public routes: the branch switcher only lists branches. */
@Injectable({ providedIn: 'root' })
export class PublicBranchesService implements Pick<MergeRequestsService, 'listBranches'> {
  private http = inject(HttpClient);

  listBranches(repositoryId: string) {
    return this.http.get<BranchInfo[]>(`${BASE}/${repositoryId}/branches`);
  }
}
