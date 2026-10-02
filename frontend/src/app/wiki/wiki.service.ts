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

  private pageUrl(repositoryId: string, slug: string): string {
    return `/api/repositories/${repositoryId}/wiki/pages/${encodeURIComponent(slug)}`;
  }

  list(repositoryId: string) {
    return this.http.get<WikiList>(`/api/repositories/${repositoryId}/wiki`);
  }

  detail(repositoryId: string, slug: string) {
    return this.http.get<WikiPageDetail>(this.pageUrl(repositoryId, slug));
  }

  save(repositoryId: string, slug: string, options: SaveWikiPageOptions) {
    return this.http.put<WikiPageDetail>(this.pageUrl(repositoryId, slug), options);
  }

  delete(repositoryId: string, slug: string, baseSha: string) {
    return this.http.delete<void>(`${this.pageUrl(repositoryId, slug)}?baseSha=${encodeURIComponent(baseSha)}`);
  }

  revisions(repositoryId: string, slug: string) {
    return this.http.get<WikiRevision[]>(`${this.pageUrl(repositoryId, slug)}/revisions`);
  }

  revisionContent(repositoryId: string, slug: string, commitSha: string) {
    return this.http.get<{ content: string }>(`${this.pageUrl(repositoryId, slug)}/revisions/${encodeURIComponent(commitSha)}`);
  }
}
