import { HttpClient } from '@angular/common/http';
import { Injectable, inject } from '@angular/core';

export interface Label {
  id: string;
  name: string;
  color: string;
  repositoryId: string | null;
  groupId: string | null;
  createdAt: string;
}

export type LabelScope = { repositoryId: string } | { groupId: string };

@Injectable({ providedIn: 'root' })
export class LabelsService {
  private readonly http = inject(HttpClient);

  private scopeUrl(scope: LabelScope): string {
    return 'repositoryId' in scope ? `/api/repositories/${scope.repositoryId}/labels` : `/api/groups/${scope.groupId}/labels`;
  }

  listForRepository(repositoryId: string) {
    return this.http.get<Label[]>(`/api/repositories/${repositoryId}/labels`);
  }

  listForGroup(groupId: string) {
    return this.http.get<Label[]>(`/api/groups/${groupId}/labels`);
  }

  create(scope: LabelScope, name: string, color: string) {
    return this.http.post<Label>(this.scopeUrl(scope), { name, color });
  }

  update(labelId: string, name: string, color: string) {
    return this.http.patch<Label>(`/api/labels/${labelId}`, { name, color });
  }

  delete(labelId: string) {
    return this.http.delete<void>(`/api/labels/${labelId}`);
  }

  setForIssue(repositoryId: string, number: number, labelIds: string[]) {
    return this.http.put<Label[]>(`/api/repositories/${repositoryId}/issues/${number}/labels`, { labelIds });
  }

  setForMergeRequest(mergeRequestId: string, labelIds: string[]) {
    return this.http.put<Label[]>(`/api/merge-requests/${mergeRequestId}/labels`, { labelIds });
  }
}
