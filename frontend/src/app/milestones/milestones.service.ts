import { HttpClient } from '@angular/common/http';
import { Injectable, inject } from '@angular/core';

export interface Milestone {
  id: string;
  title: string;
  description: string;
  dueDate: string | null;
  state: 'open' | 'closed';
  repositoryId: string | null;
  groupId: string | null;
  createdAt: string;
}

export type MilestoneScope = { repositoryId: string } | { groupId: string };

@Injectable({ providedIn: 'root' })
export class MilestonesService {
  private readonly http = inject(HttpClient);

  private scopeUrl(scope: MilestoneScope): string {
    return 'repositoryId' in scope ? `/api/repositories/${scope.repositoryId}/milestones` : `/api/groups/${scope.groupId}/milestones`;
  }

  listForRepository(repositoryId: string) {
    return this.http.get<Milestone[]>(`/api/repositories/${repositoryId}/milestones`);
  }

  listForGroup(groupId: string) {
    return this.http.get<Milestone[]>(`/api/groups/${groupId}/milestones`);
  }

  create(scope: MilestoneScope, title: string, description: string, dueDate: string | null) {
    return this.http.post<Milestone>(this.scopeUrl(scope), { title, description, dueDate });
  }

  update(milestoneId: string, title: string, description: string, dueDate: string | null, state: 'open' | 'closed') {
    return this.http.patch<Milestone>(`/api/milestones/${milestoneId}`, { title, description, dueDate, state });
  }

  delete(milestoneId: string) {
    return this.http.delete<void>(`/api/milestones/${milestoneId}`);
  }
}
