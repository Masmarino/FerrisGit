import { HttpClient, HttpParams } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';

export interface Repository {
  id: string;
  name: string;
  description: string;
  owner: string;
  role: 'owner' | 'reader' | 'contributor' | 'maintainer';
  visibility: 'private' | 'public';
  createdAt: string;
  /** The resolvable segment path: `[ownerUsername, name]` or `[...ancestorGroupNames, name]`. `owner` is only the creator's username, so build links from `path`. */
  path: string[];
  starCount?: number;
  isStarred?: boolean;
  sizeBytes?: number;
}

export interface CreateRepositoryOptions {
  description?: string;
  ciEnabled?: boolean;
  requiredApprovals?: number;
  pipelineFilePath?: string;
  groupPath?: string;
}

export interface TreeEntry {
  name: string;
  isDir: boolean;
  lastCommit: CommitInfo | null;
}

export interface BlobContent {
  sha: string;
  size: number;
  isBinary: boolean;
  content: string | null;
}

export interface ReadmeContent {
  content: string | null;
}

export interface StarResponse {
  starCount: number;
  isStarred: boolean;
}

export interface Contributor {
  name: string;
  email: string;
  commitCount: number;
}

export interface LanguageStat {
  name: string;
  bytes: number;
  percentage: number;
}

export interface CommitInfo {
  sha: string;
  message: string;
  authorName: string;
  authorEmail: string;
  committedAt: string;
}

export type ResolvedPath =
  | { type: 'personalRepository'; repositoryId: string }
  | { type: 'group'; groupId: string; chain: { id: string; name: string }[]; role: 'reader' | 'contributor' | 'maintainer' | null }
  | { type: 'groupRepository'; repositoryId: string; chain: { id: string; name: string }[] };

@Injectable({ providedIn: 'root' })
export class RepositoriesService {
  private http = inject(HttpClient);

  list(options: { starred?: boolean } = {}) {
    const params: Record<string, string> = options.starred ? { starred: 'true' } : {};
    return this.http.get<Repository[]>('/api/repositories', { params });
  }

  create(name: string, visibility: 'private' | 'public', options: CreateRepositoryOptions = {}) {
    return this.http.post<Repository>('/api/repositories', { name, visibility, ...options });
  }

  get(owner: string, name: string) {
    return this.http.get<Repository>(`/api/repositories/${owner}/${name}`);
  }

  commits(owner: string, name: string) {
    return this.http.get<CommitInfo[]>(`/api/repositories/${owner}/${name}/commits`);
  }

  getById(repositoryId: string) {
    return this.http.get<Repository>(`/api/repositories/by-id/${repositoryId}`);
  }

  delete(repositoryId: string) {
    return this.http.delete<void>(`/api/repositories/by-id/${repositoryId}`);
  }

  /** Lists a folder at `ref`. `options.lastCommit: false` skips the server's costly per-entry last-commit lookup, and `lastCommit` is then `null`. */
  treeAt(repositoryId: string, ref: string, path: string[], options?: { lastCommit?: boolean }) {
    const suffix = path.length > 0 ? `/${path.map(encodeURIComponent).join('/')}` : '';
    const params = options?.lastCommit === false ? new HttpParams().set('lastCommit', 'false') : undefined;
    return this.http.get<TreeEntry[]>(`/api/repositories/by-id/${repositoryId}/tree/${encodeURIComponent(ref)}${suffix}`, { params });
  }

  blobAt(repositoryId: string, ref: string, path: string[]) {
    return this.http.get<BlobContent>(`/api/repositories/by-id/${repositoryId}/blob/${encodeURIComponent(ref)}/${path.map(encodeURIComponent).join('/')}`);
  }

  readmeAt(repositoryId: string, ref: string) {
    return this.http.get<ReadmeContent>(`/api/repositories/by-id/${repositoryId}/readme/${encodeURIComponent(ref)}`);
  }

  star(repositoryId: string) {
    return this.http.post<StarResponse>(`/api/repositories/by-id/${repositoryId}/star`, {});
  }

  unstar(repositoryId: string) {
    return this.http.delete<StarResponse>(`/api/repositories/by-id/${repositoryId}/star`);
  }

  listContributors(repositoryId: string, ref: string) {
    return this.http.get<Contributor[]>(`/api/repositories/by-id/${repositoryId}/contributors/${encodeURIComponent(ref)}`);
  }

  getLanguages(repositoryId: string, ref: string) {
    return this.http.get<{ languages: LanguageStat[] }>(`/api/repositories/by-id/${repositoryId}/languages/${encodeURIComponent(ref)}`);
  }

  commitsById(repositoryId: string, ref?: string) {
    const params = ref ? new HttpParams().set('ref', ref) : undefined;
    return this.http.get<CommitInfo[]>(`/api/repositories/by-id/${repositoryId}/commits`, { params });
  }

  listForGroup(groupId: string) {
    return this.http.get<Repository[]>(`/api/groups/${groupId}/repositories`);
  }

  resolve(segments: string[]) {
    return this.http.get<ResolvedPath>(`/api/resolve/${segments.map(encodeURIComponent).join('/')}`);
  }

  cloneUrl(path: string[]): string {
    return `${location.origin}/${path.join('/')}.git`;
  }
}
