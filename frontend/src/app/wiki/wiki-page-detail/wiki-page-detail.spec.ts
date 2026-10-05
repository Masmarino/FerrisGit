import { LOCALE_ID } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter, Router } from '@angular/router';
import { of, throwError, Subject } from 'rxjs';
import { WikiPageDetail } from './wiki-page-detail';
import { WikiRevision, WikiService } from '../wiki.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { GbtToastService } from '@masmarino/gabarit/toaster';

type Role = 'owner' | 'reader' | 'contributor' | 'maintainer';

const TWO_DAYS_AGO = new Date(Date.now() - 2 * 24 * 60 * 60 * 1000 - 60_000).toISOString();

const REVISIONS: WikiRevision[] = [
  { commitSha: 'abc1234def5678', authorName: 'Ada', authorEmail: 'ada@example.com', committedAt: TWO_DAYS_AGO, message: 'Create Home' },
  { commitSha: 'fed9876cba5432', authorName: 'Grace', authorEmail: 'grace@example.com', committedAt: '2026-01-01T00:00:00Z', message: 'First draft' },
];

const WITH_HEADINGS = '# Home\n\nIntro.\n\n## Installation\n\nPas à pas.\n\n## Utilisation\n\nAu quotidien.';

describe('WikiPageDetail', () => {
  const text = (el: Element | null | undefined) => (el?.textContent ?? '').replace(/\s+/g, ' ').trim();
  const buttonByText = (root: Element, label: string) => Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find((button) => text(button) === label);

  type Options = { content?: string; revisionsFail?: boolean; detailFails?: boolean; revisions?: WikiRevision[]; revisionsPending?: Subject<WikiRevision[]> };

  function setup(role: Role, showHistory = false, options: Options = {}) {
    const stubs = configure(options);
    return { ...stubs, ...create(role, showHistory) };
  }

  function configure(options: Options) {
    const wikiStub = {
      list: vi.fn(() => of({ headSha: 'wiki-head', pages: [{ slug: 'Home', title: 'Home' }, { slug: 'FAQ', title: 'FAQ' }] })),
      detail: vi.fn(() => (options.detailFails ? throwError(() => ({ status: 404 })) : of({ content: options.content ?? '# Home', headSha: 'abc123', title: 'Home' }))),
      revisions: vi.fn(() => options.revisionsPending ?? (options.revisionsFail ? throwError(() => ({ status: 500 })) : of(options.revisions ?? REVISIONS))),
      revisionContent: vi.fn(() => of({ content: '# old content' })),
      delete: vi.fn(() => of(undefined)),
    };
    const toastStub = { show: vi.fn() };
    TestBed.configureTestingModule({
      providers: [provideRouter([]), { provide: WikiService, useValue: wikiStub }, { provide: GbtToastService, useValue: toastStub }, { provide: LOCALE_ID, useValue: 'fr' }],
    });
    return { wikiStub, toastStub };
  }

  function create(role: Role, showHistory: boolean) {
    const fixture = TestBed.createComponent(WikiPageDetail);
    fixture.componentRef.setInput('repositoryId', 'r1');
    fixture.componentRef.setInput('slug', 'Home');
    fixture.componentRef.setInput('showHistory', showHistory);
    fixture.componentRef.setInput('path', ['alice', 'hello']);
    TestBed.inject(RepositoryContextService).current.set({ repositoryId: 'r1', path: ['alice', 'hello'], role, ancestors: [], groupId: null });
    fixture.detectChanges();
    const router = TestBed.inject(Router);
    return { fixture, component: fixture.componentInstance, router, el: fixture.nativeElement as HTMLElement };
  }

  // Spies (`window.confirm`, `router.navigate`) mustn't outlive their test: spec files share one global scope.
  afterEach(() => {
    vi.restoreAllMocks();
  });

  const headerActions = (el: HTMLElement) => el.querySelector('.gbt-page-header__actions')!;

  it('renders the current page content for a reader, with no edit/delete actions', () => {
    const { fixture, component, el } = setup('reader');
    expect(fixture.nativeElement.textContent).toContain('Home');
    expect(component.canManage()).toBe(false);
    expect(component.canDelete()).toBe(false);
    expect(buttonByText(headerActions(el), 'Modifier')).toBeUndefined();
    expect(buttonByText(headerActions(el), 'Supprimer')).toBeUndefined();
    expect(buttonByText(headerActions(el), 'Historique')).toBeTruthy();
  });

  it('grants edit to a contributor but not delete', () => {
    const { component, el } = setup('contributor');
    expect(component.canManage()).toBe(true);
    expect(component.canDelete()).toBe(false);
    expect(buttonByText(headerActions(el), 'Modifier')).toBeTruthy();
    expect(buttonByText(headerActions(el), 'Supprimer')).toBeUndefined();
  });

  it('grants both edit and delete to a maintainer', () => {
    const { component } = setup('maintainer');
    expect(component.canManage()).toBe(true);
    expect(component.canDelete()).toBe(true);
  });

  it('is built on the wiki shell (wide layout) with this page current in the pages nav', () => {
    const { el } = setup('reader');
    expect(el.querySelector('fg-wiki-layout gbt-page-layout[data-width="wide"]')).toBeTruthy();
    expect(el.querySelector('.container-wide')).toBeNull();
    expect(text(el.querySelector('.wiki-nav__list a[aria-current="page"]'))).toBe('Home');
  });

  describe('page', () => {
    it('titles the page with its h1 and renders the body in a card, headings shifted under the h1', () => {
      const { el } = setup('reader', false, { content: WITH_HEADINGS });
      expect(text(el.querySelector('h1'))).toBe('Home');
      const body = el.querySelector('.wiki-page-detail__card fg-markdown-view')!;
      expect(body).toBeTruthy();
      expect(body.querySelector('h1')).toBeNull();
      expect(text(body.querySelector('h2'))).toBe('Home');
      const host = el.querySelector('article.wiki-page-detail__card gbt-card')!;
      const card = host.querySelector(':scope > .gbt-card')!;
      expect(Array.from(host.children)).toEqual([card]);
      expect(card.getAttribute('data-variant')).toBe('outlined');
      expect(card.hasAttribute('data-flush')).toBe(true);
    });

    it('shows the byline from the newest revision: when, and who', () => {
      const { el, wikiStub } = setup('reader');
      expect(wikiStub.revisions).toHaveBeenCalledTimes(1);
      const byline = el.querySelector('.wiki-page-detail__byline')!;
      expect(text(byline)).toBe('Dernière modification avant-hier par Ada');
      expect(byline.querySelector('time')?.getAttribute('datetime')).toBe(TWO_DAYS_AGO);
      expect(byline.querySelector('time')?.getAttribute('title')).toMatch(/^\d{2}\/\d{2}\/\d{4} \d{2}:\d{2}$/);
    });

    it('hides the byline (and keeps the page) when the revisions cannot be loaded', () => {
      const { el } = setup('reader', false, { revisionsFail: true });
      expect(el.querySelector('.wiki-page-detail__byline')).toBeNull();
      expect(el.querySelector('.wiki-page-detail__card fg-markdown-view')).toBeTruthy();
      expect(el.querySelector('.wiki-page-detail__not-found')).toBeNull();
    });

    it('puts a "Sur cette page" outline in the aside when the page has at least two headings', () => {
      const { fixture, el } = setup('reader', false, { content: WITH_HEADINGS });
      fixture.detectChanges();
      const outline = el.querySelector('.gbt-page-layout__aside fg-wiki-outline')!;
      expect(outline).toBeTruthy();
      expect(Array.from(outline.querySelectorAll('a')).map((a) => text(a))).toEqual(['Home', 'Installation', 'Utilisation']);
    });

    it('leaves the outline out when the page has fewer than two headings', () => {
      const { fixture, el } = setup('reader', false, { content: '# Home\n\nJuste un paragraphe.' });
      fixture.detectChanges();
      expect(el.querySelector('fg-wiki-outline')).toBeNull();
    });

    it('sums up the history in the aside: the latest revisions and a link to all of them', () => {
      const { el } = setup('reader');
      const panel = el.querySelector('.gbt-page-layout__aside .wiki-page-detail__history-panel')!;
      expect(panel).toBeTruthy();
      expect(text(panel.querySelector('h2'))).toBe('Historique');
      expect(Array.from(panel.querySelectorAll('li')).map((li) => text(li.querySelector('.wiki-page-detail__recent-message')))).toEqual(['Create Home', 'First draft']);
      const link = panel.querySelector<HTMLAnchorElement>('a.wiki-page-detail__history-link')!;
      expect(text(link)).toBe('Voir les 2 révisions');
      expect(link.getAttribute('href')).toBe('/repositories/alice/hello/-/wiki/Home/history');
    });

    it('has no aside at all when there is neither an outline nor a history', () => {
      const { fixture, el } = setup('reader', false, { revisionsFail: true });
      fixture.detectChanges();
      expect(el.querySelector('.gbt-page-layout__aside')!.children.length).toBe(0);
    });

    it('header actions: "Modifier" and "Historique" lead to the editor and the history', () => {
      const { el, router } = setup('maintainer');
      const navigate = vi.spyOn(router, 'navigate').mockResolvedValue(true);
      const edit = buttonByText(headerActions(el), 'Modifier')!;
      expect(edit.classList).toContain('gbt-button--secondary');
      edit.click();
      expect(navigate).toHaveBeenLastCalledWith(['/repositories', 'alice', 'hello', '-', 'wiki', 'Home', 'edit']);
      buttonByText(headerActions(el), 'Historique')!.click();
      expect(navigate).toHaveBeenLastCalledWith(['/repositories', 'alice', 'hello', '-', 'wiki', 'Home', 'history']);
      expect(el.querySelectorAll('.gbt-button--primary').length).toBe(0);
    });
  });

  // The router does no anchor scrolling, so a page opened on `…#user-content-installation` scrolls to that heading itself once the markdown has rendered.
  describe('opened on a #fragment', () => {
    const originalScrollIntoView = Object.getOwnPropertyDescriptor(Element.prototype, 'scrollIntoView');
    let scrolled: Element[];

    beforeEach(() => {
      scrolled = [];
      Element.prototype.scrollIntoView = vi.fn(function (this: Element) {
        scrolled.push(this);
      });
    });

    afterEach(() => {
      if (originalScrollIntoView) {
        Object.defineProperty(Element.prototype, 'scrollIntoView', originalScrollIntoView);
      } else {
        delete (Element.prototype as Partial<Element>).scrollIntoView;
      }
    });

    async function openAt(url: string, content = WITH_HEADINGS) {
      configure({ content });
      await TestBed.inject(Router).navigateByUrl(url);
      const opened = create('reader', false);
      await opened.fixture.whenStable();
      opened.fixture.detectChanges();
      return opened;
    }

    it('scrolls to the heading once the markdown has rendered', async () => {
      const { el } = await openAt('/#user-content-installation');

      const heading = el.querySelector('#user-content-installation')!;
      expect(heading.tagName).toBe('H3');
      expect(scrolled).toEqual([heading]);
      expect(Element.prototype.scrollIntoView).toHaveBeenCalledWith({ block: 'start' });
    });

    it('does not scroll without a fragment', async () => {
      await openAt('/');

      expect(scrolled).toEqual([]);
    });

    it('does not scroll for a fragment that names no heading', async () => {
      await openAt('/#user-content-absent');

      expect(scrolled).toEqual([]);
    });

    it('scrolls only once: later renders of the markdown leave the reader where they are', async () => {
      const { fixture, component } = await openAt('/#user-content-installation');
      expect(scrolled).toHaveLength(1);

      (component as unknown as { page: { set(page: object): void } }).page.set({
        content: '## Installation\n\nMis à jour.\n\n## Utilisation',
        headSha: 'def456',
        title: 'Home',
      });
      fixture.detectChanges();
      await fixture.whenStable();
      fixture.detectChanges();

      expect(scrolled).toHaveLength(1);
    });
  });

  describe('delete', () => {
    it('asks for confirmation in a danger modal (no native confirm), then deletes with the page headSha', async () => {
      const { fixture, el, wikiStub, toastStub, router } = setup('maintainer');
      const confirmSpy = vi.spyOn(window, 'confirm');
      const navigate = vi.spyOn(router, 'navigate').mockResolvedValue(true);
      expect(el.querySelector('gbt-confirm-danger-modal')).toBeNull();

      const remove = buttonByText(headerActions(el), 'Supprimer')!;
      expect(remove.classList).toContain('gbt-button--danger');
      remove.click();
      fixture.detectChanges();

      const modal = el.querySelector('gbt-confirm-danger-modal')!;
      expect(modal).toBeTruthy();
      expect(confirmSpy).not.toHaveBeenCalled();
      expect(text(modal.querySelector('.gbt-modal__title'))).toBe('Supprimer la page');
      expect(text(modal)).toContain('« Home »');
      expect(wikiStub.delete).not.toHaveBeenCalled();

      const input = modal.querySelector<HTMLInputElement>('input')!;
      input.value = 'Home';
      input.dispatchEvent(new Event('input'));
      fixture.detectChanges();
      buttonByText(modal, 'Supprimer')!.click();
      fixture.detectChanges();

      expect(wikiStub.delete).toHaveBeenCalledWith('r1', 'Home', 'abc123');
      expect(toastStub.show).toHaveBeenCalledWith('Page wiki supprimée.');
      expect(navigate).toHaveBeenCalledWith(['/repositories', 'alice', 'hello', '-', 'wiki']);
      expect(el.querySelector('gbt-confirm-danger-modal')).toBeNull();
    });

    it('cancelling the modal deletes nothing', () => {
      const { fixture, el, wikiStub } = setup('maintainer');
      buttonByText(headerActions(el), 'Supprimer')!.click();
      fixture.detectChanges();
      buttonByText(el.querySelector('gbt-confirm-danger-modal')!, 'Annuler')!.click();
      fixture.detectChanges();
      expect(el.querySelector('gbt-confirm-danger-modal')).toBeNull();
      expect(wikiStub.delete).not.toHaveBeenCalled();
    });

    it('keeps the page and says so when the delete fails', () => {
      const { fixture, el, wikiStub, toastStub } = setup('maintainer');
      wikiStub.delete.mockReturnValue(throwError(() => ({ status: 409 })));
      fixture.componentInstance.deletePage();
      fixture.detectChanges();
      fixture.componentInstance.confirmDelete();
      fixture.detectChanges();
      expect(toastStub.show).toHaveBeenCalledWith('Impossible de supprimer la page', 'error');
      expect(el.querySelector('gbt-confirm-danger-modal')).toBeNull();
      expect(text(el.querySelector('h1'))).toBe('Home');
    });
  });

  describe('history', () => {
    it('in history mode, fetches revisions instead of the page detail', () => {
      const { wikiStub } = setup('reader', true);
      expect(wikiStub.revisions).toHaveBeenCalledWith('r1', 'Home');
      expect(wikiStub.detail).not.toHaveBeenCalled();
    });

    it('shows a skeleton with its own status region (not inside aria-busy) while the revisions load', () => {
      const pending = new Subject<WikiRevision[]>();
      const { fixture, el } = setup('reader', true, { revisionsPending: pending });

      const status = el.querySelector('[role="status"]');
      expect(text(status)).toBe('Chargement de l’historique…');
      expect(status?.closest('[aria-busy="true"]')).toBeNull();
      expect(el.querySelector('gbt-skeleton-list')).toBeTruthy();

      pending.next(REVISIONS);
      pending.complete();
      fixture.detectChanges();
      expect(el.querySelector('gbt-skeleton-list')).toBeNull();
    });

    it('clicking a revision loads and displays its content inline', () => {
      const { component, wikiStub } = setup('reader', true);
      component.viewRevision('abc123');
      expect(wikiStub.revisionContent).toHaveBeenCalledWith('r1', 'Home', 'abc123');
      expect(component.selectedRevisionContent()).toBe('# old content');
    });

    it('heads the page "Historique de …" with the revision count and a way back to the page', () => {
      const { el, router } = setup('reader', true);
      const navigate = vi.spyOn(router, 'navigate').mockResolvedValue(true);
      expect(text(el.querySelector('h1'))).toBe('Historique de Home');
      expect(text(el.querySelector('.gbt-page-header__meta'))).toBe('2 révisions');
      buttonByText(headerActions(el), 'Retour à la page')!.click();
      expect(navigate).toHaveBeenCalledWith(['/repositories', 'alice', 'hello', '-', 'wiki', 'Home']);
    });

    it('lists the revisions on a rail: avatar, message, author, relative date, short sha, and a button', () => {
      const { el } = setup('reader', true);
      const items = Array.from(el.querySelectorAll<HTMLElement>('ol.wiki-history > li.wiki-history__item'));
      expect(items.length).toBe(2);
      const first = items[0];
      expect(first.querySelector('gbt-avatar.wiki-history__avatar')).toBeTruthy();
      expect(text(first.querySelector('.wiki-history__message'))).toBe('Create Home');
      expect(text(first.querySelector('.wiki-history__author'))).toBe('Ada');
      expect(text(first.querySelector('.wiki-history__meta time'))).toBe('avant-hier');
      expect(text(first.querySelector('.wiki-history__sha'))).toBe('abc1234');
      expect(first.querySelector('.wiki-history__sha .gbt-badge__label')?.getAttribute('title')).toBe('abc1234def5678');
      expect(text(first.querySelector('.wiki-history__current'))).toBe('Version actuelle');
      expect(items[1].querySelector('.wiki-history__current')).toBeNull();
      expect(buttonByText(first, 'Voir cette version')).toBeTruthy();
    });

    it('opens the chosen version in a card under its revision, from one button whose label follows the state', () => {
      const { fixture, el, wikiStub } = setup('reader', true);
      const second = el.querySelectorAll<HTMLElement>('li.wiki-history__item')[1];
      const button = buttonByText(second, 'Voir cette version')!;
      button.click();
      fixture.detectChanges();

      expect(wikiStub.revisionContent).toHaveBeenCalledWith('r1', 'Home', 'fed9876cba5432');
      expect(second.classList).toContain('wiki-history__item--selected');
      const card = second.querySelector('.wiki-history__version')!;
      expect(card).toBeTruthy();
      expect(text(card.querySelector('fg-markdown-view'))).toBe('old content');
      expect(buttonByText(second, 'Masquer cette version')).toBe(button);
      expect(el.querySelectorAll('.wiki-history__version').length).toBe(1);

      button.click();
      fixture.detectChanges();
      expect(second.querySelector('.wiki-history__version')).toBeNull();
      expect(buttonByText(second, 'Voir cette version')).toBe(button);
    });

    it('draws the revisions and the opened version as outlined cards under their headers, the version in a card of its own', () => {
      const { fixture, el } = setup('reader', true);
      const historyHost = el.querySelector('gbt-card.wiki-page-detail__card')!;
      const history = historyHost.querySelector(':scope > .gbt-card')!;
      expect(history.getAttribute('data-variant')).toBe('outlined');
      expect(Array.from(historyHost.children)).toEqual([historyHost.querySelector(':scope > .gbt-card__header'), history]);
      expect(text(historyHost.querySelector(':scope > .gbt-card__header h2'))).toBe('Révisions');

      buttonByText(el.querySelectorAll<HTMLElement>('li.wiki-history__item')[1], 'Voir cette version')!.click();
      fixture.detectChanges();
      // The nested card's own `[card-header]` mustn't take over the history card's header.
      expect(text(historyHost.querySelector(':scope > .gbt-card__header h2'))).toBe('Révisions');
      const versionHost = el.querySelector('.wiki-history__version gbt-card')!;
      const versionHeader = versionHost.querySelector(':scope > .gbt-card__header')!;
      const version = versionHost.querySelector(':scope > .gbt-card')!;
      expect(Array.from(versionHost.children)).toEqual([versionHeader, version]);
      expect(version.getAttribute('data-variant')).toBe('outlined');
      expect(text(versionHeader)).toContain('Version fed9876');
      expect(versionHeader.querySelector('time')).toBeTruthy();
      expect(version.querySelector('fg-markdown-view')).toBeTruthy();
    });

    it('lists the page\'s dates in the aside as an inline description list: created and last modified, with their authors', () => {
      const { el } = setup('reader', true);
      const dates = el.querySelector('gbt-description-list.wiki-page-detail__dates dl')!;
      expect(dates.getAttribute('data-layout')).toBe('inline');
      expect(Array.from(dates.querySelectorAll('dt'), (dt) => text(dt))).toEqual(['Créée', 'Modifiée']);
      const values = Array.from(dates.querySelectorAll('dd'), (dd) => text(dd));
      expect(values[0]).toMatch(/^.+ par Grace$/);
      expect(values[1]).toBe('avant-hier par Ada');
      expect(dates.querySelectorAll('dd time').length).toBe(2);
    });

    it('says when a page has no history', () => {
      const { el } = setup('reader', true, { revisions: [] });
      expect(text(el.querySelector('.wiki-history__empty'))).toBe('Aucun historique.');
    });
  });

  describe('not found', () => {
    it('says the page does not exist, with a way back to the wiki', () => {
      const { el, router } = setup('reader', false, { detailFails: true });
      const navigate = vi.spyOn(router, 'navigate').mockResolvedValue(true);
      const card = el.querySelector('.wiki-page-detail__not-found')!;
      expect(text(card)).toContain('Page introuvable');
      buttonByText(card, 'Retour au wiki')!.click();
      expect(navigate).toHaveBeenCalledWith(['/repositories', 'alice', 'hello', '-', 'wiki']);
      expect(el.querySelector('fg-wiki-layout')).toBeTruthy();
    });

    it('also for the history of a page that does not exist', () => {
      const { el } = setup('reader', true, { revisionsFail: true });
      expect(text(el.querySelector('.wiki-page-detail__not-found'))).toContain('Page introuvable');
    });
  });
});
