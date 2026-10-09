import { MergeRequestSummary, UserRef } from './merge-requests.service';
import { t } from '../shared/i18n/translator';

export function authorName(author: UserRef | null): string {
  return author?.username ?? t('common.deletedUser');
}

export function mergeRequestEnd(mergeRequest: MergeRequestSummary): { verb: string; at: string } | null {
  if (mergeRequest.status === 'open' || !mergeRequest.closedAt) {
    return null;
  }
  return { verb: mergeRequest.status === 'merged' ? t('common.mergedVerb') : t('common.closedVerb'), at: mergeRequest.closedAt };
}
