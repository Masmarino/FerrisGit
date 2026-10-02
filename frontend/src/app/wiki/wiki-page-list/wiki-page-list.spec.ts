import { TestBed } from '@angular/core/testing';
import { provideRouter, Router } from '@angular/router';
import { NEVER, of, throwError } from 'rxjs';
import { By } from '@angular/platform-browser';
import { EmptyState } from '@masmarino/gabarit';
import { WikiPageList } from './wiki-page-list';
import { WikiList, WikiService } from '../wiki.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { PageTitleService } from '../../shell/page-title.service';

describe('WikiPageList', () => {
  const text = (el: Element | null | undefined) => (el?.textContent ?? '').replace(/\s+/g, ' ').trim();

  function setup(role: 'owner' | 'reader' | 'contributor' | 'maintainer' = 'reader', list: WikiList = { headSha: null, pages: [] }) {
    const wikiStub = { list: vi.fn(() => of<WikiList>(list)) };
    TestBed.configureTestingModule({
      providers: [provideRouter([]), { provide: WikiService, useValue: wikiStub }],
    });
    const fixture = TestBed.createComponent(WikiPageList);
    fixture.componentRef.setInput('repositoryId', 'r1');
    fixture.componentRef.setInput('path', ['alice', 'hello']);
    TestBed.inject(RepositoryContextService).current.set({ repositoryId: 'r1', path: ['alice', 'hello'], role, ancestors: [], groupId: null });
    fixture.detectChanges();
    return { fixture, component: fixture.componentInstance, wikiStub, el: fixture.nativeElement as HTMLElement };
  }

  const TWO_PAGES: WikiList = {
    headSha: 'abc123',
    pages: [
      { slug: 'Getting-Started', title: 'Getting Started' },
      { slug: 'FAQ', title: 'Frequently Asked Questions' },
    ],
  };

  it('does not show the "new page" link to a reader', () => {
    const { fixture } = setup('reader');
    expect(fixture.nativeElement.querySelector('.wiki-page-list__new-page-link')).toBeFalsy();
    expect(fixture.nativeElement.querySelector('.wiki-nav__new')).toBeFalsy();
  });

  it('shows the "new page" link to a contributor', () => {
    const { component, el } = setup('contributor', TWO_PAGES);
    expect(component.canManage()).toBe(true);
    expect(text(el.querySelector('.wiki-nav__new'))).toBe('Nouvelle page');
  });

  it('lists pages returned by the service', () => {
    const { fixture, component, wikiStub } = setup('reader');
    wikiStub.list.mockReturnValue(of({ headSha: 'abc123', pages: [{ slug: 'Getting-Started', title: 'Getting Started' }] }));
    component.refresh();
    fixture.detectChanges();
    const card = fixture.nativeElement.querySelector('gbt-list-card');
    expect(card.querySelector('.gbt-list-card').getAttribute('data-state')).toBe('ready');
    expect(card.textContent).toContain('Getting Started');
    const tile = card.querySelector('a.wiki-page-list__tile');
    expect(tile?.querySelector('gbt-icon-marker .gbt-icon-marker')?.getAttribute('data-shape')).toBe('tile');
    expect(tile?.querySelector('gbt-icon-marker .gbt-icon-marker')?.getAttribute('aria-hidden')).toBe('true');
  });

  it('is built on the wiki shell (wide layout, pages nav) instead of a bare .container-wide', () => {
    const { el } = setup('reader', TWO_PAGES);
    expect(el.querySelector('fg-wiki-layout gbt-page-layout[data-width="wide"]')).toBeTruthy();
    expect(el.querySelector('.container-wide')).toBeNull();
  });

  it('shows a busy skeleton card, with a polite status beside it, until the wiki arrives', () => {
    const wikiStub = { list: vi.fn(() => NEVER) };
    TestBed.configureTestingModule({ providers: [provideRouter([]), { provide: WikiService, useValue: wikiStub }] });
    const fixture = TestBed.createComponent(WikiPageList);
    fixture.componentRef.setInput('repositoryId', 'r1');
    fixture.componentRef.setInput('path', ['alice', 'hello']);
    fixture.detectChanges();

    const card = (fixture.nativeElement as HTMLElement).querySelector('gbt-list-card')!;
    expect(card.querySelector('.gbt-list-card')!.getAttribute('data-state')).toBe('loading');
    const busy = card.querySelector('[aria-busy="true"]')!;
    expect(busy.querySelectorAll('gbt-skeleton').length).toBeGreaterThan(0);
    expect(text(card.querySelector('[role="status"]'))).toBe('Chargement du wiki…');
    expect(busy.querySelector('[role="status"]')).toBeNull();
  });

  it('shows the illustrated empty state (with the create-first-page action) when there are no pages', () => {
    const { fixture } = setup('contributor');
    expect(fixture.nativeElement.querySelector('gbt-list-card .gbt-list-card').getAttribute('data-state')).toBe('empty');
    expect(fixture.debugElement.query(By.directive(EmptyState)).componentInstance.illustration()).toBe('book');
    expect(fixture.nativeElement.querySelector('gbt-empty-state')).toBeTruthy();
    expect(fixture.nativeElement.textContent).toContain('Aucune page pour l\'instant');
    expect(fixture.nativeElement.querySelector('.wiki-page-list__new-page-link')).toBeTruthy();
  });

  it('the empty state\'s action is the page\'s only primary button, leading to the creation form', () => {
    const { el } = setup('contributor');
    const router = TestBed.inject(Router);
    const navigate = vi.spyOn(router, 'navigate').mockResolvedValue(true);
    const action = el.querySelector<HTMLElement>('.wiki-page-list__new-page-link')!;
    expect(text(action)).toBe('Créer la première page');
    expect(el.querySelectorAll('.gbt-button--primary').length).toBe(1);
    action.querySelector('button')!.click();
    expect(navigate).toHaveBeenCalledWith(['/repositories', 'alice', 'hello', '-', 'wiki', 'new']);
  });

  it('filters pages by title via the pages search', () => {
    const { fixture, wikiStub } = setup('reader');
    wikiStub.list.mockReturnValue(of(TWO_PAGES));
    fixture.componentInstance.refresh();
    fixture.detectChanges();

    const input: HTMLInputElement = fixture.nativeElement.querySelector('.wiki-nav__search input[type="text"]');
    input.value = 'faq';
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();

    const navText = fixture.nativeElement.querySelector('.wiki-nav__list').textContent as string;
    expect(navText).toContain('Frequently Asked Questions');
    expect(navText).not.toContain('Getting Started');
  });

  it('heads the page "Wiki" with the page count, and lists every page in the index card, sorted by title', () => {
    const { el } = setup('reader', TWO_PAGES);
    expect(text(el.querySelector('h1'))).toBe('Wiki');
    expect(text(el.querySelector('.gbt-page-header__meta'))).toBe('2 pages');
    const rows = Array.from(el.querySelectorAll<HTMLAnchorElement>('.wiki-page-list__items > li a'));
    expect(rows.map((a) => text(a))).toEqual(['Frequently Asked Questions', 'Getting Started']);
    expect(rows[0].getAttribute('href')).toBe('/repositories/alice/hello/-/wiki/FAQ');
  });

  it('writes a single page in the singular', () => {
    const { el } = setup('reader', { headSha: 'h', pages: [{ slug: 'Home', title: 'Home' }] });
    expect(text(el.querySelector('.gbt-page-header__meta'))).toBe('1 page');
  });

  it('says the wiki could not be loaded instead of calling it empty', () => {
    const { el, wikiStub, component, fixture } = setup('contributor', TWO_PAGES);
    wikiStub.list.mockReturnValue(throwError(() => ({ status: 500 })));
    component.refresh();
    fixture.detectChanges();
    expect(el.querySelector('gbt-empty-state')).toBeNull();
    expect(el.querySelector('gbt-list-card')).toBeNull();
    // The wiki nav announces the same failed request, so the page's own notice stays silent.
    const notice = el.querySelector('.wiki-page-list__load-error .gbt-alert');
    expect(text(notice)).toBe('Le wiki n’a pas pu être chargé.');
    expect(notice?.getAttribute('role')).toBeNull();
    expect(notice?.getAttribute('aria-live')).toBeNull();
  });

  it('sets the browser title', () => {
    setup('reader', TWO_PAGES);
    expect(TestBed.inject(PageTitleService).title()).toBe('Wiki');
  });
});
