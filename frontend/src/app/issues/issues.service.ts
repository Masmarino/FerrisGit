import { HttpClient } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { Label } from '../labels/labels.service';
import { UserRef } from '../shared/user-ref';

export interface Issue {
  id: string;
  number: number;
  authorId: string;
  assigneeId: string | null;
  title: string;
  description: string;
  status: 'todo' | 'in_progress' | 'in_review' | 'done';
  kind: 'bug' | 'feature' | 'task' | 'epic';
  parentIssueId: string | null;
  createdAt: string;
  closedAt: string | null;
  milestoneId: string | null;
  labels: Label[];
  author: UserRef | null;
  /** `null` when unassigned, or when the assignee no longer resolves. */
  assignee: UserRef | null;
  commentCount: number;
}

export interface IssueComment {
  id: string;
  authorId: string;
  author: UserRef | null;
  body: string;
  createdAt: string;
}

@Injectable({ providedIn: 'root' })
export class IssuesService {
  private http = inject(HttpClient);

  list(repositoryId: string, filters: { labelIds?: string[]; milestoneId?: string } = {}) {
    const params: Record<string, string> = {};
    if (filters.labelIds?.length) {
      params['labelIds'] = filters.labelIds.join(',');
    }
    if (filters.milestoneId) {
      params['milestoneId'] = filters.milestoneId;
    }
    return this.http.get<Issue[]>(`/api/repositories/${repositoryId}/issues`, { params });
  }

  create(repositoryId: string, title: string, description: string, kind: string) {
    return this.http.post<Issue>(`/api/repositories/${repositoryId}/issues`, { title, description, kind });
  }

  detail(repositoryId: string, number: number) {
    return this.http.get<Issue>(`/api/repositories/${repositoryId}/issues/${number}`);
  }

  update(repositoryId: string, number: number, title: string, description: string, kind: string, milestoneId: string | null) {
    return this.http.patch<Issue>(`/api/repositories/${repositoryId}/issues/${number}`, { title, description, kind, milestoneId });
  }

  updateStatus(repositoryId: string, number: number, status: string) {
    return this.http.patch<Issue>(`/api/repositories/${repositoryId}/issues/${number}/status`, { status });
  }

  assign(repositoryId: string, number: number, assigneeId: string | null) {
    return this.http.post<Issue>(`/api/repositories/${repositoryId}/issues/${number}/assign`, { assigneeId });
  }

  close(repositoryId: string, number: number) {
    return this.http.post<Issue>(`/api/repositories/${repositoryId}/issues/${number}/close`, {});
  }

  reopen(repositoryId: string, number: number) {
    return this.http.post<Issue>(`/api/repositories/${repositoryId}/issues/${number}/reopen`, {});
  }

  listComments(repositoryId: string, number: number) {
    return this.http.get<IssueComment[]>(`/api/repositories/${repositoryId}/issues/${number}/comments`);
  }

  addComment(repositoryId: string, number: number, body: string) {
    return this.http.post<IssueComment>(`/api/repositories/${repositoryId}/issues/${number}/comments`, { body });
  }
}
