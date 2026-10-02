import { inject } from '@angular/core';
import { CanActivateFn, Router } from '@angular/router';
import { catchError, map, of } from 'rxjs';
import { DocsService, firstDocsPage } from './docs.service';

/**
 * `/docs` opens the first page of the index. If the index can't be read or is empty, the docs page shows that itself,
 * inside the docs layout.
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
