import { HttpClient } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { Notification } from '../notifications/notifications.service';
import { SearchIssueResult, SearchMergeRequestResult } from '../search/search.service';

export interface DashboardResponse {
  assignedIssues: SearchIssueResult[];
  authoredIssues: SearchIssueResult[];
  authoredMergeRequests: SearchMergeRequestResult[];
  mergeRequestsToReview: SearchMergeRequestResult[];
  activity: Notification[];
}

@Injectable({ providedIn: 'root' })
export class DashboardService {
  private http = inject(HttpClient);

  get() {
    return this.http.get<DashboardResponse>('/api/dashboard');
  }
}
