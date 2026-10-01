import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { provideRouter, Router } from '@angular/router';
import { GbtInput } from '@masmarino/gabarit';
import { of, throwError } from 'rxjs';
import { WikiLayout } from './wiki-layout';
import { WikiList, WikiService } from '../wiki.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';

type Role = 'owner' | 'reader' | 'contributor' | 'maintainer';

const LIST: WikiList = {
  headSha: 'wiki-head',
  pages: [
    { slug: 'Home', title: 'Home' },
    { slug: 'FAQ', title: 'Frequently Asked Questions' },
    { slug: 'Deploiement', title: 'Déploiement en production' },
    { slug: 'Getting-Started', title: 'Getting Started' },
  ],
};

@Component({
  standalone: true,
  imports: [WikiLayout],
  template: `
    <fg-wiki-layout repositoryId="r1" [path]="path" [currentSlug]="currentSlug()" (loaded)="loaded.push($event)" (loadFailed)="failures = failures + 1">
      <p class="host-main">Contenu principal</p>
      @if (withAside()) {
        <div page-aside class="host-aside">Panneau</div>
      }
    </fg-wiki-layout>
  `,
})
class HostView {
  readonly path = ['alice', 'hello'];
  currentSlug = signal<string | null>(null);
  withAside = signal(false);
  loaded: WikiList[] = [];
  failures = 0;
}

