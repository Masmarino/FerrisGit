import { MergeRequestSummary, UserRef } from './merge-requests.service';

/** The name shown for an author whose account was deleted: their comments outlive them. */
export function authorName(author: UserRef | null): string {
  return author?.username ?? 'Utilisateur supprimé';
}

/** How a merge request ended and when; `null` while it is open. */
export function mergeRequestEnd(mergeRequest: MergeRequestSummary): { verb: string; at: string } | null {
  if (mergeRequest.status === 'open' || !mergeRequest.closedAt) {
    return null;
  }
  return { verb: mergeRequest.status === 'merged' ? 'fusionnée' : 'fermée', at: mergeRequest.closedAt };
}
