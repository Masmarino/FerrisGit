import { afterNextRender, inject } from '@angular/core';
import { ActivatedRoute, Router } from '@angular/router';
import { activationToken } from '@masmarino/gabarit';

/**
 * Reads the token of a mailed link (`<path>#token=…`; `?token=` still works for mails sent before) and scrubs it, and
 * any other parameter, from the address bar and the history entry. The fragment is preferred so no server sees the token.
 * Call it from a field initializer: it needs an injection context. The page keeps the result in a plain field, not a
 * value that follows the URL, because the kit's page restarts when its `token` changes. A token not shaped like the
 * server's comes back as `null`.
 */
export function consumeLinkToken(path: string): string | null {
  const router = inject(Router);
  const { fragment, queryParamMap } = inject(ActivatedRoute).snapshot;
  if (fragment !== null || queryParamMap.keys.length > 0) {
    afterNextRender(() => void router.navigateByUrl(path, { replaceUrl: true }).catch(() => undefined));
  }
  return activationToken(fragment, queryParamMap.get('token'));
}
