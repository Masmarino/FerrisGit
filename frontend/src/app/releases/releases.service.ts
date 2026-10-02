import { HttpClient } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { UserRef } from '../shared/user-ref';

export interface TagSummary {
  name: string;
  targetSha: string;
}

export interface ReleaseSummary {
  id: string;
  tagName: string;
  title: string;
  draft: boolean;
  prerelease: boolean;
  /** `null` once the author's account was deleted (the release outlives it). */
  authorId: string | null;
  author: UserRef | null;
  /** The first 240 characters of the notes, trimmed (raw markdown); `''` without notes. */
  notesExcerpt: string;
  assetCount: number;
  createdAt: string;
  publishedAt: string | null;
}

export interface ReleaseAsset {
  id: string;
  filename: string;
  contentType: string;
  sizeBytes: number;
  /** `null` once the uploader's account was deleted (the file outlives it). */
  uploadedBy: string | null;
  uploader: UserRef | null;
  createdAt: string;
}

export interface ReleaseDetail {
  id: string;
  tagName: string;
  title: string;
  notes: string;
  draft: boolean;
  prerelease: boolean;
  /** `null` once the author's account was deleted (the release outlives it). */
  authorId: string | null;
  author: UserRef | null;
  createdAt: string;
  publishedAt: string | null;
  targetCommitSha: string | null;
  assets: ReleaseAsset[];
}

export type ReleaseStatus = 'draft' | 'prerelease' | 'published';

/** A draft stays a draft whatever its prerelease flag: it is not public yet. */
export function releaseStatus(release: Pick<ReleaseSummary, 'draft' | 'prerelease'>): ReleaseStatus {
  if (release.draft) {
    return 'draft';
  }
  return release.prerelease ? 'prerelease' : 'published';
}

export interface CreateReleaseOptions {
  tagName: string;
  targetCommitSha: string;
  title: string;
  notes: string;
  draft: boolean;
  prerelease: boolean;
}

export interface UpdateReleaseOptions {
  title?: string;
  notes?: string;
  prerelease?: boolean;
  draft?: boolean;
}

@Injectable({ providedIn: 'root' })
export class ReleasesService {
  private http = inject(HttpClient);

  private releaseUrl(repositoryId: string, tagName: string): string {
    return `/api/repositories/${repositoryId}/releases/${encodeURIComponent(tagName)}`;
  }

  listTags(repositoryId: string) {
    return this.http.get<TagSummary[]>(`/api/repositories/${repositoryId}/tags`);
  }

  /** Maintainers and above only, and refused while any release references the tag. Lets a tag stuck at the wrong commit (left by a deleted draft) be removed. */
  deleteTag(repositoryId: string, tagName: string) {
    return this.http.delete<void>(`/api/repositories/${repositoryId}/tags/${encodeURIComponent(tagName)}`);
  }

  list(repositoryId: string) {
    return this.http.get<ReleaseSummary[]>(`/api/repositories/${repositoryId}/releases`);
  }

  create(repositoryId: string, options: CreateReleaseOptions) {
    return this.http.post<ReleaseSummary>(`/api/repositories/${repositoryId}/releases`, options);
  }

  detail(repositoryId: string, tagName: string) {
    return this.http.get<ReleaseDetail>(this.releaseUrl(repositoryId, tagName));
  }

  update(repositoryId: string, tagName: string, options: UpdateReleaseOptions) {
    return this.http.patch<ReleaseDetail>(this.releaseUrl(repositoryId, tagName), options);
  }

  delete(repositoryId: string, tagName: string) {
    return this.http.delete<void>(this.releaseUrl(repositoryId, tagName));
  }

  uploadAsset(repositoryId: string, tagName: string, file: File) {
    const formData = new FormData();
    formData.append('file', file, file.name);
    return this.http.post<ReleaseAsset>(`${this.releaseUrl(repositoryId, tagName)}/assets`, formData);
  }

  deleteAsset(repositoryId: string, tagName: string, assetId: string) {
    return this.http.delete<void>(`${this.releaseUrl(repositoryId, tagName)}/assets/${assetId}`);
  }

  /** Fetched via `HttpClient` so the auth interceptor attaches the JWT (a bare anchor carries no `Authorization` header); callers turn the blob into a save prompt. */
  downloadAsset(repositoryId: string, tagName: string, assetId: string) {
    return this.http.get(`${this.releaseUrl(repositoryId, tagName)}/assets/${assetId}`, { responseType: 'blob' });
  }
}
