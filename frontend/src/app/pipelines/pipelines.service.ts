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
  /** Null when it can't be resolved; the list endpoint only resolves it for the 50 newest pipelines. */
  commitMessage: string | null;
  /** The parser's message when the pipeline file was invalid. Such a pipeline fails from the start and has no job. */
  error: string | null;
}

export interface JobSummary {
  id: string;
  stage: string;
  name: string;
  /** `skipped` means it never started, because a job it depends on (or a previous stage) didn't succeed. */
  status: 'pending' | 'running' | 'success' | 'failed' | 'canceled' | 'skipped';
  needs: string[];
  tags: string[];
  logs: string;
  /** When log retention emptied this job's log; absent or null while the log is intact. */
  logsPurgedAt?: string | null;
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
