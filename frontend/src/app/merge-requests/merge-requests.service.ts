import { HttpClient } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { Label } from '../labels/labels.service';
import { UserRef } from '../shared/user-ref';

export type { UserRef } from '../shared/user-ref';

export interface BranchInfo {
  name: string;
  tipSha: string;
  isDefault: boolean;
}

export interface MergeRequestSummary {
  id: string;
  sourceBranch: string;
  targetBranch: string;
  title: string;
  description: string;
  status: 'open' | 'merged' | 'closed';
  mergeCommitSha: string | null;
  createdAt: string;
  closedAt: string | null;
  milestoneId: string | null;
  labels: Label[];
  author: UserRef | null;
  commentCount: number;
}

export interface SplitDiffRow {
  oldLine: number | null;
  oldContent: string | null;
  newLine: number | null;
  newContent: string | null;
  kind: 'context' | 'added' | 'removed' | 'modified';
}

export interface Hunk {
  rows: SplitDiffRow[];
}

export interface FileDiff {
  path: string;
  change: 'added' | 'modified' | 'deleted' | 'binary';
  hunks: Hunk[];
}

export interface Comment {
  id: string;
  /** `null` once the author's account is deleted; the comment stays. */
  authorId: string | null;
  author: UserRef | null;
  body: string;
  createdAt: string;
  replyToId: string | null;
  filePath: string | null;
  lineNumber: number | null;
  endLine: number | null;
  side: 'old' | 'new' | null;
  outdated: boolean;
  resolved: boolean;
  suggestedContent: string | null;
  appliedAt: string | null;
  appliedCommitSha: string | null;
}

export interface Review {
  userId: string;
  username: string;
  decision: 'approved' | 'changes_requested';
  stale: boolean;
  createdAt: string;
}

export interface ReviewSummary {
  reviews: Review[];
  requiredApprovals: number;
  liveApprovalCount: number;
  blocked: boolean;
}

export interface ExcerptLine {
  line: number | null;
  kind: 'context' | 'added' | 'removed';
  content: string;
}

export type MergeRequestEventKind = 'review_submitted' | 'labels_changed' | 'milestone_changed' | 'title_changed' | 'commits_pushed' | 'merged' | 'closed';

export interface TimelineComment {
  type: 'comment';
  id: string;
  createdAt: string;
  author: UserRef | null;
  body: string;
}

export interface TimelineThread {
  type: 'thread';
  id: string;
  createdAt: string;
  filePath: string;
  lineNumber: number;
  endLine: number | null;
  side: 'old' | 'new';
  outdated: boolean;
  resolved: boolean;
  resolvedBy: UserRef | null;
  resolvedAt: string | null;
  excerpt: ExcerptLine[];
  root: Comment;
  replies: Comment[];
}

export interface TimelineEvent {
  type: 'event';
  id: string;
  createdAt: string;
  actor: UserRef | null;
  kind: MergeRequestEventKind;
  payload: Record<string, unknown>;
}

export type TimelineItem = TimelineComment | TimelineThread | TimelineEvent;

export interface TimelineResponse {
  author: UserRef | null;
  items: TimelineItem[];
}

export type MergeAttemptResult = (MergeRequestSummary & { outcome: 'merged' }) | { outcome: 'conflicting' };

export interface CreateMergeRequestPayload {
  sourceBranch: string;
  targetBranch: string;
  title: string;
  description: string;
}

@Injectable({ providedIn: 'root' })
export class MergeRequestsService {
  private http = inject(HttpClient);

  listBranches(repositoryId: string) {
    return this.http.get<BranchInfo[]>(`/api/repositories/${repositoryId}/branches`);
  }

  listForRepository(repositoryId: string, filters: { labelIds?: string[]; milestoneId?: string } = {}) {
    const params: Record<string, string> = {};
    if (filters.labelIds?.length) {
      params['labelIds'] = filters.labelIds.join(',');
    }
    if (filters.milestoneId) {
      params['milestoneId'] = filters.milestoneId;
    }
    return this.http.get<MergeRequestSummary[]>(`/api/repositories/${repositoryId}/merge-requests`, { params });
  }

  create(repositoryId: string, payload: CreateMergeRequestPayload) {
    return this.http.post<MergeRequestSummary>(`/api/repositories/${repositoryId}/merge-requests`, payload);
  }

  detail(id: string) {
    return this.http.get<MergeRequestSummary>(`/api/merge-requests/${id}`);
  }

  update(mergeRequestId: string, title: string, description: string, milestoneId: string | null) {
    return this.http.patch<MergeRequestSummary>(`/api/merge-requests/${mergeRequestId}`, { title, description, milestoneId });
  }

  diff(id: string) {
    return this.http.get<FileDiff[]>(`/api/merge-requests/${id}/diff`);
  }

  timeline(id: string) {
    return this.http.get<TimelineResponse>(`/api/merge-requests/${id}/timeline`);
  }

  listComments(id: string) {
    return this.http.get<Comment[]>(`/api/merge-requests/${id}/comments`);
  }

  addComment(mergeRequestId: string, body: string, options?: { replyToId?: string; filePath?: string; lineNumber?: number; endLine?: number; side?: 'old' | 'new'; suggestedContent?: string }) {
    return this.http.post<Comment>(`/api/merge-requests/${mergeRequestId}/comments`, { body, ...options });
  }

  resolveComment(mergeRequestId: string, commentId: string) {
    return this.http.post<void>(`/api/merge-requests/${mergeRequestId}/comments/${commentId}/resolve`, {});
  }

  unresolveComment(mergeRequestId: string, commentId: string) {
    return this.http.post<void>(`/api/merge-requests/${mergeRequestId}/comments/${commentId}/unresolve`, {});
  }

  applySuggestion(mergeRequestId: string, commentId: string) {
    return this.http.post<Comment>(`/api/merge-requests/${mergeRequestId}/comments/${commentId}/apply-suggestion`, {});
  }

  listReviews(id: string) {
    return this.http.get<ReviewSummary>(`/api/merge-requests/${id}/reviews`);
  }

  submitReview(id: string, decision: 'approved' | 'changes_requested') {
    return this.http.post<Review>(`/api/merge-requests/${id}/reviews`, { decision });
  }

  merge(id: string) {
    return this.http.post<MergeAttemptResult>(`/api/merge-requests/${id}/merge`, {});
  }

  close(id: string) {
    return this.http.post<void>(`/api/merge-requests/${id}/close`, {});
  }
}
