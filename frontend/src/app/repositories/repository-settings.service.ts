import { HttpClient } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { MemberRole } from './repositories.service';

export interface RepositorySettings {
  pipelineFilePath: string;
  ciEnabled: boolean;
  requiredApprovals: number;
}

export interface CiVariableSummary {
  id: string;
  key: string;
  masked: boolean;
}

export interface CollaboratorSummary {
  userId: string;
  username: string;
  role: MemberRole;
  createdAt: string;
}

export interface WebhookSummary {
  id: string;
  url: string;
  events: string[];
  active: boolean;
  createdAt: string;
}

export interface WebhookDelivery {
  id: string;
  eventKind: string;
  httpStatus: number | null;
  success: boolean;
  errorMessage: string | null;
  createdAt: string;
}

export const WEBHOOK_EVENT_OPTIONS = [
  'merge_request_approved',
  'merge_request_changes_requested',
  'merge_request_commented',
  'merge_request_merged',
  'merge_request_closed',
  'collaborator_added',
  'collaborator_role_changed',
  'collaborator_removed',
  'pipeline_failed',
  'issue_assigned',
  'issue_commented',
  'issue_closed',
] as const;

@Injectable({ providedIn: 'root' })
export class RepositorySettingsService {
  private http = inject(HttpClient);

  get(repositoryId: string) {
    return this.http.get<RepositorySettings>(`/api/repositories/${repositoryId}/settings`);
  }

  update(repositoryId: string, update: Partial<RepositorySettings>) {
    return this.http.put<RepositorySettings>(`/api/repositories/${repositoryId}/settings`, update);
  }

  listCiVariables(repositoryId: string) {
    return this.http.get<CiVariableSummary[]>(`/api/repositories/${repositoryId}/ci-variables`);
  }

  setCiVariable(repositoryId: string, key: string, value: string, masked: boolean) {
    return this.http.post<CiVariableSummary>(`/api/repositories/${repositoryId}/ci-variables`, { key, value, masked });
  }

  deleteCiVariable(repositoryId: string, id: string) {
    return this.http.delete<void>(`/api/repositories/${repositoryId}/ci-variables/${id}`);
  }

  listCollaborators(repositoryId: string) {
    return this.http.get<CollaboratorSummary[]>(`/api/repositories/${repositoryId}/collaborators`);
  }

  addCollaborator(repositoryId: string, username: string, role: string) {
    return this.http.post<void>(`/api/repositories/${repositoryId}/collaborators`, { username, role });
  }

  setCollaboratorRole(repositoryId: string, username: string, role: string) {
    return this.http.patch<void>(`/api/repositories/${repositoryId}/collaborators/${username}`, { role });
  }

  removeCollaborator(repositoryId: string, username: string) {
    return this.http.delete<void>(`/api/repositories/${repositoryId}/collaborators/${username}`);
  }

  listWebhooks(repositoryId: string) {
    return this.http.get<WebhookSummary[]>(`/api/repositories/${repositoryId}/webhooks`);
  }

  createWebhook(repositoryId: string, url: string, secret: string, events: string[]) {
    return this.http.post<WebhookSummary>(`/api/repositories/${repositoryId}/webhooks`, { url, secret, events });
  }

  deleteWebhook(repositoryId: string, id: string) {
    return this.http.delete<void>(`/api/repositories/${repositoryId}/webhooks/${id}`);
  }

  listWebhookDeliveries(repositoryId: string, id: string) {
    return this.http.get<WebhookDelivery[]>(`/api/repositories/${repositoryId}/webhooks/${id}/deliveries`);
  }
}
