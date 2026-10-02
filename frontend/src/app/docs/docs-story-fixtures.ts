// Storybook-only: the docs routes over the fixtures.
import { inject, provideAppInitializer } from '@angular/core';
import { provideLocationMocks } from '@angular/common/testing';
import { provideRouter, Router } from '@angular/router';
import { applicationConfig } from '@storybook/angular-vite';
import { DocsService } from './docs.service';
import { DOCS_ROUTES } from './docs.routes';
import { fakeDocsService } from './docs-fixtures';

/** The docs routes with in-memory navigation starting at `url`, over the fixtures (or the given service). */
export function withDocs(options: { url?: string; docs?: Pick<DocsService, 'index' | 'page'> } = {}) {
  return applicationConfig({
    providers: [
      provideRouter([{ path: 'docs', children: DOCS_ROUTES }]),
      provideLocationMocks(),
      provideAppInitializer(() => inject(Router).navigateByUrl(options.url ?? '/docs/ci-cd/reference-yaml')),
      { provide: DocsService, useValue: options.docs ?? fakeDocsService() },
    ],
  });
}
