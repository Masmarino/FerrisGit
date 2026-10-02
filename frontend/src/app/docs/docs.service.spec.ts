import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { DocsService, firstDocsPage, isNotFound, locateDocsPage } from './docs.service';
import { DOCS_INDEX } from './docs-fixtures';

describe('DocsService', () => {
  function setup() {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting()] });
    return { docs: TestBed.inject(DocsService), http: TestBed.inject(HttpTestingController) };
  }

  afterEach(() => TestBed.inject(HttpTestingController).verify());

  it('reads /docs/index.json once, then answers from memory', () => {
    const { docs, http } = setup();
    const answers: unknown[] = [];

    docs.index().subscribe((index) => answers.push(index));
    http.expectOne('/docs/index.json').flush(DOCS_INDEX);
    docs.index().subscribe((index) => answers.push(index));

    expect(answers).toEqual([DOCS_INDEX, DOCS_INDEX]);
  });

  it('asks for the index again after a failure', () => {
    const { docs, http } = setup();
    let failed = false;

    docs.index().subscribe({ error: () => (failed = true) });
    http.expectOne('/docs/index.json').flush('down', { status: 503, statusText: 'Unavailable' });
    docs.index().subscribe();
    http.expectOne('/docs/index.json').flush(DOCS_INDEX);

    expect(failed).toBe(true);
  });

  it('reads a page as text from /docs/<section>/<page>.md, once', () => {
    const { docs, http } = setup();
    const answers: string[] = [];

    docs.page('ci-cd', 'caches').subscribe((text) => answers.push(text));
    const request = http.expectOne('/docs/ci-cd/caches.md');
    expect(request.request.responseType).toBe('text');
    request.flush('# Caches\n');
    docs.page('ci-cd', 'caches').subscribe((text) => answers.push(text));

    expect(answers).toEqual(['# Caches\n', '# Caches\n']);
  });

  it('counts the app shell served in place of a missing file as "not found"', () => {
    const { docs, http } = setup();
    let error: unknown;

    docs.page('ci-cd', 'disparue').subscribe({ error: (e) => (error = e) });
    http.expectOne('/docs/ci-cd/disparue.md').flush('<!doctype html>\n<html lang="fr"><body><fg-root></fg-root></body></html>');

    expect(isNotFound(error)).toBe(true);
  });
});

describe('docs index helpers', () => {
  it('starts the documentation at the first page of the first section', () => {
    expect(firstDocsPage(DOCS_INDEX)).toEqual(['/docs', 'demarrer', 'presentation']);
    expect(firstDocsPage({ sections: [] })).toBeNull();
  });

  it('finds a page with its neighbours, across sections', () => {
    const location = locateDocsPage(DOCS_INDEX, 'ci-cd', 'premiers-pas')!;

    expect(location.section.title).toBe('CI/CD');
    expect(location.page.title).toBe('Premiers pas');
    expect(location.previous).toEqual({ sectionTitle: 'Démarrer', title: 'Prise en main', commands: ['/docs', 'demarrer', 'prise-en-main'] });
    expect(location.next).toEqual({ sectionTitle: 'CI/CD', title: 'Référence de .ferrisgit-ci.yml', commands: ['/docs', 'ci-cd', 'reference-yaml'] });
  });

  it('has no neighbour before the first page nor after the last', () => {
    expect(locateDocsPage(DOCS_INDEX, 'demarrer', 'presentation')!.previous).toBeNull();
    expect(locateDocsPage(DOCS_INDEX, 'administration', 'installation')!.next).toBeNull();
  });

  it('knows no page outside the index, nor a page under the wrong section', () => {
    expect(locateDocsPage(DOCS_INDEX, 'ci-cd', 'inconnue')).toBeNull();
    expect(locateDocsPage(DOCS_INDEX, 'demarrer', 'reference-yaml')).toBeNull();
    expect(locateDocsPage(DOCS_INDEX, null, null)).toBeNull();
  });
});
