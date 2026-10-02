import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { By } from '@angular/platform-browser';
import { ActivatedRoute, convertToParamMap, provideRouter, Router } from '@angular/router';
import { BehaviorSubject } from 'rxjs';
import { WorkspacePage } from './workspace-page';
import { GbtToastService } from '@masmarino/gabarit';
import { CreateGroupModal } from '../../groups/create-group-modal/create-group-modal';
import { CreateRepositoryModal } from '../create-repository-modal/create-repository-modal';
import { repositoryFixture } from '../repository-fixtures';

const text = (el: Element | null | undefined) => (el?.textContent ?? '').replace(/\s+/g, ' ').trim();

const OWNED = repositoryFixture({ id: 'r1', visibility: 'public', path: ['alice', 'mine'] });
const SHARED = repositoryFixture({ id: 'r2', role: 'contributor', createdAt: '2026-01-02T00:00:00Z', path: ['bob', 'shared'] });
const GROUPS = [
  { id: 'g1', path: 'acme', role: 'maintainer' },
  { id: 'g2', path: 'acme/backend', role: 'reader' },
];

describe('WorkspacePage', () => {
  function setup(initialTab: string | null = null) {
    const queryParamMap = new BehaviorSubject(convertToParamMap(initialTab ? { tab: initialTab } : {}));
    TestBed.configureTestingModule({
      providers: [
        provideHttpClient(),
        provideHttpClientTesting(),
        provideRouter([]),
        { provide: ActivatedRoute, useValue: { queryParamMap } },
      ],
    });
    const fixture = TestBed.createComponent(WorkspacePage);
    const http = TestBed.inject(HttpTestingController);
    return { fixture, http, queryParamMap };
  }

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  it('fetches both repositories and member groups for the default Tous tab', () => {
    const { fixture, http } = setup();
    fixture.detectChanges();

    http.expectOne('/api/repositories').flush([]);
    http.expectOne('/api/groups/member').flush([]);
  });

  it('does not fetch Mes dépôts or Favoris data until those tabs are activated', () => {
    const { fixture, http } = setup();
    fixture.detectChanges();
    http.expectOne('/api/repositories').flush([]);
    http.expectOne('/api/groups/member').flush([]);

    http.expectNone((req) => req.params.get('starred') === 'true');
  });

  it('loads the Favoris tab with the starred filter when it becomes active via the tab query param', () => {
    const { fixture, http, queryParamMap } = setup();
    fixture.detectChanges();
    http.expectOne('/api/repositories').flush([]);
    http.expectOne('/api/groups/member').flush([]);

    queryParamMap.next(convertToParamMap({ tab: 'starred' }));
    fixture.detectChanges();

    const req = http.expectOne((r) => r.url === '/api/repositories' && r.params.get('starred') === 'true');
    req.flush([repositoryFixture({ id: 'r1', role: 'reader', visibility: 'public', path: ['alice', 'starred-one'] })]);
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('starred-one');
  });

  it('does not refetch a tab that was already loaded when returning to it', () => {
    const { fixture, http, queryParamMap } = setup();
    fixture.detectChanges();
    http.expectOne('/api/repositories').flush([]);
    http.expectOne('/api/groups/member').flush([]);

    queryParamMap.next(convertToParamMap({ tab: 'groups' }));
    fixture.detectChanges();
    http.expectOne('/api/groups/member').flush([]);

    queryParamMap.next(convertToParamMap({}));
    fixture.detectChanges();
    queryParamMap.next(convertToParamMap({ tab: 'groups' }));
    fixture.detectChanges();

    http.expectNone('/api/groups/member');
  });

  it('filters "Mes dépôts" to repositories the caller owns', () => {
    const { fixture, http, queryParamMap } = setup();
    fixture.detectChanges();
    http.expectOne('/api/repositories').flush([]);
    http.expectOne('/api/groups/member').flush([]);

    queryParamMap.next(convertToParamMap({ tab: 'mine' }));
    fixture.detectChanges();
    http.expectOne('/api/repositories').flush([
      repositoryFixture({ id: 'r1', visibility: 'public', path: ['alice', 'mine'] }),
      repositoryFixture({ id: 'r2', role: 'contributor', visibility: 'public', path: ['bob', 'shared'] }),
    ]);
    fixture.detectChanges();

    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('mine');
    expect(text).not.toContain('shared');
  });

  // `forkJoin` cancels the sibling request once one source errors. Whether that shows up as cancelled or
  // still open is an implementation detail, so flush it only if it is still live.
  function failAllTab(http: HttpTestingController): void {
    const reposReq = http.expectOne('/api/repositories');
    const groupsReq = http.expectOne('/api/groups/member');
    reposReq.flush({ error: 'boom' }, { status: 500, statusText: 'Internal Server Error' });
    if (!groupsReq.cancelled) {
      groupsReq.flush([]);
    }
  }

  it('shows an error toast on the Tous tab when loading repositories fails', () => {
    const { fixture, http } = setup();
    const showSpy = vi.spyOn(TestBed.inject(GbtToastService), 'show');
    fixture.detectChanges();

    failAllTab(http);
    fixture.detectChanges();

    expect(showSpy).toHaveBeenCalledWith('Impossible de charger les dépôts. Réessayez plus tard.', 'error');
  });

  it('does not show a further error toast on the Tous tab once a reload succeeds', () => {
    const { fixture, http } = setup();
    const showSpy = vi.spyOn(TestBed.inject(GbtToastService), 'show');
    fixture.detectChanges();

    failAllTab(http);
    fixture.detectChanges();
    expect(showSpy).toHaveBeenCalledTimes(1);

    fixture.componentInstance.onRepoCreated();
    http.expectOne('/api/repositories').flush([]);
    http.expectOne('/api/groups/member').flush([]);
    fixture.detectChanges();

    expect(showSpy).toHaveBeenCalledTimes(1);
  });

  it('shows an error toast on the Groupes tab when loading groups fails', () => {
    const { fixture, http, queryParamMap } = setup();
    const showSpy = vi.spyOn(TestBed.inject(GbtToastService), 'show');
    fixture.detectChanges();
    http.expectOne('/api/repositories').flush([]);
    http.expectOne('/api/groups/member').flush([]);

    queryParamMap.next(convertToParamMap({ tab: 'groups' }));
    fixture.detectChanges();
    http.expectOne('/api/groups/member').flush({ error: 'boom' }, { status: 500, statusText: 'Internal Server Error' });
    fixture.detectChanges();

    expect(showSpy).toHaveBeenCalledWith('Impossible de charger les groupes. Réessayez plus tard.', 'error');
  });

  it('reloads the active tab after a repository is created', () => {
    const { fixture, http } = setup();
    fixture.detectChanges();
    http.expectOne('/api/repositories').flush([]);
    http.expectOne('/api/groups/member').flush([]);

    fixture.componentInstance.onRepoCreated();

    http.expectOne('/api/repositories').flush([]);
    http.expectOne('/api/groups/member').flush([]);
  });

  describe('after a change, every tab loaded so far is refreshed', () => {
    function loadedAllAndMine() {
      const ctx = setup();
      ctx.fixture.detectChanges();
      ctx.http.expectOne('/api/repositories').flush([OWNED, SHARED]);
      ctx.http.expectOne('/api/groups/member').flush(GROUPS);
      ctx.queryParamMap.next(convertToParamMap({ tab: 'mine' }));
      ctx.fixture.detectChanges();
      ctx.http.expectOne('/api/repositories').flush([OWNED]);
      ctx.queryParamMap.next(convertToParamMap({}));
      ctx.fixture.detectChanges();
      ctx.fixture.detectChanges();
      return { ...ctx, el: ctx.fixture.nativeElement as HTMLElement };
    }
    const tabLabels = (el: HTMLElement) => Array.from(el.querySelectorAll('.workspace-page__tabs [role="radio"]'), (b) => text(b));

    it('refreshes "Mes dépôts" too when a repository is created from "Tous"', () => {
      const { fixture, http, queryParamMap, el } = loadedAllAndMine();
      expect(tabLabels(el)[1]).toBe('Mes dépôts (1)');

      fixture.componentInstance.onRepoCreated();
      const repoRequests = http.match('/api/repositories');
      expect(repoRequests.length).toBe(2);
      http.expectOne('/api/groups/member').flush(GROUPS);
      const created = { ...OWNED, id: 'r3', name: 'fresh', path: ['alice', 'fresh'] };
      repoRequests.forEach((r) => r.flush([OWNED, created, SHARED]));
      fixture.detectChanges();

      expect(tabLabels(el)[1]).toBe('Mes dépôts (2)');
      queryParamMap.next(convertToParamMap({ tab: 'mine' }));
      fixture.detectChanges();
      expect(el.textContent).toContain('fresh');
    });

    it('refreshes "Mes dépôts" when a repository is deleted under "Tous", without touching unloaded tabs', () => {
      const { fixture, http, el } = loadedAllAndMine();
      const grid = fixture.debugElement.query(By.css('fg-workspace-grid')).componentInstance as { changed: { emit(): void } };

      grid.changed.emit();
      const repoRequests = http.match('/api/repositories');
      expect(repoRequests.length).toBe(2);
      expect(repoRequests.some((r) => r.request.params.get('starred') === 'true')).toBe(false);
      http.expectOne('/api/groups/member').flush(GROUPS);
      repoRequests.forEach((r) => r.flush([SHARED]));
      fixture.detectChanges();

      expect(tabLabels(el)).toEqual(['Tous (3)', 'Mes dépôts (0)', 'Favoris', 'Groupes (2)']);
    });
  });

  describe('the redesigned page', () => {
    const tabLabels = (el: HTMLElement) => Array.from(el.querySelectorAll('.workspace-page__tabs [role="radio"]'), (b) => text(b));

    function loadedAll(options: { initialTab?: string | null } = {}) {
      const ctx = setup(options.initialTab ?? null);
      ctx.fixture.detectChanges();
      ctx.http.expectOne('/api/repositories').flush([OWNED, SHARED]);
      ctx.http.expectOne('/api/groups/member').flush(GROUPS);
      ctx.fixture.detectChanges();
      return { ...ctx, el: ctx.fixture.nativeElement as HTMLElement };
    }

    it('titles the page with a single h1 and offers exactly one primary action, "Nouveau dépôt", next to a secondary "Nouveau groupe"', () => {
      const { el } = loadedAll();
      expect(Array.from(el.querySelectorAll('h1'), (h) => text(h))).toEqual(['Dépôts']);
      const primaries = Array.from(el.querySelectorAll('.gbt-button--primary'));
      expect(primaries.map((b) => text(b))).toEqual(['Nouveau dépôt']);
      const actions = Array.from(el.querySelectorAll('.gbt-page-header__actions button'), (b) => text(b));
      expect(actions).toEqual(['Nouveau groupe', 'Nouveau dépôt']);
    });

    it('shows the scope tabs in the list card, with the counts it already knows', () => {
      const { el } = loadedAll();
      // The "Tous" tab adds up 2 repositories and 2 groups, and "Mes dépôts" and "Groupes" are counted from the same
      // responses. "Favoris" needs its own request, so it shows no count until it is opened.
      expect(tabLabels(el)).toEqual(['Tous (4)', 'Mes dépôts (1)', 'Favoris', 'Groupes (2)']);
      expect(el.querySelector('.workspace-page__tabs [aria-checked="true"]')?.textContent).toContain('Tous');
    });

    it('fills the Favoris count once that tab has loaded', () => {
      const { fixture, http, queryParamMap, el } = loadedAll();
      queryParamMap.next(convertToParamMap({ tab: 'starred' }));
      fixture.detectChanges();
      http.expectOne((r) => r.params.get('starred') === 'true').flush([SHARED]);
      fixture.detectChanges();

      expect(tabLabels(el)).toEqual(['Tous (4)', 'Mes dépôts (1)', 'Favoris (1)', 'Groupes (2)']);
      expect(el.querySelector('.workspace-page__tabs [aria-checked="true"]')?.textContent).toContain('Favoris');
    });

    it('deep-links the picked tab through ?tab= (and drops it for Tous)', () => {
      const { fixture, el } = loadedAll();
      const navigate = vi.spyOn(TestBed.inject(Router), 'navigate').mockResolvedValue(true);

      Array.from(el.querySelectorAll<HTMLButtonElement>('.workspace-page__tabs [role="radio"]'))
        .find((b) => text(b).startsWith('Groupes'))!
        .click();
      fixture.detectChanges();
      expect(navigate).toHaveBeenLastCalledWith([], expect.objectContaining({ queryParams: { tab: 'groups' }, queryParamsHandling: 'merge' }));

      Array.from(el.querySelectorAll<HTMLButtonElement>('.workspace-page__tabs [role="radio"]'))
        .find((b) => text(b).startsWith('Tous'))!
        .click();
      expect(navigate).toHaveBeenLastCalledWith([], expect.objectContaining({ queryParams: { tab: null } }));
    });

    it('lists the active tab only: "Mes dépôts" shows the owned repositories, no group', () => {
      const { fixture, http, queryParamMap, el } = loadedAll();
      queryParamMap.next(convertToParamMap({ tab: 'mine' }));
      fixture.detectChanges();
      http.expectOne('/api/repositories').flush([OWNED, SHARED]);
      fixture.detectChanges();

      const titles = Array.from(el.querySelectorAll('.workspace-grid__items .gbt-list-row__title > a'), (a) => text(a));
      expect(titles).toEqual(['alice/mine']);
    });

    it('offers the member groups as shortcuts in the aside, with a link to the Groupes tab', () => {
      const { el } = loadedAll();
      const panel = Array.from(el.querySelectorAll<HTMLElement>('[page-aside] gbt-panel')).find((p) => text(p.querySelector('h2')) === 'Groupes')!;
      expect(panel).toBeTruthy();
      const links = Array.from(panel.querySelectorAll<HTMLAnchorElement>('.workspace-page__group-link'));
      expect(links.map((a) => a.getAttribute('href'))).toEqual(['/repositories/acme', '/repositories/acme/backend']);
      const all = panel.querySelector<HTMLAnchorElement>('.workspace-page__all-groups')!;
      expect(all.getAttribute('href')).toBe('/repositories?tab=groups');
    });

    it('has the search and sort panels in the aside once there is something to search', () => {
      const { el } = loadedAll();
      const headings = Array.from(el.querySelectorAll('[page-aside] gbt-panel h2'), (h) => text(h));
      expect(headings).toEqual(['Recherche', 'Trier', 'Groupes']);
      expect(text(el.querySelector('.workspace-grid-filters__search label'))).toBe('Rechercher un dépôt ou un groupe');
    });

    it('drops the aside altogether for a brand-new user with nothing to list', () => {
      const ctx = setup();
      ctx.fixture.detectChanges();
      ctx.http.expectOne('/api/repositories').flush([]);
      ctx.http.expectOne('/api/groups/member').flush([]);
      ctx.fixture.detectChanges();
      const el = ctx.fixture.nativeElement as HTMLElement;

      expect(el.querySelector('[page-aside]')).toBeNull();
      expect(text(el.querySelector('gbt-empty-state'))).toContain("Aucun dépôt ni groupe pour l'instant");
    });

    it('keeps the tabs usable while the active tab is loading', () => {
      const ctx = setup();
      ctx.fixture.detectChanges();
      const el = ctx.fixture.nativeElement as HTMLElement;

      expect(el.querySelectorAll('gbt-list-card gbt-skeleton-list .gbt-skeleton-list__row')).toHaveLength(5);
      // One polite status, outside any aria-busy region (a busy ancestor can hold the announcement back).
      expect(text(el.querySelector('gbt-skeleton-list [role="status"]'))).toBe('Chargement…');
      expect(el.querySelector('[aria-busy="true"]')).toBeNull();
      // The card stays `ready` (its own loading state would remove the header and the tabs in it).
      expect(el.querySelector('gbt-list-card .gbt-list-card')!.getAttribute('data-state')).toBe('ready');
      expect(tabLabels(el)).toEqual(['Tous', 'Mes dépôts', 'Favoris', 'Groupes']);
      ctx.http.expectOne('/api/repositories').flush([]);
      ctx.http.expectOne('/api/groups/member').flush([]);
    });

    it('shows a failed state in the card and loads the tab again on "Réessayer"', () => {
      const ctx = setup();
      ctx.fixture.detectChanges();
      failAllTab(ctx.http);
      ctx.fixture.detectChanges();
      const el = ctx.fixture.nativeElement as HTMLElement;

      const failed = el.querySelector('gbt-list-card gbt-alert .gbt-alert')!;
      expect(failed).not.toBeNull();
      expect(failed.getAttribute('data-variant')).toBe('error');
      // The page's error toast announces it once: the alert in the card is silent.
      expect(failed.getAttribute('role')).toBeNull();
      expect(failed.getAttribute('aria-live')).toBeNull();
      expect(el.querySelector('[role="alert"]')).toBeNull();
      expect(tabLabels(el)).toEqual(['Tous', 'Mes dépôts', 'Favoris', 'Groupes']);
      Array.from(failed.querySelectorAll<HTMLButtonElement>('button'))
        .find((b) => text(b) === 'Réessayer')!
        .click();
      ctx.fixture.detectChanges();
      ctx.http.expectOne('/api/repositories').flush([OWNED]);
      ctx.http.expectOne('/api/groups/member').flush([]);
      ctx.fixture.detectChanges();

      expect(el.querySelector('gbt-list-card gbt-alert')).toBeNull();
      expect(text(el.querySelector('.workspace-grid__items'))).toContain('alice/mine');
    });

    it('opens the creation dialog from the header, rebuilt empty on each opening', () => {
      const { fixture, http, el } = loadedAll();
      const open = () => {
        Array.from(el.querySelectorAll<HTMLButtonElement>('.gbt-page-header__actions button'))
          .find((b) => text(b) === 'Nouveau dépôt')!
          .click();
        fixture.detectChanges();
        http.expectOne('/api/groups/writable').flush([]);
        fixture.detectChanges();
        return fixture.debugElement.query(By.directive(CreateRepositoryModal))?.componentInstance as CreateRepositoryModal;
      };

      const first = open();
      expect(first).toBeTruthy();
      first.name.set('brouillon');
      first.close.emit();
      fixture.detectChanges();
      expect(fixture.debugElement.query(By.directive(CreateRepositoryModal))).toBeNull();

      const second = open();
      expect(second).not.toBe(first);
      expect(second.name()).toBe('');
    });

    describe('"Nouveau groupe"', () => {
      const openGroupDialog = (fixture: { nativeElement: HTMLElement; detectChanges(): void }) => {
        Array.from(fixture.nativeElement.querySelectorAll<HTMLButtonElement>('.gbt-page-header__actions button'))
          .find((b) => text(b) === 'Nouveau groupe')!
          .click();
        fixture.detectChanges();
      };

      it('is offered to any signed-in user and opens the root-group dialog, rebuilt empty on each opening', () => {
        const { fixture } = loadedAll();
        const modal = () => fixture.debugElement.query(By.directive(CreateGroupModal))?.componentInstance as CreateGroupModal;

        openGroupDialog(fixture);
        const first = modal();
        expect(first).toBeTruthy();
        first.name.set('brouillon');
        first.close.emit();
        fixture.detectChanges();
        expect(modal()).toBeUndefined();

        openGroupDialog(fixture);
        expect(modal()).not.toBe(first);
        expect(modal().name()).toBe('');
      });

      it('creates the group at the root, then reloads the lists so that the new group shows up', () => {
        const { fixture, http } = loadedAll();
        openGroupDialog(fixture);
        const modal = fixture.debugElement.query(By.directive(CreateGroupModal)).componentInstance as CreateGroupModal;
        modal.name.set('nouveau');

        modal.submit();

        const create = http.expectOne('/api/groups');
        expect(create.request.method).toBe('POST');
        expect(create.request.body).toEqual({ name: 'nouveau', description: '' });
        create.flush({ id: 'g3', parentGroupId: null, name: 'nouveau', description: '', createdAt: '2026-01-03T00:00:00Z' });
        fixture.detectChanges();

        expect(fixture.debugElement.query(By.directive(CreateGroupModal))).toBeNull();
        http.expectOne('/api/repositories').flush([OWNED, SHARED]);
        http.expectOne('/api/groups/member').flush([...GROUPS, { id: 'g3', path: 'nouveau', role: 'maintainer' }]);
        fixture.detectChanges();
        expect(text(fixture.nativeElement.querySelector('.workspace-grid__items'))).toContain('nouveau');
      });
    });
  });
});
