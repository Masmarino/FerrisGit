import { HttpClient } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { ReleaseDetail, ReleasesService, ReleaseSummary, TagSummary } from '../releases/releases.service';

const BASE = '/api/public/repositories';

/** The read side of `ReleasesService` under `/api/public`, for the release pages and branch switcher on public routes. */
@Injectable({ providedIn: 'root' })
export class PublicReleasesService implements Pick<ReleasesService, 'listTags' | 'list' | 'detail' | 'downloadAsset'> {
  private http = inject(HttpClient);

  listTags(repositoryId: string) {
    return this.http.get<TagSummary[]>(`${BASE}/${repositoryId}/tags`);
  }

  list(repositoryId: string) {
    return this.http.get<ReleaseSummary[]>(`${BASE}/${repositoryId}/releases`);
  }

  detail(repositoryId: string, tagName: string) {
    return this.http.get<ReleaseDetail>(`${BASE}/${repositoryId}/releases/${encodeURIComponent(tagName)}`);
  }

  downloadAsset(repositoryId: string, tagName: string, assetId: string) {
    return this.http.get(`${BASE}/${repositoryId}/releases/${encodeURIComponent(tagName)}/assets/${assetId}`, { responseType: 'blob' });
  }
}
