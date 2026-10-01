import { HttpClient } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';

export interface SearchRepositoryRef {
  id: string;
  name: string;
  path: string[];
}

export interface SearchRepositoryResult {
  id: string;
  name: string;
  description: string;
  path: string[];
  visibility: 'private' | 'public';
}

export interface SearchIssueResult {
  id: string;
  number: number;
  title: string;
  status: 'todo' | 'in_progress' | 'in_review' | 'done';
  kind: 'bug' | 'feature' | 'task' | 'epic';
  createdAt: string;
  repository: SearchRepositoryRef;
}

export interface SearchMergeRequestResult {
  id: string;
  title: string;
  status: 'open' | 'merged' | 'closed';
  sourceBranch: string;
  targetBranch: string;
  createdAt: string;
  repository: SearchRepositoryRef;
}

export interface SearchUserResult {
  id: string;
  username: string;
}

export interface SearchResponse {
  repositories: SearchRepositoryResult[];
  issues: SearchIssueResult[];
  mergeRequests: SearchMergeRequestResult[];
  users: SearchUserResult[];
}

@Injectable({ providedIn: 'root' })
export class SearchService {
  private http = inject(HttpClient);

  search(query: string) {
    return this.http.get<SearchResponse>('/api/search', { params: { q: query } });
  }
}
