import { reviewStatus } from './review-status';
import { Review, ReviewSummary } from './merge-requests.service';

function review(overrides: Partial<Review> = {}): Review {
  return { userId: 'u1', username: 'alice', decision: 'approved', stale: false, createdAt: '2026-09-23T10:00:00Z', ...overrides };
}

function summary(overrides: Partial<ReviewSummary> = {}): ReviewSummary {
  return { reviews: [], requiredApprovals: 0, liveApprovalCount: 0, blocked: false, ...overrides };
}

describe('reviewStatus', () => {
  it('is null while the reviews are not loaded', () => {
    expect(reviewStatus(null)).toBeNull();
  });

  it('says changes were requested when a live changes_requested review blocks the merge', () => {
    expect(reviewStatus(summary({ blocked: true, reviews: [review({ decision: 'changes_requested' })] }))).toEqual({
      blocked: true,
      tone: 'warning',
      icon: 'alert-triangle',
      text: 'Des changements ont été demandés.',
    });
  });

  it('counts the approvals still required when blocked without a live changes request (a stale one does not count)', () => {
    const status = reviewStatus(summary({ blocked: true, liveApprovalCount: 1, requiredApprovals: 2, reviews: [review(), review({ userId: 'u2', decision: 'changes_requested', stale: true })] }));
    expect(status).toEqual({ blocked: true, tone: 'info', icon: 'info', text: '1/2 approbations requises.' });
  });

  it('counts the live approvals when nothing blocks the merge', () => {
    expect(reviewStatus(summary({ liveApprovalCount: 1, reviews: [review()] }))).toEqual({ blocked: false, tone: 'success', icon: 'check', text: '1 approbation' });
    expect(reviewStatus(summary({ liveApprovalCount: 2, requiredApprovals: 2, reviews: [review(), review({ userId: 'u2' })] }))?.text).toBe('2 approbations');
  });

  it('says there is no review yet, or no live approval', () => {
    expect(reviewStatus(summary())).toEqual({ blocked: false, tone: 'neutral', icon: 'message-circle', text: "Aucune revue pour l'instant." });
    expect(reviewStatus(summary({ reviews: [review({ stale: true })] }))).toEqual({ blocked: false, tone: 'neutral', icon: 'message-circle', text: 'Aucune approbation à jour.' });
  });
});
