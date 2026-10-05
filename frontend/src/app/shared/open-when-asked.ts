import { inject } from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { ActivatedRoute, Router } from '@angular/router';

/** The query parameter the quick search sets to open a page's "Nouveau …" dialog, e.g. `?new=issue`. */
export const NEW_PARAM = 'new';

/**
 * Opens a page's creation dialog when the URL asks for it (`?new=<kind>`), then drops the parameter so a reload or the
 * Back button doesn't open it again. Also works when the page is already shown. Needs an injection context.
 */
export function openWhenAsked(kind: string, open: () => void): void {
  const route = inject(ActivatedRoute);
  const router = inject(Router);
  route.queryParamMap.pipe(takeUntilDestroyed()).subscribe((params) => {
    if (params.get(NEW_PARAM) !== kind) {
      return;
    }
    open();
    router.navigate([], { relativeTo: route, queryParams: { [NEW_PARAM]: null }, queryParamsHandling: 'merge', replaceUrl: true });
  });
}
