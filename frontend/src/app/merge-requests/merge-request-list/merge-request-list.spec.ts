import { LOCALE_ID } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { By } from '@angular/platform-browser';
import { provideRouter } from '@angular/router';
import { EmptyState, GbtToastService, formatDateTime, formatRelativeTime } from '@masmarino/gabarit';
import { MergeRequestList } from './merge-request-list';
import { BranchInfo, MergeRequestSummary } from '../merge-requests.service';
import { mergeRequestFixture } from '../merge-request-fixtures';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { PageTitleService } from '../../shell/page-title.service';

const RELATIVE_OPTIONS = { style: 'short', maxUnit: 'day', absoluteAfterDays: 30 } as const;
const ABSOLUTE_OPTIONS = { day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' } as const;
const relativeTime = (iso: string) => formatRelativeTime(iso, 'fr', undefined, RELATIVE_OPTIONS);
const absoluteDateTime = (iso: string) => formatDateTime(iso, 'fr', ABSOLUTE_OPTIONS);

const LIST_URL = '/api/repositories/repo-1/merge-requests';
const BRANCHES_URL = '/api/repositories/repo-1/branches';

const LABEL_URGENT = { id: 'label-1', name: 'Urgent', color: '#dc2626', repositoryId: 'repo-1', groupId: null, createdAt: '2026-01-01T00:00:00Z' };
const MILESTONE_V1 = { id: 'm1', title: 'v1.0', description: '', dueDate: null, state: 'open', repositoryId: 'repo-1', groupId: null, createdAt: '2026-01-01T00:00:00Z' };

const BRANCHES: BranchInfo[] = [
  { name: 'develop', tipSha: 'ccc3333333', isDefault: false },
  { name: 'main', tipSha: 'aaa1111111', isDefault: true },
  { name: 'feat', tipSha: 'bbb2222222', isDefault: false },
];

function mr(overrides: Partial<MergeRequestSummary> = {}): MergeRequestSummary {
  return mergeRequestFixture({ id: 'mr1', sourceBranch: 'feat', title: 'Add feature', ...overrides });
}

function manyMergeRequests(count: number, status: MergeRequestSummary['status'] = 'open'): MergeRequestSummary[] {
  return Array.from({ length: count }, (_, index) =>
    mr({
      id: `mr${index + 1}`,
      title: `Demande ${index + 1}`,
      status,
      closedAt: status === 'open' ? null : '2026-03-01T00:00:00Z',
      createdAt: new Date(Date.UTC(2026, 0, 1, 0, index)).toISOString(),
    }),
  );
}

const closedVersion = (m: MergeRequestSummary): MergeRequestSummary => ({ ...m, status: 'closed', closedAt: '2026-03-02T00:00:00Z' });
const mergedVersion = (m: MergeRequestSummary): MergeRequestSummary => ({ ...m, status: 'merged', mergeCommitSha: 'abc123', closedAt: '2026-03-02T00:00:00Z' });

type Role = 'owner' | 'contributor' | 'maintainer' | 'reader';

describe('MergeRequestList', () => {
  function setup(options: { role?: Role } = {}) {
    TestBed.configureTestingModule({
      providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([]), { provide: LOCALE_ID, useValue: 'fr' }],
    });
    if (options.role) {
      TestBed.inject(RepositoryContextService).current.set({ repositoryId: 'repo-1', path: ['acme', 'widget'], role: options.role, ancestors: [], groupId: null });
    }
    const fixture = TestBed.createComponent(MergeRequestList);
    fixture.componentRef.setInput('repositoryId', 'repo-1');
    fixture.componentRef.setInput('path', ['acme', 'widget']);
    const http = TestBed.inject(HttpTestingController);
    const el = fixture.nativeElement as HTMLElement;
    return { fixture, http, el };
  }

  function loaded(list: MergeRequestSummary[], options: { role?: Role; labels?: unknown[]; milestones?: unknown[]; branches?: BranchInfo[] } = {}) {
    const ctx = setup(options);
    ctx.fixture.detectChanges();
    ctx.http.expectOne(LIST_URL).flush(list);
    ctx.http.expectOne(BRANCHES_URL).flush(options.branches ?? BRANCHES);
    ctx.http.expectOne('/api/repositories/repo-1/labels').flush(options.labels ?? []);
    ctx.http.expectOne('/api/repositories/repo-1/milestones').flush(options.milestones ?? []);
    ctx.fixture.detectChanges();
    return ctx;
  }

  const text = (el: Element | null | undefined) => (el?.textContent ?? '').replace(/\s+/g, ' ').trim();
  const rows = (el: HTMLElement) => Array.from(el.querySelectorAll<HTMLElement>('gbt-list-card ul > li'));
  const rowTitles = (el: HTMLElement) => rows(el).map((row) => text(row.querySelector('.gbt-list-row__title > a')));
  const buttonByText = (root: Element, label: string) =>
    Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find((button) => text(button) === label);
  const tabButtons = (el: HTMLElement) => Array.from(el.querySelectorAll<HTMLButtonElement>('.merge-request-list__tabs [role="radio"]'));
  const searchInput = (el: HTMLElement) => el.querySelector<HTMLInputElement>('.merge-request-list__search input')!;
  const dialog = () => document.querySelector<HTMLElement>('[role="dialog"]');

  function type(input: HTMLInputElement | HTMLTextAreaElement, value: string, fixture: { detectChanges(): void }) {
    input.value = value;
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();
  }

  function openSelectAndPick(fixture: { detectChanges(): void; nativeElement: HTMLElement }, selectIndex: number, optionLabel: string, root: ParentNode = fixture.nativeElement): void {
    const trigger = root.querySelectorAll<HTMLButtonElement>('.gbt-select__trigger')[selectIndex];
    trigger.click();
    fixture.detectChanges();
    const option = Array.from(document.querySelectorAll<HTMLElement>('.gbt-select__option')).find((el) => el.textContent?.trim() === optionLabel);
    option!.click();
    fixture.detectChanges();
  }

  function openRowMenu(fixture: { detectChanges(): void }, row: HTMLElement): HTMLButtonElement[] {
    row.querySelector<HTMLButtonElement>('.gbt-menu__trigger')!.click();
    fixture.detectChanges();
    return Array.from(row.querySelectorAll<HTMLButtonElement>('[role="menuitem"]'));
  }

  const goToPage = (fixture: { detectChanges(): void }, el: HTMLElement, page: number) => {
    el.querySelector<HTMLButtonElement>(`gbt-pagination [aria-label="Page ${page}"]`)!.click();
    fixture.detectChanges();
  };

  describe('page frame', () => {
    it('lays the page out as a wide page layout with the filters in its aside', () => {
      const { el } = loaded([mr()]);

      const layout = el.querySelector('gbt-page-layout');
      expect(layout).toBeTruthy();
      expect(layout!.getAttribute('data-width')).toBe('wide');
      expect(el.querySelector('.gbt-page-layout__aside .merge-request-list__filters')).toBeTruthy();
      expect(el.querySelector('.gbt-page-layout__main gbt-list-card')).toBeTruthy();
    });

    it('titles the page "Demandes de fusion" in the page header (the page h1) and in the shell', () => {
      const { el } = loaded([]);

      expect(text(el.querySelector('gbt-page-header h1'))).toBe('Demandes de fusion');
      expect(TestBed.inject(PageTitleService).title()).toBe('Demandes de fusion');
    });

    it('offers "Nouvelle demande de fusion" as the one primary button of the page for a writer', () => {
      const { el } = loaded([mr()], { role: 'contributor' });

      const header = el.querySelector('.gbt-page-header__actions')!;
      expect(buttonByText(header, 'Nouvelle demande de fusion')).toBeTruthy();
      const primaries = Array.from(el.querySelectorAll('.gbt-button--primary'));
      expect(primaries.length).toBe(1);
      expect(text(primaries[0])).toBe('Nouvelle demande de fusion');
    });

    it('has no "Nouvelle demande de fusion" button and no row menu for a reader', () => {
      const { el } = loaded([mr()], { role: 'reader' });

      expect(buttonByText(el, 'Nouvelle demande de fusion')).toBeUndefined();
      expect(el.querySelector('.gbt-menu__trigger')).toBeNull();
      expect(el.querySelector('.gbt-button--primary')).toBeNull();
    });
  });

  describe('loading and empty states', () => {
    it('shows skeleton rows until the merge requests arrive, then the rows', () => {
      const { fixture, http, el } = setup();
      fixture.detectChanges();

      const card = el.querySelector('gbt-list-card')!;
      expect(card.querySelector('.gbt-list-card')!.getAttribute('data-state')).toBe('loading');
      const busy = card.querySelector('[aria-busy="true"]')!;
      expect(busy).toBeTruthy();
      expect(busy.querySelectorAll('gbt-skeleton').length).toBeGreaterThan(0);
      expect(text(card.querySelector('[role="status"]'))).toBe('Chargement des demandes de fusion…');
      expect(busy.querySelector('[role="status"]')).toBeNull();
      expect(el.querySelector('gbt-empty-state')).toBeNull();

      http.expectOne(LIST_URL).flush([mr()]);
      http.match(() => true).forEach((req) => req.flush([]));
      fixture.detectChanges();

      expect(el.querySelector('gbt-list-card .gbt-list-card')!.getAttribute('data-state')).toBe('ready');
      expect(el.querySelector('gbt-list-card [aria-busy="true"]')).toBeNull();
      expect(text(el.querySelector('gbt-list-card [role="status"]'))).toBe('');
      expect(rows(el).length).toBe(1);
    });

    it('shows the illustrated empty state, without a create button, to a reader when there are no merge requests', () => {
      const { fixture, el } = loaded([], { role: 'reader' });

      const emptyState = el.querySelector('gbt-empty-state');
      expect(emptyState).toBeTruthy();
      expect(fixture.debugElement.query(By.directive(EmptyState)).componentInstance.illustration()).toBe('merge');
      expect(el.querySelector('gbt-list-card .gbt-list-card')!.getAttribute('data-state')).toBe('empty');
      expect(text(emptyState)).toContain("Aucune demande de fusion pour l'instant");
      expect(emptyState!.querySelector('gbt-button')).toBeNull();
      expect(el.querySelector('.merge-request-list__tabs')).toBeNull();
      expect(el.querySelector('.merge-request-list__filters')).toBeNull();
    });

    it('offers a secondary "Créer une demande de fusion" in the empty state to a writer (the header keeps the only primary)', () => {
      const { fixture, el } = loaded([], { role: 'owner' });

      const emptyState = el.querySelector('gbt-empty-state')!;
      const create = buttonByText(emptyState, 'Créer une demande de fusion');
      expect(create).toBeTruthy();
      expect(create!.classList).toContain('gbt-button--secondary');
      expect(el.querySelectorAll('.gbt-button--primary').length).toBe(1);

      create!.click();
      fixture.detectChanges();
      expect(dialog()).toBeTruthy();
    });

    it('says the list could not be loaded in one alert (no toast on top of it) and no tabs, when the request fails', () => {
      const { fixture, http, el } = setup();
      fixture.detectChanges();
      http.expectOne(LIST_URL).flush('boom', { status: 500, statusText: 'Server Error' });
      fixture.detectChanges();

      const card = el.querySelector('gbt-list-card')!;
      expect(card.querySelector('.gbt-list-card')!.getAttribute('data-state')).toBe('failed');
      // The card's failed block is the announcement: exactly one alert on the page, and the error toast is not fired for it.
      const alerts = el.querySelectorAll('[role="alert"]');
      expect(alerts).toHaveLength(1);
      expect(card.contains(alerts[0])).toBe(true);
      expect(text(alerts[0].querySelector('.gbt-empty-state__heading'))).toBe("Les demandes de fusion n'ont pas pu être chargées.");
      expect(alerts[0].querySelector('.gbt-empty-state')!.getAttribute('data-tone')).toBe('error');
      expect(card.querySelector('[aria-live]')).toBeNull();
      expect(TestBed.inject(GbtToastService).toasts()).toEqual([]);
      expect(text(card.querySelector('button'))).toBe('Réessayer');
      expect(el.querySelector('.merge-request-list__tabs')).toBeNull();
      expect(rows(el)).toHaveLength(0);
    });

    it('reloads on "Réessayer", and a second failure gets a toast (the alert itself does not re-announce)', () => {
      const { fixture, http, el } = setup();
      fixture.detectChanges();
      http.expectOne(LIST_URL).flush('boom', { status: 500, statusText: 'Server Error' });
      fixture.detectChanges();

      el.querySelector('gbt-list-card button')?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
      fixture.detectChanges();
      http.expectOne(LIST_URL).flush('boom again', { status: 500, statusText: 'Server Error' });
      fixture.detectChanges();

      expect(el.querySelector('gbt-list-card .gbt-list-card')!.getAttribute('data-state')).toBe('failed');
      expect(TestBed.inject(GbtToastService).toasts().at(-1)).toMatchObject({ message: 'Impossible de charger les demandes de fusion. Réessayez plus tard.', variant: 'error' });
    });

    it('says that nothing matches, rather than "none yet", when a server-side filter returns nothing', () => {
      const { fixture, http, el } = loaded([mr()], { milestones: [MILESTONE_V1] });

      openSelectAndPick(fixture, 2, 'v1.0');
      http.expectOne((r) => r.url === LIST_URL && r.params.get('milestoneId') === 'm1').flush([]);
      fixture.detectChanges();

      expect(el.querySelector('gbt-empty-state')).toBeNull();
      expect(text(el.querySelector('[list-card-message]'))).toBe('Aucune demande de fusion ouverte ne correspond à ces filtres');
    });

    it('says that a tab is empty when another one is not', () => {
      const { fixture, el } = loaded([mr({ status: 'merged', closedAt: '2026-01-02T00:00:00Z' })]);

      expect(rows(el).length).toBe(0);
      expect(text(el.querySelector('[list-card-message]'))).toBe('Aucune demande de fusion ouverte');

      tabButtons(el)[2].click();
      fixture.detectChanges();
      expect(text(el.querySelector('[list-card-message]'))).toBe('Aucune demande de fusion fermée');
    });
  });

  describe('state tabs', () => {
    const MIXED = [
      mr({ id: 'mr1', title: 'Fix login', createdAt: '2026-01-01T00:00:00Z' }),
      mr({ id: 'mr2', title: 'Add export', createdAt: '2026-01-02T00:00:00Z' }),
      mr({ id: 'mr3', title: 'Write docs', status: 'merged', mergeCommitSha: 'abc', closedAt: '2026-01-05T00:00:00Z', createdAt: '2026-01-03T00:00:00Z' }),
      mr({ id: 'mr4', title: 'Drop legacy', status: 'closed', closedAt: '2026-01-06T00:00:00Z', createdAt: '2026-01-04T00:00:00Z' }),
    ];

    it('labels the tabs with the open, merged and closed counts and shows the open merge requests first', () => {
      const { el } = loaded(MIXED);

      expect(tabButtons(el).map(text)).toEqual(['Ouvertes (2)', 'Fusionnées (1)', 'Fermées (1)']);
      expect(tabButtons(el)[0].getAttribute('aria-checked')).toBe('true');
      expect(rowTitles(el)).toEqual(['Add export', 'Fix login']);
    });

    it('shows only the merged ones on "Fusionnées" and only the closed ones on "Fermées"', () => {
      const { fixture, el } = loaded(MIXED);

      tabButtons(el)[1].click();
      fixture.detectChanges();
      expect(rowTitles(el)).toEqual(['Write docs']);

      tabButtons(el)[2].click();
      fixture.detectChanges();
      expect(rowTitles(el)).toEqual(['Drop legacy']);
    });

    it('counts only the merge requests that match the search', () => {
      const { fixture, el } = loaded(MIXED);

      type(searchInput(el), 'docs', fixture);

      expect(tabButtons(el).map(text)).toEqual(['Ouvertes (0)', 'Fusionnées (1)', 'Fermées (0)']);
    });
  });

  describe('rows', () => {
    it('renders the status icon, the title link, a label chip, the milestone and the branches', () => {
      const { el } = loaded([mr({ title: 'Ajoute la connexion via SSO', sourceBranch: 'feature/sso-login', milestoneId: 'm1', labels: [LABEL_URGENT] })], {
        milestones: [MILESTONE_V1],
      });

      const [row] = rows(el);
      const link = row.querySelector<HTMLAnchorElement>('.gbt-list-row__title > a')!;
      expect(link.getAttribute('href')).toBe('/repositories/acme/widget/-/merge-requests/mr1');
      expect(link.getAttribute('title')).toBe('Ajoute la connexion via SSO');
      expect(text(link)).toBe('Ajoute la connexion via SSO');

      const status = row.querySelector('[row-leading]')!;
      expect(row.querySelector('.gbt-list-row__leading')!.getAttribute('data-tone')).toBe('info');
      expect(status.querySelector('gbt-icon')).toBeTruthy();
      expect(text(status)).toBe('Ouverte');

      expect(text(row.querySelector('.gbt-list-row__title gbt-tag'))).toBe('Urgent');
      expect(text(row.querySelector('.merge-request-list__milestone'))).toBe('v1.0');

      const branches = row.querySelector('.merge-request-list__branches')!;
      expect(Array.from(branches.querySelectorAll('.gbt-badge__label'), text)).toEqual(['feature/sso-login', 'main']);
      expect(branches.getAttribute('title')).toBe('feature/sso-login → main');
    });

    it('shows the status in French, never the raw API status', () => {
      const { fixture, el } = loaded([
        mr({ id: 'mr1', title: 'Un' }),
        mr({ id: 'mr2', title: 'Deux', status: 'merged', closedAt: '2026-01-02T00:00:00Z' }),
        mr({ id: 'mr3', title: 'Trois', status: 'closed', closedAt: '2026-01-02T00:00:00Z' }),
      ]);
      const rawStatus = /\b(open|merged|closed)\b/i;
      const visibleAndTitles = () => text(el) + ' ' + Array.from(el.querySelectorAll('[title]'), (node) => node.getAttribute('title')).join(' ');

      expect(text(rows(el)[0].querySelector('[row-leading]'))).toBe('Ouverte');
      expect(visibleAndTitles()).not.toMatch(rawStatus);

      tabButtons(el)[1].click();
      fixture.detectChanges();
      expect(rows(el)[0].querySelector('.gbt-list-row__leading')!.getAttribute('data-tone')).toBe('success');
      expect(text(rows(el)[0].querySelector('[row-leading]'))).toBe('Fusionnée');
      expect(visibleAndTitles()).not.toMatch(rawStatus);

      tabButtons(el)[2].click();
      fixture.detectChanges();
      expect(rows(el)[0].querySelector('.gbt-list-row__leading')!.getAttribute('data-tone')).toBe('neutral');
      expect(text(rows(el)[0].querySelector('[row-leading]'))).toBe('Fermée');
      expect(visibleAndTitles()).not.toMatch(rawStatus);
    });

    it('writes the meta line: when it was opened, by whom, with the exact date on hover, and the comment count', () => {
      const createdAt = '2026-01-10T08:30:00Z';
      const { el } = loaded([mr({ createdAt, commentCount: 3 })]);

      const meta = rows(el)[0].querySelector('.merge-request-list__meta')!;
      const time = meta.querySelector('time')!;
      expect(time.getAttribute('datetime')).toBe(createdAt);
      expect(time.getAttribute('title')).toBe(absoluteDateTime(createdAt));
      expect(text(time)).toBe(relativeTime(createdAt));
      expect(text(meta.querySelector('.merge-request-list__opened'))).toBe(`ouverte ${relativeTime(createdAt)} par alice`);
      expect(text(meta)).toMatch(new RegExp(`ouverte ${relativeTime(createdAt)} par alice · 3 commentaires$`));
    });

    it('says "1 commentaire" in the singular and leaves the count out when there is none', () => {
      const { el } = loaded([
        mr({ id: 'mr1', commentCount: 1 }),
        mr({ id: 'mr2', commentCount: 0, createdAt: '2026-01-02T00:00:00Z' }),
      ]);

      const [none, one] = rows(el).map((row) => text(row.querySelector('.merge-request-list__meta')));
      expect(one).toMatch(/· 1 commentaire$/);
      expect(none).not.toContain('commentaire');
    });

    it('says "par Utilisateur supprimé" when the author\'s account was deleted', () => {
      const { el } = loaded([mr({ author: null })]);

      const opened = text(rows(el)[0].querySelector('.merge-request-list__opened'));
      expect(opened).toMatch(/^ouverte .* par Utilisateur supprimé$/);
    });

    it('adds when a merged merge request was merged, and when a closed one was closed', () => {
      const mergedAt = '2026-01-20T10:00:00Z';
      const closedAt = '2026-01-21T10:00:00Z';
      const { fixture, el } = loaded([
        mr({ id: 'mr1', status: 'merged', mergeCommitSha: 'abc', closedAt: mergedAt }),
        mr({ id: 'mr2', status: 'closed', closedAt }),
      ]);

      tabButtons(el)[1].click();
      fixture.detectChanges();
      let meta = rows(el)[0].querySelector('.merge-request-list__meta')!;
      expect(text(meta)).toContain(`· fusionnée ${relativeTime(mergedAt)}`);
      expect(text(meta)).not.toContain('fermée');
      expect(meta.querySelectorAll('time')[1].getAttribute('title')).toBe(absoluteDateTime(mergedAt));

      tabButtons(el)[2].click();
      fixture.detectChanges();
      meta = rows(el)[0].querySelector('.merge-request-list__meta')!;
      expect(text(meta)).toContain(`· fermée ${relativeTime(closedAt)}`);
      expect(meta.querySelectorAll('time')[1].getAttribute('datetime')).toBe(closedAt);
    });
  });

  describe('pagination', () => {
    it('shows no pager for 25 merge requests', () => {
      const { el } = loaded(manyMergeRequests(25));

      expect(rows(el).length).toBe(25);
      expect(el.querySelector('gbt-pagination')).toBeNull();
    });

    it('pages 26 merge requests 25 at a time', () => {
      const { fixture, el } = loaded(manyMergeRequests(26));

      expect(rows(el).length).toBe(25);
      expect(el.querySelector('gbt-pagination')).toBeTruthy();

      goToPage(fixture, el, 2);

      expect(rowTitles(el)).toEqual(['Demande 1']);
      expect(el.querySelector('gbt-pagination [aria-current="page"]')?.getAttribute('aria-label')).toBe('Page 2');
    });

    it('goes back to page 1 when the search changes', () => {
      const { fixture, el } = loaded(manyMergeRequests(30));
      el.querySelector<HTMLButtonElement>('gbt-pagination [aria-label="Page suivante"]')!.click();
      fixture.detectChanges();
      expect(rows(el).length).toBe(5);

      type(searchInput(el), 'Demande', fixture);

      expect(rows(el).length).toBe(25);
      expect(rowTitles(el)[0]).toBe('Demande 30');
    });

    it('goes back to page 1 when the tab changes', () => {
      const open = manyMergeRequests(26).map((m) => ({ ...m, id: `o-${m.id}` }));
      const { fixture, el } = loaded([...open, ...manyMergeRequests(27, 'merged')]);
      goToPage(fixture, el, 2);

      tabButtons(el)[1].click();
      fixture.detectChanges();

      expect(rows(el).length).toBe(25);
      expect(el.querySelector('gbt-pagination [aria-current="page"]')?.getAttribute('aria-label')).toBe('Page 1');
    });

    it('falls back to the last page when closing the only merge request of the last page unmounts the pager', async () => {
      const open = manyMergeRequests(26);
      const { fixture, http, el } = loaded(open, { role: 'contributor' });
      goToPage(fixture, el, 2);
      expect(rowTitles(el)).toEqual(['Demande 1']);

      openRowMenu(fixture, rows(el)[0]).find((item) => text(item) === 'Fermer')!.click();
      http.expectOne('/api/merge-requests/mr1/close').flush(null);
      http.expectOne(LIST_URL).flush([closedVersion(open[0]), ...open.slice(1)]);
      fixture.detectChanges();
      await fixture.whenStable();
      fixture.detectChanges();

      expect(tabButtons(el).map(text)).toEqual(['Ouvertes (25)', 'Fusionnées (0)', 'Fermées (1)']);
      expect(el.querySelector('gbt-pagination')).toBeNull();
      expect(rows(el).length).toBe(25);
      expect(el.querySelector('[list-card-message]')).toBeNull();
    });

    it('does the same when merging the only merge request of the last page', async () => {
      const open = manyMergeRequests(26);
      const { fixture, http, el } = loaded(open, { role: 'maintainer' });
      goToPage(fixture, el, 2);

      openRowMenu(fixture, rows(el)[0]).find((item) => text(item) === 'Fusionner')!.click();
      http.expectOne('/api/merge-requests/mr1/merge').flush({ ...mergedVersion(open[0]), outcome: 'merged' });
      http.expectOne(LIST_URL).flush([mergedVersion(open[0]), ...open.slice(1)]);
      fixture.detectChanges();
      await fixture.whenStable();
      fixture.detectChanges();

      expect(tabButtons(el).map(text)).toEqual(['Ouvertes (25)', 'Fusionnées (1)', 'Fermées (0)']);
      expect(el.querySelector('gbt-pagination')).toBeNull();
      expect(rows(el).length).toBe(25);
      expect(el.querySelector('[list-card-message]')).toBeNull();
    });

    it('lets the pager clamp its current page when it stays mounted (51 → 50 merge requests)', async () => {
      const open = manyMergeRequests(51);
      const { fixture, http, el } = loaded(open, { role: 'contributor' });
      goToPage(fixture, el, 3);
      expect(rowTitles(el)).toEqual(['Demande 1']);

      openRowMenu(fixture, rows(el)[0]).find((item) => text(item) === 'Fermer')!.click();
      http.expectOne('/api/merge-requests/mr1/close').flush(null);
      http.expectOne(LIST_URL).flush([closedVersion(open[0]), ...open.slice(1)]);
      fixture.detectChanges();
      await fixture.whenStable();
      fixture.detectChanges();

      expect(el.querySelector('gbt-pagination')).toBeTruthy();
      expect(el.querySelector('gbt-pagination [aria-current="page"]')?.getAttribute('aria-label')).toBe('Page 2');
      expect(rows(el).length).toBe(25);
      expect(rowTitles(el)[24]).toBe('Demande 2');
    });
  });

  describe('aside filters', () => {
    it('filters merge requests by title via the search field', () => {
      const { fixture, el } = loaded([mr({ id: 'mr1', title: 'Fix login' }), mr({ id: 'mr2', title: 'Add export', createdAt: '2026-01-02T00:00:00Z' })]);

      type(searchInput(el), 'export', fixture);

      const list = text(el.querySelector('gbt-list-card ul'));
      expect(list).toContain('Add export');
      expect(list).not.toContain('Fix login');
    });

    it('sorts newest first by default, by title on demand, and reverses with the direction control', async () => {
      const { fixture, el } = loaded([
        mr({ id: 'mr1', title: 'Charlie', createdAt: '2026-01-01T00:00:00Z' }),
        mr({ id: 'mr2', title: 'Alpha', createdAt: '2026-01-02T00:00:00Z' }),
        mr({ id: 'mr3', title: 'Bravo', createdAt: '2026-01-03T00:00:00Z' }),
      ]);
      expect(rowTitles(el)).toEqual(['Bravo', 'Alpha', 'Charlie']);
      await fixture.whenStable();
      fixture.detectChanges();
      expect(text(el.querySelector('.merge-request-list__sort .gbt-select__trigger'))).toBe('Date de création');
      const directionOption = (label: string) =>
        Array.from(el.querySelectorAll<HTMLButtonElement>('.merge-request-list__direction [role="radio"]')).find((b) => text(b) === label)!;
      expect(directionOption('Décroissant').getAttribute('aria-checked')).toBe('true');

      directionOption('Croissant').click();
      fixture.detectChanges();
      expect(rowTitles(el)).toEqual(['Charlie', 'Alpha', 'Bravo']);

      openSelectAndPick(fixture, 0, 'Titre');
      expect(rowTitles(el)).toEqual(['Alpha', 'Bravo', 'Charlie']);

      directionOption('Décroissant').click();
      fixture.detectChanges();
      expect(rowTitles(el)).toEqual(['Charlie', 'Bravo', 'Alpha']);
    });

    it('re-fetches merge requests with the selected label id as a query parameter', () => {
      const { fixture, http } = loaded([mr()], { labels: [{ ...LABEL_URGENT, name: 'Bug' }] });

      openSelectAndPick(fixture, 1, 'Bug');

      const req = http.expectOne((r) => r.url === LIST_URL && r.params.get('labelIds') === 'label-1');
      req.flush([]);
    });

    it('re-fetches merge requests with the selected milestone id as a query parameter', () => {
      const { fixture, http } = loaded([mr()], { milestones: [MILESTONE_V1] });

      openSelectAndPick(fixture, 2, 'v1.0');

      const req = http.expectOne((r) => r.url === LIST_URL && r.params.get('milestoneId') === 'm1');
      req.flush([]);
    });

    it('clears the milestone filter via "Tous les milestones", re-fetching without a milestoneId param', () => {
      const { fixture, http } = loaded([mr()], { milestones: [MILESTONE_V1] });

      openSelectAndPick(fixture, 2, 'v1.0');
      http.expectOne((r) => r.url === LIST_URL && r.params.get('milestoneId') === 'm1').flush([]);

      openSelectAndPick(fixture, 2, 'Tous les milestones');

      const req = http.expectOne((r) => r.url === LIST_URL);
      expect(req.request.params.has('milestoneId')).toBe(false);
      req.flush([]);
    });

    it('shows "Réinitialiser" only while a filter is active', () => {
      const { fixture, el } = loaded([mr()]);
      const resetButton = () => buttonByText(el.querySelector('.merge-request-list__filters')!, 'Réinitialiser');
      expect(resetButton()).toBeUndefined();

      type(searchInput(el), 'zzz', fixture);
      expect(resetButton()).toBeTruthy();

      resetButton()!.click();
      fixture.detectChanges();
      expect(resetButton()).toBeUndefined();
      expect(rows(el).length).toBe(1);
    });

    it('resets the search, labels and milestone at once, re-fetching without any filter', async () => {
      const { fixture, http, el } = loaded([mr()], { labels: [LABEL_URGENT], milestones: [MILESTONE_V1] });
      type(searchInput(el), 'zzz', fixture);
      openSelectAndPick(fixture, 1, 'Urgent');
      http.expectOne((r) => r.url === LIST_URL && r.params.get('labelIds') === 'label-1').flush([]);
      openSelectAndPick(fixture, 2, 'v1.0');
      http.expectOne((r) => r.url === LIST_URL && r.params.get('milestoneId') === 'm1').flush([]);

      buttonByText(el.querySelector('.merge-request-list__filters')!, 'Réinitialiser')!.click();
      fixture.detectChanges();

      const req = http.expectOne((r) => r.url === LIST_URL);
      expect(req.request.params.keys()).toEqual([]);
      req.flush([mr()]);
      fixture.detectChanges();
      await fixture.whenStable();
      fixture.detectChanges();

      expect(rows(el).length).toBe(1);
      expect(searchInput(el).value).toBe('');
      expect(buttonByText(el.querySelector('.merge-request-list__filters')!, 'Réinitialiser')).toBeUndefined();
    });
  });

  describe('row menu', () => {
    const MIXED = [
      mr({ id: 'mr1', title: 'Open one' }),
      mr({ id: 'mr2', title: 'Merged one', status: 'merged', closedAt: '2026-01-02T00:00:00Z' }),
      mr({ id: 'mr3', title: 'Closed one', status: 'closed', closedAt: '2026-01-02T00:00:00Z' }),
    ];

    it('shows no kebab for an open merge request when the caller cannot write', () => {
      const { el } = loaded([mr()], { role: 'reader' });

      expect(rows(el).length).toBe(1);
      expect(el.querySelector('.gbt-menu__trigger')).toBeNull();
    });

    it('offers a contributor "Fermer" but not "Fusionner", which only maintainers and the owner may do', () => {
      const { fixture, el } = loaded(MIXED, { role: 'contributor' });

      expect(openRowMenu(fixture, rows(el)[0]).map(text)).toEqual(['Fermer']);
    });

    it('offers "Fermer" and "Fusionner" on open merge requests only', () => {
      const { fixture, el } = loaded(MIXED, { role: 'maintainer' });

      const [open] = rows(el);
      expect(open.querySelector('.gbt-menu__trigger')!.getAttribute('aria-label')).toBe('Actions de la demande de fusion « Open one »');
      expect(openRowMenu(fixture, open).map(text)).toEqual(['Fermer', 'Fusionner']);

      for (const tab of [1, 2]) {
        tabButtons(el)[tab].click();
        fixture.detectChanges();
        expect(rows(el).length).toBe(1);
        expect(el.querySelector('gbt-list-card ul .gbt-menu__trigger')).toBeNull();
      }
    });

    it('closes an open merge request, reloads the list and confirms', () => {
      const { fixture, http, el } = loaded([mr()], { role: 'maintainer' });

      openRowMenu(fixture, rows(el)[0]).find((item) => text(item) === 'Fermer')!.click();

      http.expectOne('/api/merge-requests/mr1/close').flush(null);
      http.expectOne(LIST_URL).flush([closedVersion(mr())]);
      fixture.detectChanges();

      expect(TestBed.inject(GbtToastService).toasts().at(-1)).toMatchObject({ message: 'Demande de fusion fermée.', variant: 'success' });
      expect(tabButtons(el).map(text)).toEqual(['Ouvertes (0)', 'Fusionnées (0)', 'Fermées (1)']);
    });

    it('shows an error toast when closing fails, without reloading', () => {
      const { fixture, http, el } = loaded([mr()], { role: 'maintainer' });

      openRowMenu(fixture, rows(el)[0]).find((item) => text(item) === 'Fermer')!.click();
      http.expectOne('/api/merge-requests/mr1/close').flush('boom', { status: 500, statusText: 'Server Error' });
      fixture.detectChanges();

      expect(TestBed.inject(GbtToastService).toasts().at(-1)).toMatchObject({ message: 'Impossible de fermer la demande de fusion. Réessayez plus tard.', variant: 'error' });
      http.expectNone(LIST_URL);
      expect(rows(el).length).toBe(1);
    });

    it('attempts to merge an open merge request and reloads on success', () => {
      const { fixture, http, el } = loaded([mr()], { role: 'maintainer' });

      openRowMenu(fixture, rows(el)[0]).find((item) => text(item) === 'Fusionner')!.click();

      http.expectOne('/api/merge-requests/mr1/merge').flush({ ...mergedVersion(mr()), outcome: 'merged' });
      http.expectOne(LIST_URL).flush([mergedVersion(mr())]);
      fixture.detectChanges();

      expect(TestBed.inject(GbtToastService).toasts().at(-1)).toMatchObject({ message: 'Demande de fusion fusionnée.', variant: 'success' });
      expect(tabButtons(el).map(text)).toEqual(['Ouvertes (0)', 'Fusionnées (1)', 'Fermées (0)']);
    });

    it('says so when the merge hits a conflict, without reloading', () => {
      const { fixture, http, el } = loaded([mr()], { role: 'maintainer' });

      openRowMenu(fixture, rows(el)[0]).find((item) => text(item) === 'Fusionner')!.click();
      http.expectOne('/api/merge-requests/mr1/merge').flush({ outcome: 'conflicting' });
      fixture.detectChanges();

      expect(TestBed.inject(GbtToastService).toasts().at(-1)).toMatchObject({ message: 'Fusion impossible : un conflit a été détecté.', variant: 'error' });
      http.expectNone(LIST_URL);
    });

    it('shows an error toast when the merge is refused', () => {
      const { fixture, http, el } = loaded([mr()], { role: 'maintainer' });

      openRowMenu(fixture, rows(el)[0]).find((item) => text(item) === 'Fusionner')!.click();
      http.expectOne('/api/merge-requests/mr1/merge').flush('blocked', { status: 409, statusText: 'Conflict' });
      fixture.detectChanges();

      expect(TestBed.inject(GbtToastService).toasts().at(-1)).toMatchObject({
        message: 'Fusion impossible : vérifiez les approbations requises. Réessayez plus tard.',
        variant: 'error',
      });
    });
  });

  describe('creation modal', () => {
    // The form's NgForm registers its ngModel controls (and writes their values) a microtask later.
    async function openModal(fixture: { detectChanges(): void; whenStable(): Promise<unknown> }, el: HTMLElement): Promise<HTMLElement> {
      buttonByText(el.querySelector('.gbt-page-header__actions')!, 'Nouvelle demande de fusion')!.click();
      fixture.detectChanges();
      await fixture.whenStable();
      fixture.detectChanges();
      return dialog()!;
    }
    const submitButton = (modal: HTMLElement) => buttonByText(modal, 'Créer la demande de fusion')!;
    const titleInput = (modal: HTMLElement) => modal.querySelector<HTMLInputElement>('.merge-request-list__create-title input')!;
    const descriptionInput = (modal: HTMLElement) => modal.querySelector<HTMLTextAreaElement>('.merge-request-list__create-description textarea')!;
    const sourceTrigger = (modal: HTMLElement) => text(modal.querySelector('.merge-request-list__create-source .gbt-select__trigger'));
    const targetTrigger = (modal: HTMLElement) => text(modal.querySelector('.merge-request-list__create-target .gbt-select__trigger'));
    const SOURCE = 0;
    const TARGET = 1;

    async function fillDraft(fixture: { detectChanges(): void; nativeElement: HTMLElement }, modal: HTMLElement) {
      openSelectAndPick(fixture, SOURCE, 'feat', modal);
      openSelectAndPick(fixture, TARGET, 'develop', modal);
      type(titleInput(modal), 'Brouillon', fixture);
      type(descriptionInput(modal), 'Texte du brouillon', fixture);
      titleInput(modal).dispatchEvent(new Event('blur'));
      fixture.detectChanges();
    }

    async function expectFreshForm(fixture: { detectChanges(): void; whenStable(): Promise<unknown> }, el: HTMLElement) {
      const reopened = await openModal(fixture, el);
      expect(sourceTrigger(reopened)).toBe('Choisir une branche');
      expect(targetTrigger(reopened)).toBe('main');
      expect(titleInput(reopened).value).toBe('');
      expect(descriptionInput(reopened).value).toBe('');
      expect(reopened.querySelector('.gbt-input__error, .gbt-select__error')).toBeNull();
      expect(submitButton(reopened).disabled).toBe(true);
    }

    it('has no always-visible creation form', () => {
      const { el } = loaded([mr()], { role: 'owner' });

      expect(el.querySelector('form')).toBeNull();
      expect(dialog()).toBeNull();
    });

    it('opens a "Nouvelle demande de fusion" dialog with the branches, title and description, targeting the default branch', async () => {
      const { fixture, el } = loaded([mr()], { role: 'owner' });

      const modal = await openModal(fixture, el);

      expect(text(modal.querySelector('.gbt-modal__title'))).toBe('Nouvelle demande de fusion');
      expect(sourceTrigger(modal)).toBe('Choisir une branche');
      expect(targetTrigger(modal)).toBe('main');
      expect(titleInput(modal)).toBeTruthy();
      expect(descriptionInput(modal)).toBeTruthy();
    });

    it('keeps the submit button disabled until a source branch and a non-blank title are given', async () => {
      const { fixture, el } = loaded([mr()], { role: 'owner' });
      const modal = await openModal(fixture, el);

      expect(submitButton(modal).disabled).toBe(true);
      type(titleInput(modal), 'Un titre', fixture);
      expect(submitButton(modal).disabled).toBe(true);
      openSelectAndPick(fixture, SOURCE, 'feat', modal);
      expect(submitButton(modal).disabled).toBe(false);
      type(titleInput(modal), '   ', fixture);
      expect(submitButton(modal).disabled).toBe(true);
    });

    it('says the title is required once the empty title field is left', async () => {
      const { fixture, el } = loaded([mr()], { role: 'owner' });
      const modal = await openModal(fixture, el);

      expect(modal.querySelector('.gbt-input__error')).toBeNull();
      titleInput(modal).dispatchEvent(new Event('blur'));
      fixture.detectChanges();

      expect(text(modal.querySelector('.gbt-input__error'))).toBe('Le titre est requis');
    });

    it('refuses a source branch identical to the target branch', async () => {
      const { fixture, el } = loaded([mr()], { role: 'owner' });
      const modal = await openModal(fixture, el);
      type(titleInput(modal), 'Un titre', fixture);

      openSelectAndPick(fixture, SOURCE, 'main', modal);

      expect(text(modal.querySelector('.merge-request-list__create-target .gbt-select__error'))).toBe('Identique à la branche source');
      expect(submitButton(modal).disabled).toBe(true);

      openSelectAndPick(fixture, TARGET, 'develop', modal);
      expect(modal.querySelector('.gbt-select__error')).toBeNull();
      expect(submitButton(modal).disabled).toBe(false);
    });

    it('creates the merge request with the same payload as before, refreshes the list, confirms and closes', async () => {
      const { fixture, http, el } = loaded([mr()], { role: 'owner' });
      const modal = await openModal(fixture, el);

      openSelectAndPick(fixture, SOURCE, 'feat', modal);
      type(titleInput(modal), 'Ajoute le SSO', fixture);
      type(descriptionInput(modal), 'Voir le ticket', fixture);
      submitButton(modal).click();
      fixture.detectChanges();

      const req = http.expectOne((r) => r.method === 'POST' && r.url === LIST_URL);
      expect(req.request.body).toEqual({ sourceBranch: 'feat', targetBranch: 'main', title: 'Ajoute le SSO', description: 'Voir le ticket' });
      req.flush(mr({ id: 'mr2', title: 'Ajoute le SSO' }));
      http.expectOne((r) => r.method === 'GET' && r.url === LIST_URL).flush([mr(), mr({ id: 'mr2', title: 'Ajoute le SSO', createdAt: '2026-01-02T00:00:00Z' })]);
      fixture.detectChanges();

      expect(TestBed.inject(GbtToastService).toasts().at(-1)).toMatchObject({ message: 'Demande de fusion créée.', variant: 'success' });
      expect(dialog()).toBeNull();
      expect(rowTitles(el)).toContain('Ajoute le SSO');

      await expectFreshForm(fixture, el);
    });

    it('shows an error toast and keeps the dialog and what was typed when the creation fails', async () => {
      const { fixture, http, el } = loaded([mr()], { role: 'owner' });
      const modal = await openModal(fixture, el);
      await fillDraft(fixture, modal);

      submitButton(modal).click();
      fixture.detectChanges();
      http.expectOne((r) => r.method === 'POST' && r.url === LIST_URL).flush('boom', { status: 422, statusText: 'Unprocessable Entity' });
      fixture.detectChanges();

      expect(TestBed.inject(GbtToastService).toasts().at(-1)).toMatchObject({
        message: 'Impossible de créer la demande de fusion (branches identiques ou introuvables ?).',
        variant: 'error',
      });
      const kept = dialog()!;
      expect(kept).toBeTruthy();
      expect(sourceTrigger(kept)).toBe('feat');
      expect(targetTrigger(kept)).toBe('develop');
      expect(titleInput(kept).value).toBe('Brouillon');
      expect(descriptionInput(kept).value).toBe('Texte du brouillon');
      expect(submitButton(kept).disabled).toBe(false);
    });

    it('closes without creating anything on "Annuler", and reopens on a fresh form', async () => {
      const { fixture, http, el } = loaded([mr()], { role: 'owner' });
      const modal = await openModal(fixture, el);
      await fillDraft(fixture, modal);

      buttonByText(modal, 'Annuler')!.click();
      fixture.detectChanges();

      expect(dialog()).toBeNull();
      http.expectNone((r) => r.method === 'POST');
      await expectFreshForm(fixture, el);
    });

    it('ignores Escape, the backdrop, × and "Annuler" while the creation runs: the draft is never dropped in flight', async () => {
      const { fixture, http, el } = loaded([mr()], { role: 'owner' });
      const modal = await openModal(fixture, el);
      await fillDraft(fixture, modal);
      submitButton(modal).click();
      fixture.detectChanges();

      expect(text(modal.querySelector('[role="status"]'))).toBe('Création en cours');
      expect(buttonByText(modal, 'Annuler')!.disabled).toBe(true);
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
      document.querySelector<HTMLElement>('.gbt-modal__backdrop')!.click();
      modal.querySelector<HTMLButtonElement>('.gbt-modal__close')!.click();
      buttonByText(modal, 'Annuler')!.click();
      fixture.detectChanges();
      expect(dialog()).toBeTruthy();
      expect(titleInput(dialog()!).value).toBe('Brouillon');

      http.expectOne((r) => r.method === 'POST' && r.url === LIST_URL).flush('boom', { status: 422, statusText: 'Unprocessable Entity' });
      fixture.detectChanges();
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
      fixture.detectChanges();
      expect(dialog()).toBeNull();
    });

    it('resets the form when the dialog is closed with its × button', async () => {
      const { fixture, el } = loaded([mr()], { role: 'owner' });
      const modal = await openModal(fixture, el);
      await fillDraft(fixture, modal);

      modal.querySelector<HTMLButtonElement>('.gbt-modal__close')!.click();
      fixture.detectChanges();

      expect(dialog()).toBeNull();
      await expectFreshForm(fixture, el);
    });

    it('resets the form when the dialog is closed with Escape', async () => {
      const { fixture, el } = loaded([mr()], { role: 'owner' });
      const modal = await openModal(fixture, el);
      await fillDraft(fixture, modal);

      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
      fixture.detectChanges();

      expect(dialog()).toBeNull();
      await expectFreshForm(fixture, el);
    });

    it('keeps the draft after a failed creation, and drops it once the dialog is cancelled', async () => {
      const { fixture, http, el } = loaded([mr()], { role: 'owner' });
      const modal = await openModal(fixture, el);
      await fillDraft(fixture, modal);
      submitButton(modal).click();
      fixture.detectChanges();
      http.expectOne((r) => r.method === 'POST' && r.url === LIST_URL).flush('boom', { status: 500, statusText: 'Server Error' });
      fixture.detectChanges();
      expect(titleInput(dialog()!).value).toBe('Brouillon');

      buttonByText(dialog()!, 'Annuler')!.click();
      fixture.detectChanges();

      await expectFreshForm(fixture, el);
    });
  });
});
