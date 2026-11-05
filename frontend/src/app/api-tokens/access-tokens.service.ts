import { HttpClient } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';

export interface AccessTokenSummary {
  id: string;
  name: string;
  /** As the server writes them: `merge-requests:write`, one per area. */
  scopes: string[];
  expiresAt: string;
  expired: boolean;
  createdAt: string;
  lastUsedAt: string | null;
}

export interface NewAccessToken {
  name: string;
  scopes: string[];
  expiresInDays: number;
}

/** The scoped `fga_` tokens, for the REST API and Git. The `fg_` Git tokens are `TokensService`'s. */
@Injectable({ providedIn: 'root' })
export class AccessTokensService {
  private http = inject(HttpClient);

  list() {
    return this.http.get<AccessTokenSummary[]>('/api/access-tokens');
  }

  create(token: NewAccessToken) {
    return this.http.post<AccessTokenSummary & { token: string }>('/api/access-tokens', token);
  }

  revoke(id: string) {
    return this.http.delete<void>(`/api/access-tokens/${id}`);
  }
}
