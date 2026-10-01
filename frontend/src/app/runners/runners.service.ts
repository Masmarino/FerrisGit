import { HttpClient } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';

export interface RunnerSummary {
  id: string;
  name: string;
  tags: string[];
  lastHeartbeatAt: string | null;
  createdAt: string;
}

@Injectable({ providedIn: 'root' })
export class RunnersService {
  private http = inject(HttpClient);

  list() {
    return this.http.get<RunnerSummary[]>('/api/admin/runners');
  }

  register(name: string, tags: string[]) {
    return this.http.post<{ id: string; name: string; tags: string[]; token: string }>('/api/admin/runners', { name, tags });
  }
}
