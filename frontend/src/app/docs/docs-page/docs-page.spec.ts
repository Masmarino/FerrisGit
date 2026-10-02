import { TestBed } from '@angular/core/testing';
import { provideRouter, Router } from '@angular/router';
import { RouterTestingHarness } from '@angular/router/testing';
import { Observable } from 'rxjs';
import { PageTitleService } from '../../shell/page-title.service';
import { DOCS_ROUTES } from '../docs.routes';
import { DocsIndex, DocsService } from '../docs.service';
import { FAILING_DOCS, fakeDocsService } from '../docs-fixtures';

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();
const wait = (ms = 10) => new Promise<void>((resolve) => setTimeout(resolve, ms));

describe('DocsPage', () => {
  let scrolled: Element[];
  let scrollTo: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    scrolled = [];
    Element.prototype.scrollIntoView = function (this: Element) {
      scrolled.push(this);
    };
    scrollTo = vi.fn();
    Object.defineProperty(window, 'scrollTo', { value: scrollTo, configurable: true, writable: true });
  });

  afterEach(() => {
    delete (Element.prototype as { scrollIntoView?: unknown }).scrollIntoView;
  });

  async function setup(url: string, options: { index?: () => Observable<DocsIndex>; page?: (key: string) => Observable<string> } = {}) {
    TestBed.configureTestingModule({
      providers: [provideRouter([{ path: 'docs', children: DOCS_ROUTES }]), { provide: DocsService, useValue: fakeDocsService(options) }],
    });
    const harness = await RouterTestingHarness.create(url);
    const settle = async () => {
      await wait();
      await harness.fixture.whenStable();
      harness.detectChanges();
    };
    await settle();
    const el = () => harness.routeNativeElement as HTMLElement;
    const go = async (to: string) => {
      await harness.navigateByUrl(to);
      await settle();
    };
    return { harness, el, settle, go, router: TestBed.inject(Router), title: () => TestBed.inject(PageTitleService).title() };
  }

  it('renders the page with its breadcrumb, and names it in the title', async () => {
    const { el, title } = await setup('/docs/ci-cd/reference-yaml');

    const crumbs = Array.from(el().querySelectorAll('.docs-page__breadcrumb li'));
    expect(crumbs.map((li) => text(li))).toEqual(['Documentation', 'CI/CD', 'Référence de .ferrisgit-ci.yml']);
    expect(crumbs[0].querySelector('a')?.getAttribute('href')).toBe('/docs');
    expect(crumbs[1].querySelector('a')?.getAttribute('href')).toBe('/docs/ci-cd/premiers-pas');
    expect(crumbs[2].getAttribute('aria-current')).toBe('page');
    expect(text(el().querySelector('article h1'))).toBe('Référence de .ferrisgit-ci.yml');
    expect(title()).toBe('Référence de .ferrisgit-ci.yml');
  });

  it('keeps the navigation beside it, the current page marked', async () => {
    const { el } = await setup('/docs/ci-cd/reference-yaml');

    expect(el().querySelector('nav[aria-label="Documentation"] fg-docs-nav')).not.toBeNull();
    expect(text(el().querySelector('fg-docs-nav a[aria-current="page"]'))).toBe('Référence de .ferrisgit-ci.yml');
  });

  it('outlines the page in a "Sur cette page" aside', async () => {
    const { el } = await setup('/docs/ci-cd/reference-yaml');

    const aside = el().querySelector('aside[aria-label="Sur cette page"]')!;
    expect(text(aside.querySelector('h2'))).toBe('Sur cette page');
    expect(Array.from(aside.querySelectorAll('a')).map((a) => text(a))).toEqual(['Clés globales', 'Variables', 'Variables prédéfinies', 'Cache']);
  });

  it('turns "Note" and "Attention" quotes into callouts, and leaves other quotes alone', async () => {
    const { el } = await setup('/docs/ci-cd/reference-yaml');

    const callout = el().querySelector('blockquote')!;
    expect(callout.getAttribute('data-callout')).toBe('warning');
    expect(callout.getAttribute('role')).toBe('note');
  });

  it('links the previous and next pages, across sections', async () => {
    const { el } = await setup('/docs/ci-cd/premiers-pas');

    const pager = el().querySelector('nav[aria-label="Pages voisines"]')!;
    expect(Array.from(pager.querySelectorAll('.docs-page__neighbour-label')).map((label) => text(label))).toEqual(['Précédent · Démarrer', 'Suivant · CI/CD']);
    expect(Array.from(pager.querySelectorAll('a')).map((a) => text(a))).toEqual(['Prise en main', 'Référence de .ferrisgit-ci.yml']);
    expect(Array.from(pager.querySelectorAll('a')).map((a) => a.getAttribute('href'))).toEqual(['/docs/demarrer/prise-en-main', '/docs/ci-cd/reference-yaml']);
  });

  it('has no "previous" on the first page', async () => {
    const { el } = await setup('/docs/demarrer/presentation');

    expect(Array.from(el().querySelectorAll('.docs-page__neighbour-label')).map((label) => text(label))).toEqual(['Suivant · Démarrer']);
  });

  it('follows a link to another page through the router, then scrolls to the heading it names', async () => {
    const { el, router, settle } = await setup('/docs/demarrer/presentation');

    el().querySelector<HTMLAnchorElement>('article a[href="/docs/ci-cd/reference-yaml#variables"]')!.click();
    await settle();

    expect(router.url).toBe('/docs/ci-cd/reference-yaml#variables');
    expect(scrolled.map((node) => node.id)).toEqual(['user-content-variables']);
    expect(document.activeElement?.id).toBe('user-content-variables');
  });

  it('scrolls to the heading of the address on first load, without taking the focus', async () => {
    const { el } = await setup('/docs/ci-cd/reference-yaml#user-content-cache');

    expect(scrolled.map((node) => node.id)).toEqual(['user-content-cache']);
    expect(el().contains(document.activeElement)).toBe(false);
  });

  it('starts a new page at its top, its title focused', async () => {
    const { el, go } = await setup('/docs/ci-cd/premiers-pas');

    await go('/docs/ci-cd/reference-yaml');

    expect(document.activeElement).toBe(el().querySelector('article h1'));
    expect(scrollTo).toHaveBeenCalledWith({ top: 0 });
  });

  it('shows its own "not found" in the docs frame for a page outside the index', async () => {
    const { el, title } = await setup('/docs/ci-cd/inconnue');

    expect(text(el().querySelector('h1'))).toBe('Page introuvable');
    expect(el().querySelector('gbt-empty-state a')?.getAttribute('href')).toBe('/docs/demarrer/presentation');
    expect(el().querySelector('fg-docs-nav')).not.toBeNull();
    expect(title()).toBe('Page introuvable');
  });

  it('shows the same "not found" for any other depth under /docs', async () => {
    const { el } = await setup('/docs/a/b/c');

    expect(text(el().querySelector('h1'))).toBe('Page introuvable');
  });

  it('says "not found" when a page of the index has no file', async () => {
    const { el } = await setup('/docs/administration/installation', { page: () => fakeDocsService().page('nulle', 'part') });

    expect(text(el().querySelector('h1'))).toBe('Page introuvable');
  });

  it('offers to try again when the documentation cannot be loaded', async () => {
    let failing = true;
    const { el, settle } = await setup('/docs/ci-cd/reference-yaml', {
      page: (key) => (failing ? FAILING_DOCS() : fakeDocsService().page(...(key.split('/') as [string, string]))),
    });

    expect(text(el().querySelector('gbt-alert'))).toContain('La documentation n’a pas pu être chargée.');
    failing = false;
    el().querySelector<HTMLButtonElement>('gbt-alert button')!.click();
    await settle();

    expect(el().querySelector('gbt-alert')).toBeNull();
    expect(text(el().querySelector('article h1'))).toBe('Référence de .ferrisgit-ci.yml');
  });
});
