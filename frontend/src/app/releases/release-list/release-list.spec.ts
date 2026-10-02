import { LOCALE_ID } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { provideRouter } from '@angular/router';
import { NEVER, Observable, of, throwError } from 'rxjs';
import { ReleaseList, RELEASES_PAGE_SIZE } from './release-list';
import { ReleaseSummary, ReleasesService } from '../releases.service';
import { CreateReleaseModal } from '../create-release-modal/create-release-modal';
import { MergeRequestsService } from '../../merge-requests/merge-requests.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { GbtToastService, formatDateTime, formatRelativeTime } from '@masmarino/gabarit';

const RELATIVE_OPTIONS = { style: 'short', maxUnit: 'day', absoluteAfterDays: 30 } as const;
const ABSOLUTE_OPTIONS = { day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' } as const;
const relativeTime = (iso: string) => formatRelativeTime(iso, 'fr', undefined, RELATIVE_OPTIONS);
const absoluteDateTime = (iso: string) => formatDateTime(iso, 'fr', ABSOLUTE_OPTIONS);

type Role = 'owner' | 'reader' | 'contributor' | 'maintainer';

function release(overrides: Partial<ReleaseSummary> = {}): ReleaseSummary {
  return {
    id: '1',
    tagName: 'v1.0.0',
    title: 'First',
    draft: false,
    prerelease: false,
    authorId: 'a',
    author: null,
    notesExcerpt: '',
    assetCount: 0,
    createdAt: '2026-01-01T00:00:00Z',
    publishedAt: '2026-01-01T00:00:00Z',
    ...overrides,
  };
}

describe('ReleaseList', () => {
  function setup(role: Role = 'reader', list: () => Observable<ReleaseSummary[]> = () => of([])) {
    const releasesStub = {
      list: vi.fn(list),
      listTags: vi.fn(() => of([])),
      create: vi.fn(),
      deleteTag: vi.fn(),
    };
    TestBed.configureTestingModule({
      providers: [
        provideRouter([]),
        { provide: ReleasesService, useValue: releasesStub },
        { provide: MergeRequestsService, useValue: { listBranches: () => of([{ name: 'main', tipSha: 'abc123', isDefault: true }]) } },
        { provide: LOCALE_ID, useValue: 'fr' },
      ],
    });
    const fixture = TestBed.createComponent(ReleaseList);
    fixture.componentRef.setInput('repositoryId', 'r1');
    fixture.componentRef.setInput('path', ['alice', 'hello']);
    TestBed.inject(RepositoryContextService).current.set({ repositoryId: 'r1', path: ['alice', 'hello'], role, ancestors: [], groupId: null });
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;
    return { fixture, component: fixture.componentInstance, releasesStub, el };
  }

  function loaded(releases: ReleaseSummary[], role: Role = 'reader') {
    return setup(role, () => of(releases));
  }

  const text = (el: Element | null | undefined) => {
    if (!el) return '';
    const clone = el.cloneNode(true) as Element;
    clone.querySelectorAll('gbt-avatar').forEach((avatar) => avatar.remove());
    return (clone.textContent ?? '').replace(/\s+/g, ' ').trim();
  };
  const cards = (el: HTMLElement) => Array.from(el.querySelectorAll<HTMLElement>('.release-list__items > li'));
  const cardTitles = (el: HTMLElement) => cards(el).map((card) => text(card.querySelector('.release-card__title a')));
  const buttonByText = (root: Element, label: string) =>
    Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find((button) => text(button) === label);
  const searchInput = (el: HTMLElement) => el.querySelector<HTMLInputElement>('.release-list__search input')!;

  function type(input: HTMLInputElement, value: string, fixture: { detectChanges(): void }) {
    input.value = value;
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();
  }

  describe('page frame', () => {
    it('lays the page out as a wide page layout with the search and sort panels in its aside', () => {
      const { el } = loaded([release()]);

      const layout = el.querySelector('gbt-page-layout');
      expect(layout!.getAttribute('data-width')).toBe('wide');
      expect(layout!.getAttribute('data-aside-width')).toBe('sm');
      expect(el.querySelector('.gbt-page-layout__main .release-list__items')).toBeTruthy();
      const aside = el.querySelector('.gbt-page-layout__aside .release-list__filters')!;
      expect(Array.from(aside.querySelectorAll('.gbt-panel__heading')).map(text)).toEqual(['Recherche', 'Trier']);
    });

    it('titles the page "Releases" in the page header (the page h1)', () => {
      const { el } = loaded([]);
      expect(text(el.querySelector('gbt-page-header h1'))).toBe('Releases');
    });

    it('does not show the "new release" button to a reader', () => {
      const { el } = setup('reader');
      expect(buttonByText(el, 'Nouvelle release')).toBeUndefined();
      expect(el.querySelector('.gbt-button--primary')).toBeNull();
    });

    it('does not show the "new release" button to a contributor (releases are for maintainers)', () => {
      const { el, component } = setup('contributor');
      expect(component.canManage()).toBe(false);
      expect(buttonByText(el, 'Nouvelle release')).toBeUndefined();
    });

    it('shows the "new release" button to a maintainer, as the one primary button of the page', () => {
      const { el, component } = loaded([release()], 'maintainer');

      expect(component.canManage()).toBe(true);
      const primaries = Array.from(el.querySelectorAll('.gbt-button--primary'));
      expect(primaries.length).toBe(1);
      expect(text(primaries[0])).toBe('Nouvelle release');
      expect(primaries[0].closest('.gbt-page-header__actions')).toBeTruthy();
    });

    it('shows the "new release" button to an owner', () => {
      const { el } = loaded([release()], 'owner');
      expect(buttonByText(el.querySelector('.gbt-page-header__actions')!, 'Nouvelle release')).toBeTruthy();
    });
  });

  describe('release cards', () => {
    it('lists releases returned by the service', () => {
      const { fixture, component, releasesStub } = setup('reader');
      releasesStub.list.mockReturnValue(of([release()]));
      component.refresh();
      fixture.detectChanges();
      expect(fixture.nativeElement.textContent).toContain('First');
      expect(fixture.nativeElement.textContent).toContain('v1.0.0');
    });

    it('heads each card with the tag chip, the title linking to the release, and its status', () => {
      const { el } = loaded([release({ tagName: 'v2.1.0', title: 'Version 2.1' })]);

      const card = cards(el)[0];
      const header = card.querySelector('.release-card__header')!;
      expect(text(header.querySelector('.release-card__tag'))).toBe('v2.1.0');
      const link = header.querySelector<HTMLAnchorElement>('.release-card__title a')!;
      expect(text(link)).toBe('Version 2.1');
      expect(link.getAttribute('href')).toBe('/repositories/alice/hello/-/releases/v2.1.0');
      expect(text(header.querySelector('fg-status-badge'))).toBe('Publiée');
    });

    it('draws each release as an outlined card, its header above the box holding the tag, the title and the status', () => {
      const { el } = loaded([release({ tagName: 'v2.1.0', title: 'Version 2.1' })]);

      const card = cards(el)[0].querySelector('gbt-card')!;
      const header = card.querySelector(':scope > .gbt-card__header')!;
      const inner = card.querySelector(':scope > .gbt-card')!;
      expect(Array.from(card.children)).toEqual([header, inner]);
      expect(inner.getAttribute('data-variant')).toBe('outlined');
      expect(header.querySelector('.release-card__tag')).toBeTruthy();
      expect(text(header.querySelector('h2 a'))).toBe('Version 2.1');
      expect(header.querySelector('fg-status-badge')).toBeTruthy();
      expect(inner.querySelector('.gbt-card__body .release-card__excerpt')).toBeTruthy();
    });

    it('shows draft/prerelease as badges', () => {
      const { fixture, releasesStub } = setup('reader');
      releasesStub.list.mockReturnValue(of([release({ tagName: 'v1.0.0-rc1', title: 'Release candidate', prerelease: true, publishedAt: null })]));
      fixture.componentInstance.refresh();
      fixture.detectChanges();

      const badge = fixture.nativeElement.querySelector('.release-card__status gbt-badge');
      expect(badge?.textContent?.trim()).toBe('Pré-version');
    });

    it('reads a draft as "Brouillon", even when it is flagged as a pre-release', () => {
      const { el } = loaded([release({ draft: true, prerelease: true, publishedAt: null })], 'maintainer');
      expect(text(cards(el)[0].querySelector('fg-status-badge'))).toBe('Brouillon');
    });

    it('shows the notes excerpt as plain text, and says so when there are no notes', () => {
      const { el } = loaded([
        release({ id: '1', title: 'Avec notes', notesExcerpt: '## Nouveautés\n\n- **Rapide**', createdAt: '2026-02-01T00:00:00Z', publishedAt: '2026-02-01T00:00:00Z' }),
        release({ id: '2', title: 'Sans notes', notesExcerpt: '', createdAt: '2026-01-01T00:00:00Z', publishedAt: '2026-01-01T00:00:00Z' }),
      ]);

      const [withNotes, withoutNotes] = cards(el);
      expect(text(withNotes.querySelector('.release-card__excerpt'))).toBe('Nouveautés Rapide');
      expect(withNotes.querySelector('.release-card__excerpt--empty')).toBeNull();
      expect(text(withoutNotes.querySelector('.release-card__excerpt--empty'))).toBe('Aucune note de version');
    });

    it('says when and by whom a release was published, with its author chip and file count', () => {
      const publishedAt = '2026-03-01T10:00:00Z';
      const { el } = loaded([release({ author: { id: 'a', username: 'alice' }, assetCount: 3, publishedAt })]);

      const meta = cards(el)[0].querySelector('.release-card__meta')!;
      expect(text(meta)).toBe(`publiée ${relativeTime(publishedAt)} par alice · 3 fichiers`);
      expect(text(meta.querySelector('gbt-user-chip'))).toBe('alice');
      const time = meta.querySelector('time')!;
      expect(time.getAttribute('datetime')).toBe(publishedAt);
      expect(time.getAttribute('title')).toBe(absoluteDateTime(publishedAt));
    });

    it('dates a draft by its creation, says "1 fichier" in the singular, and names a deleted author "Utilisateur supprimé"', () => {
      const createdAt = '2026-02-10T00:00:00Z';
      const { el } = loaded([release({ draft: true, publishedAt: null, createdAt, author: null, assetCount: 1 })], 'maintainer');

      const meta = cards(el)[0].querySelector('.release-card__meta')!;
      expect(text(meta)).toBe(`créée ${relativeTime(createdAt)} par Utilisateur supprimé · 1 fichier`);
      expect(meta.querySelector('gbt-user-chip')).toBeNull();
    });

    it('leaves the file count out when a release has no file', () => {
      const { el } = loaded([release({ assetCount: 0, author: { id: 'a', username: 'alice' } })]);
      expect(text(cards(el)[0].querySelector('.release-card__meta'))).not.toContain('fichier');
    });
  });

  describe('search, sort and pages', () => {
    const three = [
      release({ id: '1', tagName: 'v1.0.0', title: 'First release', createdAt: '2026-01-01T00:00:00Z', publishedAt: '2026-01-01T00:00:00Z' }),
      release({ id: '3', tagName: 'v3.0.0', title: 'Another release', createdAt: '2026-03-01T00:00:00Z', publishedAt: '2026-03-01T00:00:00Z' }),
      release({ id: '2', tagName: 'v2.0.0', title: 'Second release', createdAt: '2026-02-01T00:00:00Z', publishedAt: '2026-02-01T00:00:00Z' }),
    ];

    it('shows the newest release first by default', () => {
      const { el } = loaded(three);
      expect(cardTitles(el)).toEqual(['Another release', 'Second release', 'First release']);
    });

    it('sorts by the date a card shows: an old draft published today comes first, a draft by its creation', () => {
      const { el } = loaded([
        ...three,
        release({ id: '4', tagName: 'v0.9.0', title: 'Old draft published today', createdAt: '2025-12-01T00:00:00Z', publishedAt: '2026-04-01T00:00:00Z' }),
        release({ id: '5', tagName: 'v4.0.0-rc', title: 'Unpublished draft', draft: true, createdAt: '2026-02-15T00:00:00Z', publishedAt: null }),
      ]);
      expect(cardTitles(el)).toEqual(['Old draft published today', 'Another release', 'Unpublished draft', 'Second release', 'First release']);
    });

    it('filters releases by title via the search panel', () => {
      const { fixture, el } = loaded(three);

      type(searchInput(el), 'second', fixture);

      const content = el.textContent as string;
      expect(content).toContain('Second release');
      expect(content).not.toContain('First release');
    });

    it('filters releases by tag name too', () => {
      const { fixture, el } = loaded(three);
      type(searchInput(el), 'v3', fixture);
      expect(cardTitles(el)).toEqual(['Another release']);
    });

    it('says when nothing matches the search, keeping the aside', () => {
      const { fixture, el } = loaded(three);
      type(searchInput(el), 'introuvable', fixture);

      expect(cards(el).length).toBe(0);
      expect(text(el.querySelector('.release-list__no-results'))).toBe('Aucune release ne correspond à cette recherche');
      expect(el.querySelector('.release-list__filters')).toBeTruthy();
    });

    it('sorts by title, and flips the direction', () => {
      const { fixture, el, component } = loaded(three);
      const internals = component as unknown as { sortValue: { set(v: string): void }; direction: { set(v: string): void } };

      internals.sortValue.set('title');
      internals.direction.set('asc');
      fixture.detectChanges();
      expect(cardTitles(el)).toEqual(['Another release', 'First release', 'Second release']);

      internals.direction.set('desc');
      fixture.detectChanges();
      expect(cardTitles(el)).toEqual(['Second release', 'First release', 'Another release']);
    });

    it(`shows ${RELEASES_PAGE_SIZE} releases per page, with a pager when there are more`, () => {
      const many = Array.from({ length: RELEASES_PAGE_SIZE + 3 }, (_, index) =>
        release({ id: `r${index}`, tagName: `v0.${index}.0`, title: `Version ${index}`, createdAt: new Date(Date.UTC(2026, 0, 1, index)).toISOString() }),
      );
      const { fixture, el } = loaded(many);

      expect(cards(el).length).toBe(RELEASES_PAGE_SIZE);
      expect(el.querySelector('gbt-pagination')).toBeTruthy();

      const next = el.querySelector<HTMLButtonElement>('gbt-pagination button[aria-label="Page suivante"]')!;
      next.click();
      fixture.detectChanges();
      expect(cards(el).length).toBe(3);
    });

    it('has no pager for a single page', () => {
      const { el } = loaded(three);
      expect(el.querySelector('gbt-pagination')).toBeNull();
    });
  });

  describe('loading, empty and error states', () => {
    it('shows skeleton cards until the releases arrive', () => {
      const { el } = setup('reader', () => NEVER);

      const loading = el.querySelector('.release-list__loading')!;
      expect(loading.getAttribute('aria-busy')).toBe('true');
      expect(loading.querySelectorAll('gbt-skeleton').length).toBeGreaterThan(0);
      expect(el.querySelector('gbt-empty-state')).toBeNull();
      expect(el.querySelector('.release-list__filters')).toBeNull();
    });

    it('shows the illustrated empty state when there are no releases', () => {
      const { fixture } = setup('reader');
      expect(fixture.nativeElement.querySelector('gbt-empty-state')).toBeTruthy();
      expect(fixture.nativeElement.textContent).toContain('Aucune release pour l\'instant');
    });

    it('gives a reader no action in the empty state, and no aside', () => {
      const { el } = loaded([], 'reader');
      const emptyState = el.querySelector('gbt-empty-state')!;
      expect(emptyState.getAttribute('illustration')).toBe('tag');
      expect(emptyState.querySelector('gbt-button')).toBeNull();
      expect(el.querySelector('.release-list__filters')).toBeNull();
    });

    it('offers a secondary "Créer une release" in the empty state to a maintainer (the header keeps the only primary)', () => {
      const { fixture, el } = loaded([], 'maintainer');

      const create = buttonByText(el.querySelector('gbt-empty-state')!, 'Créer une release')!;
      expect(create.classList).toContain('gbt-button--secondary');
      expect(el.querySelectorAll('.gbt-button--primary').length).toBe(1);

      create.click();
      fixture.detectChanges();
      expect(el.querySelector('fg-create-release-modal')).toBeTruthy();
    });

    it('says the list could not be loaded, with an error toast, when the request fails', () => {
      const { el } = setup('reader', () => throwError(() => new Error('500')));

      expect(TestBed.inject(GbtToastService).toasts().at(-1)).toMatchObject({ message: 'Impossible de charger les releases. Réessayez plus tard.', variant: 'error' });
      const failed = el.querySelector('gbt-alert .gbt-alert');
      expect(text(failed)).toBe("Les releases n'ont pas pu être chargées.");
      // The toast announces it, so the inline message stays silent: one live region, not two.
      expect(failed?.getAttribute('data-variant')).toBe('error');
      expect(failed?.getAttribute('role')).toBeNull();
      expect(failed?.getAttribute('aria-live')).toBeNull();
      expect(el.querySelector('gbt-empty-state')).toBeNull();
      expect(el.querySelector('.release-list__skeleton')).toBeNull();
    });
  });

  describe('creation dialog', () => {
    const dialog = (el: HTMLElement) => el.querySelector<HTMLElement>('[role="dialog"]');

    it('opens the creation dialog from the header button', () => {
      const { fixture, el } = loaded([release()], 'maintainer');
      expect(dialog(el)).toBeNull();

      buttonByText(el.querySelector('.gbt-page-header__actions')!, 'Nouvelle release')!.click();
      fixture.detectChanges();

      expect(dialog(el)?.getAttribute('aria-label')).toBe('Nouvelle release');
    });

    it('drops the draft when the dialog is closed: the next opening starts empty', () => {
      const { fixture, el } = loaded([release()], 'maintainer');
      const open = () => {
        buttonByText(el.querySelector('.gbt-page-header__actions')!, 'Nouvelle release')!.click();
        fixture.detectChanges();
      };
      const titleInput = () => dialog(el)!.querySelector<HTMLInputElement>('.create-release__title input')!;

      open();
      type(titleInput(), 'Brouillon de titre', fixture);
      buttonByText(dialog(el)!, 'Annuler')!.click();
      fixture.detectChanges();
      expect(dialog(el)).toBeNull();

      open();
      expect(titleInput().value).toBe('');
    });

    it('closes the dialog and reloads the list once a release is created', () => {
      const { fixture, el, releasesStub } = loaded([release()], 'maintainer');
      buttonByText(el.querySelector('.gbt-page-header__actions')!, 'Nouvelle release')!.click();
      fixture.detectChanges();
      expect(releasesStub.list).toHaveBeenCalledTimes(1);

      fixture.debugElement.query(By.directive(CreateReleaseModal)).componentInstance.created.emit();
      fixture.detectChanges();

      expect(dialog(el)).toBeNull();
      expect(releasesStub.list).toHaveBeenCalledTimes(2);
    });
  });
});
