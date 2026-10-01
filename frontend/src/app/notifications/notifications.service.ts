import { HttpClient } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';

export interface Notification {
  id: string;
  kind:
    | 'merge_request_approved'
    | 'merge_request_changes_requested'
    | 'merge_request_commented'
    | 'merge_request_merged'
    | 'merge_request_closed'
    | 'collaborator_added'
    | 'collaborator_role_changed'
    | 'collaborator_removed'
    | 'pipeline_failed'
    | 'issue_assigned'
    | 'issue_commented'
    | 'issue_closed';
  repositoryOwner: string;
  repositoryName: string;
  actorUsername: string | null;
  mergeRequestId: string | null;
  mergeRequestTitle: string | null;
  pipelineId: string | null;
  commitSha: string | null;
  issueId: string | null;
  issueNumber: number | null;
  issueTitle: string | null;
  role: string | null;
  read: boolean;
  createdAt: string;
}

export interface UnreadCount {
  count: number;
}

@Injectable({ providedIn: 'root' })
export class NotificationsService {
  private http = inject(HttpClient);

  list() {
    return this.http.get<Notification[]>('/api/notifications');
  }

  unreadCount() {
    return this.http.get<UnreadCount>('/api/notifications/unread-count');
  }

  markRead(id: string) {
    return this.http.post<void>(`/api/notifications/${id}/read`, {});
  }

  markAllRead() {
    return this.http.post<void>('/api/notifications/read-all', {});
  }
}
