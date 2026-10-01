import { HttpClient } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';

export interface ApiTokenSummary {
  id: string;
  name: string;
  createdAt: string;
  lastUsedAt: string | null;
}

@Injectable({ providedIn: 'root' })
export class TokensService {
  private http = inject(HttpClient);

  list() {
    return this.http.get<ApiTokenSummary[]>('/api/tokens');
  }

  create(name: string) {
    return this.http.post<{ id: string; name: string; token: string }>('/api/tokens', { name });
  }

  revoke(id: string) {
    return this.http.delete<void>(`/api/tokens/${id}`);
  }
}
