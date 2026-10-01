import { HttpClient } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';

export interface WikiPageSummary {
  slug: string;
  title: string;
}

export interface WikiList {
  headSha: string | null;
  pages: WikiPageSummary[];
}

export interface WikiPageDetail {
  content: string;
  headSha: string;
  title: string;
}

export interface WikiRevision {
  commitSha: string;
  authorName: string;
  authorEmail: string;
  committedAt: string;
  message: string;
}

export interface SaveWikiPageOptions {
  content: string;
  baseSha: string | null;
  message?: string;
}

@Injectable({ providedIn: 'root' })
export class WikiService {
  private http = inject(HttpClient);

  list(repositoryId: string) {
    return this.http.get<WikiList>(`/api/repositories/${repositoryId}/wiki`);
  }

  detail(repositoryId: string, slug: string) {
    return this.http.get<WikiPageDetail>(`/api/repositories/${repositoryId}/wiki/pages/${encodeURIComponent(slug)}`);
  }

  save(repositoryId: string, slug: string, options: SaveWikiPageOptions) {
    return this.http.put<WikiPageDetail>(`/api/repositories/${repositoryId}/wiki/pages/${encodeURIComponent(slug)}`, options);
  }

  delete(repositoryId: string, slug: string, baseSha: string) {
    return this.http.delete<void>(`/api/repositories/${repositoryId}/wiki/pages/${encodeURIComponent(slug)}?baseSha=${encodeURIComponent(baseSha)}`);
  }

  revisions(repositoryId: string, slug: string) {
    return this.http.get<WikiRevision[]>(`/api/repositories/${repositoryId}/wiki/pages/${encodeURIComponent(slug)}/revisions`);
  }

  revisionContent(repositoryId: string, slug: string, commitSha: string) {
    return this.http.get<{ content: string }>(`/api/repositories/${repositoryId}/wiki/pages/${encodeURIComponent(slug)}/revisions/${encodeURIComponent(commitSha)}`);
  }
}