describe('WikiLayout', () => {
  const text = (el: Element | null | undefined) => (el?.textContent ?? '').replace(/\s+/g, ' ').trim();

  function setup(options: { role?: Role; list?: WikiList; fail?: boolean; currentSlug?: string | null } = {}) {
    const wikiStub = {
      list: vi.fn(() => (options.fail ? throwError(() => ({ status: 500 })) : of<WikiList>(options.list ?? LIST))),
    };
    TestBed.configureTestingModule({
      providers: [provideRouter([]), { provide: WikiService, useValue: wikiStub }],
    });
    TestBed.inject(RepositoryContextService).current.set({ repositoryId: 'r1', path: ['alice', 'hello'], role: options.role ?? 'reader', ancestors: [], groupId: null });
    const fixture = TestBed.createComponent(HostView);
    fixture.componentInstance.currentSlug.set(options.currentSlug ?? null);
    fixture.detectChanges();
    const el: HTMLElement = fixture.nativeElement;
    return { fixture, el, host: fixture.componentInstance, wikiStub, router: TestBed.inject(Router) };
  }

  const navLinks = (el: HTMLElement) => Array.from(el.querySelectorAll<HTMLAnchorElement>('.wiki-nav__list a'));

  it('builds a wide page layout whose nav landmark is named after the wiki', () => {
    const { el } = setup();
    const layout = el.querySelector('gbt-page-layout')!;
    expect(layout.getAttribute('data-width')).toBe('wide');
    expect(layout.hasAttribute('data-sticky-nav')).toBe(true);
    expect(el.querySelector('nav.gbt-page-layout__nav')?.getAttribute('aria-label')).toBe('Pages du wiki');
    // The layout already wraps [page-nav] in a <nav>: the shell does not nest a second landmark.
    expect(el.querySelectorAll('nav').length).toBe(1);
  });

  it('loads the page list once and emits it for the view', () => {
    const { host, wikiStub } = setup();
    expect(wikiStub.list).toHaveBeenCalledTimes(1);
    expect(wikiStub.list).toHaveBeenCalledWith('r1');
    expect(host.loaded).toEqual([LIST]);
  });

  it('lists every page as a link, sorted by title, under a "Pages" heading with the count', () => {
    const { el } = setup();
    expect(text(el.querySelector('.wiki-nav__heading'))).toBe('Pages 4');
    expect(navLinks(el).map((a) => text(a))).toEqual(['Déploiement en production', 'Frequently Asked Questions', 'Getting Started', 'Home']);
    expect(navLinks(el)[2].getAttribute('href')).toBe('/repositories/alice/hello/-/wiki/Getting-Started');
    expect(el.querySelectorAll('.wiki-nav__list > li').length).toBe(4);
  });

  it('marks the current page with aria-current, and only that one', () => {
    const { el } = setup({ currentSlug: 'FAQ' });
    const current = navLinks(el).filter((a) => a.getAttribute('aria-current') === 'page');
    expect(current.map((a) => text(a))).toEqual(['Frequently Asked Questions']);
  });

  it('marks no page as current on the index', () => {
    const { el } = setup({ currentSlug: null });
    expect(el.querySelector('.wiki-nav__list [aria-current]')).toBeNull();
  });

  it('searches in a field named by a hidden label, with a magnifier inside it', () => {
    const { fixture, el } = setup();
    const search = fixture.debugElement.query(By.css('.wiki-nav__search')).componentInstance as GbtInput;

    expect(search.hideLabel()).toBe(true);
    expect(search.leadingIcon()).toBe('search');
    expect(el.querySelector('.wiki-nav__search label')?.textContent?.trim()).toBe('Rechercher une page');
  });

  it('filters the list by title or slug, ignoring case and accents', () => {
    const { fixture, el } = setup();
    const input = el.querySelector<HTMLInputElement>('.wiki-nav__search input')!;
    expect(el.querySelector('.wiki-nav__search label')?.textContent?.trim()).toBe('Rechercher une page');

    input.value = 'DEPLOIE';
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    expect(navLinks(el).map((a) => text(a))).toEqual(['Déploiement en production']);

    input.value = 'getting-st';
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    expect(navLinks(el).map((a) => text(a))).toEqual(['Getting Started']);
  });

  it('says so when no page matches the search', () => {
    const { fixture, el } = setup();
    const input = el.querySelector<HTMLInputElement>('.wiki-nav__search input')!;
    input.value = 'zzz';
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();
    expect(navLinks(el).length).toBe(0);
    expect(text(el.querySelector('.wiki-nav__empty'))).toBe('Aucune page ne correspond à « zzz ».');
  });

  it('offers "Nouvelle page" (secondary) to writers only, leading to the creation form', () => {
    const reader = setup({ role: 'reader' });
    expect(reader.el.querySelector('.wiki-nav__new')).toBeNull();
    TestBed.resetTestingModule();

    const { el, router } = setup({ role: 'contributor' });
    const navigate = vi.spyOn(router, 'navigate').mockResolvedValue(true);
    const button = el.querySelector<HTMLElement>('.wiki-nav__new')!;
    expect(text(button)).toBe('Nouvelle page');
    expect(button.querySelector('.gbt-button--secondary')).toBeTruthy();
    button.querySelector('button')!.click();
    expect(navigate).toHaveBeenCalledWith(['/repositories', 'alice', 'hello', '-', 'wiki', 'new']);
  });

  it('shows an empty wiki as such, with no search field', () => {
    const { el, host } = setup({ list: { headSha: null, pages: [] } });
    expect(text(el.querySelector('.wiki-nav__empty'))).toBe('Aucune page pour l’instant.');
    expect(el.querySelector('.wiki-nav__search')).toBeNull();
    expect(host.loaded).toEqual([{ headSha: null, pages: [] }]);
  });

  it('explains a failed load in the nav and tells the view', () => {
    const { el, host } = setup({ fail: true });
    const failed = el.querySelector('gbt-alert .gbt-alert');
    expect(text(failed)).toBe('Les pages n’ont pas pu être chargées.');
    expect(failed?.getAttribute('data-variant')).toBe('error');
    // No other view announces the nav's failure (detail, edit): the nav does, once.
    expect(failed?.getAttribute('role')).toBe('alert');
    // Outside the body: the body is folded away (display: none) on narrow layouts, and an alert in it is never announced.
    expect(failed?.closest('.wiki-nav__body')).toBeNull();
    expect(host.failures).toBe(1);
    expect(host.loaded).toEqual([]);
  });

  it('reload() fetches the list again', () => {
    const { fixture, wikiStub, el } = setup();
    wikiStub.list.mockReturnValue(of({ headSha: 'x', pages: [{ slug: 'Only', title: 'Only' }] }));
    fixture.debugElement.children[0].componentInstance.reload();
    fixture.detectChanges();
    expect(wikiStub.list).toHaveBeenCalledTimes(2);
    expect(navLinks(el).map((a) => text(a))).toEqual(['Only']);
  });

  it('projects the view into the main column and its [page-aside] into the aside column', () => {
    const { fixture, el, host } = setup();
    expect(el.querySelector('.gbt-page-layout__main .host-main')).toBeTruthy();
    expect(el.querySelector('.gbt-page-layout__aside')!.children.length).toBe(0);

    host.withAside.set(true);
    fixture.detectChanges();
    expect(el.querySelector('.gbt-page-layout__aside > .host-aside')).toBeTruthy();
  });

  it('folds the list behind one toggle on narrow layouts, whose label follows the state', () => {
    const { fixture, el } = setup();
    const nav = el.querySelector('.wiki-nav')!;
    const toggle = el.querySelector<HTMLElement>('.wiki-nav__toggle')!;
    const button = toggle.querySelector('button')!;
    expect(nav.classList).not.toContain('wiki-nav--open');
    expect(text(toggle)).toBe('Afficher les pages');

    button.click();
    fixture.detectChanges();
    expect(nav.classList).toContain('wiki-nav--open');
    expect(el.querySelector('.wiki-nav__toggle button')).toBe(button);
    expect(text(toggle)).toBe('Masquer les pages');
  });
});
