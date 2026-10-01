import { HttpClient } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { UserRef } from '../shared/user-ref';

export interface PipelineSummary {
  id: string;
  commitSha: string;
  status: 'pending' | 'running' | 'success' | 'failed' | 'canceled';
  createdAt: string;
  finishedAt: string | null;
  triggeredBy: UserRef | null;
  /** Null when it cannot be resolved; the list endpoint resolves it for the 50 newest pipelines only. */
  commitMessage: string | null;
}

export interface JobSummary {
  id: string;
  stage: string;
  name: string;
  status: 'pending' | 'running' | 'success' | 'failed' | 'canceled';
  needs: string[];
  tags: string[];
  logs: string;
  createdAt: string;
  startedAt: string | null;
  finishedAt: string | null;
}

export interface PipelineDetail extends PipelineSummary {
  jobs: JobSummary[];
}

@Injectable({ providedIn: 'root' })
export class PipelinesService {
  private http = inject(HttpClient);

  listForRepository(repositoryId: string) {
    return this.http.get<PipelineSummary[]>(`/api/repositories/${repositoryId}/pipelines`);
  }

  detail(id: string) {
    return this.http.get<PipelineDetail>(`/api/pipelines/${id}`);
  }

  cancel(id: string) {
    return this.http.post<void>(`/api/pipelines/${id}/cancel`, {});
  }
}
