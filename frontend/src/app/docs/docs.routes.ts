import { Routes } from '@angular/router';
import { docsHomeGuard } from './docs.guards';

const docsPage = () => import('./docs-page/docs-page').then((m) => m.DocsPage);

/**
 * Mounted twice under `/docs`: in the public layout without a session, in the shell with one. Not behind the
 * instance's "public pages" switch: the documentation is about the product, not about the instance's repositories.
 * Any other depth under `/docs` gets the docs' own "not found", not the public one.
 */
export const DOCS_ROUTES: Routes = [
  { path: '', pathMatch: 'full', canActivate: [docsHomeGuard], loadComponent: docsPage },
  { path: ':section/:page', loadComponent: docsPage },
  { path: '**', loadComponent: docsPage },
];
