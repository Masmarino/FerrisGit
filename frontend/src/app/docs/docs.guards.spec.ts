import { TestBed } from '@angular/core/testing';
import { provideRouter, UrlTree } from '@angular/router';
import { firstValueFrom, Observable, of } from 'rxjs';
import { docsHomeGuard } from './docs.guards';
import { DocsService } from './docs.service';
import { DOCS_INDEX, FAILING_DOCS, fakeDocsService } from './docs-fixtures';

describe('docsHomeGuard', () => {
  function run(docs = fakeDocsService()) {
    TestBed.configureTestingModule({ providers: [provideRouter([]), { provide: DocsService, useValue: docs }] });
    return firstValueFrom(TestBed.runInInjectionContext(() => docsHomeGuard({} as never, {} as never)) as Observable<boolean | UrlTree>);
  }

  it('opens the first page of the index', async () => {
    expect(String(await run())).toBe('/docs/demarrer/presentation');
  });

  it('follows the index, not a fixed address', async () => {
    const result = await run(fakeDocsService({ index: () => of({ sections: [DOCS_INDEX.sections[1]] }) }));
    expect(String(result)).toBe('/docs/ci-cd/premiers-pas');
  });

  it('lets the docs page tell the reader when the index cannot be read', async () => {
    expect(await run(fakeDocsService({ index: FAILING_DOCS }))).toBe(true);
  });

  it('lets the docs page tell the reader when the index is empty', async () => {
    expect(await run(fakeDocsService({ index: () => of({ sections: [] }) }))).toBe(true);
  });
});
