import { ReviewSummary } from './merge-requests.service';

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
      ? { blocked: true, tone: 'warning', icon: 'alert-triangle', text: 'Des changements ont été demandés.' }
      : { blocked: true, tone: 'info', icon: 'info', text: `${summary.liveApprovalCount}/${summary.requiredApprovals} approbations requises.` };
  }
  const approvals = summary.liveApprovalCount;
  if (approvals > 0) {
    return { blocked: false, tone: 'success', icon: 'check', text: `${approvals} ${approvals === 1 ? 'approbation' : 'approbations'}` };
  }
  return { blocked: false, tone: 'neutral', icon: 'message-circle', text: summary.reviews.length === 0 ? "Aucune revue pour l'instant." : 'Aucune approbation à jour.' };
}
