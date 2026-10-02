import { inject } from '@angular/core';
import { CanActivateFn, Router } from '@angular/router';
import { catchError, map, of } from 'rxjs';
import { DocsService, firstDocsPage } from './docs.service';

/**
 * `/docs` opens the first page of the index. When the index cannot be read (or is empty), the docs page itself
 * shows that state, inside the docs layout.
 */
export const docsHomeGuard: CanActivateFn = () => {
  const router = inject(Router);
  return inject(DocsService)
    .index()
    .pipe(
      map((index) => {
        const first = firstDocsPage(index);
        return first ? router.createUrlTree(first) : true;
      }),
      catchError(() => of(true)),
    );
};
