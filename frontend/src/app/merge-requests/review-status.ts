import { ReviewSummary } from './merge-requests.service';
import { t, tn } from '../shared/i18n/translator';

export interface ReviewStatus {
  blocked: boolean;
  tone: 'warning' | 'info' | 'success' | 'neutral';
  icon: string;
  text: string;
}

export function reviewStatus(summary: ReviewSummary | null): ReviewStatus | null {
  if (!summary) {
    return null;
  }
  if (summary.blocked) {
    const changesRequested = summary.reviews.some((review) => !review.stale && review.decision === 'changes_requested');
    return changesRequested
      ? { blocked: true, tone: 'warning', icon: 'alert-triangle', text: t('mergeRequests.review.changesRequested') }
      : { blocked: true, tone: 'info', icon: 'info', text: t('mergeRequests.review.required', { live: summary.liveApprovalCount, required: summary.requiredApprovals }) };
  }
  const approvals = summary.liveApprovalCount;
  if (approvals > 0) {
    return { blocked: false, tone: 'success', icon: 'check', text: tn('mergeRequests.review.approvals', approvals) };
  }
  return { blocked: false, tone: 'neutral', icon: 'message-circle', text: summary.reviews.length === 0 ? t('mergeRequests.review.none') : t('mergeRequests.review.noneCurrent') };
}
