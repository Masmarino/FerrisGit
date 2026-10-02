import { MergeRequestSummary, UserRef } from './merge-requests.service';

export function authorName(author: UserRef | null): string {
  return author?.username ?? 'Utilisateur supprimé';
}

export function mergeRequestEnd(mergeRequest: MergeRequestSummary): { verb: string; at: string } | null {
  if (mergeRequest.status === 'open' || !mergeRequest.closedAt) {
    return null;
  }
  return { verb: mergeRequest.status === 'merged' ? 'fusionnée' : 'fermée', at: mergeRequest.closedAt };
}
