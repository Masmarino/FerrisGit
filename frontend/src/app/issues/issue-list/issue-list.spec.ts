import { LOCALE_ID } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { By } from '@angular/platform-browser';
import { provideRouter, Router } from '@angular/router';
import { EmptyState, GbtToastService, formatDateTime, formatRelativeTime } from '@masmarino/gabarit';
import { IssueList } from './issue-list';
import { Issue } from '../issues.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { MeService } from '../../shell/me.service';
import { PageTitleService } from '../../shell/page-title.service';

const RELATIVE_OPTIONS = { style: 'short', maxUnit: 'day', absoluteAfterDays: 30 } as const;
const ABSOLUTE_OPTIONS = { day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' } as const;
const relativeTime = (iso: string) => formatRelativeTime(iso, 'fr', undefined, RELATIVE_OPTIONS);
const absoluteDateTime = (iso: string) => formatDateTime(iso, 'fr', ABSOLUTE_OPTIONS);

const ISSUES_URL = '/api/repositories/repo-1/issues';

const LABEL_URGENT = { id: 'label-1', name: 'Urgent', color: '#dc2626', repositoryId: 'repo-1', groupId: null, createdAt: '2026-01-01T00:00:00Z' };
const MILESTONE_V1 = { id: 'm1', title: 'v1.0', description: '', dueDate: null, state: 'open', repositoryId: 'repo-1', groupId: null, createdAt: '2026-01-01T00:00:00Z' };

function issue(overrides: Partial<Issue> = {}): Issue {
  return {
    id: 'i1',
    number: 1,
    authorId: 'u1',
    assigneeId: null,
    title: 'Bug',
    description: '',
    status: 'todo',
    kind: 'bug',
    parentIssueId: null,
    createdAt: '2026-01-01T00:00:00Z',
    closedAt: null,
    milestoneId: null,
    labels: [],
    author: { id: 'u1', username: 'alice' },
    assignee: null,
    commentCount: 0,
    ...overrides,
  };
}

function manyIssues(count: number): Issue[] {
  return Array.from({ length: count }, (_, index) =>
    issue({ id: `i${index + 1}`, number: index + 1, title: `Ticket ${index + 1}`, createdAt: new Date(Date.UTC(2026, 0, 1, 0, index)).toISOString() }),
  );
}

type Role = 'owner' | 'contributor' | 'maintainer' | 'reader';

describe('IssueList', () => {
  function setup(options: { role?: Role; meId?: string } = {}) {
    TestBed.configureTestingModule({
      providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([]), { provide: LOCALE_ID, useValue: 'fr' }],
    });
    if (options.role) {
      TestBed.inject(RepositoryContextService).current.set({ repositoryId: 'repo-1', path: ['acme', 'widget'], role: options.role, ancestors: [], groupId: null });
    }
    if (options.meId) {
      TestBed.inject(MeService).id.set(options.meId);
    }
    const fixture = TestBed.createComponent(IssueList);
    fixture.componentRef.setInput('repositoryId', 'repo-1');
    fixture.componentRef.setInput('path', ['acme', 'widget']);
    const http = TestBed.inject(HttpTestingController);
    const el = fixture.nativeElement as HTMLElement;
    return { fixture, http, el };
  }

  function loaded(issues: Issue[], options: { role?: Role; meId?: string; labels?: unknown[]; milestones?: unknown[] } = {}) {
    const ctx = setup(options);
    ctx.fixture.detectChanges();
    ctx.http.expectOne(ISSUES_URL).flush(issues);
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
  const tabButtons = (el: HTMLElement) => Array.from(el.querySelectorAll<HTMLButtonElement>('.issue-list__tabs [role="radio"]'));
  const searchInput = (el: HTMLElement) => el.querySelector<HTMLInputElement>('.issue-list__search input')!;
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

  describe('page frame', () => {
    it('lays the page out as a wide page layout with the filters in its aside', () => {
      const { el } = loaded([issue()]);

      const layout = el.querySelector('gbt-page-layout');
      expect(layout).toBeTruthy();
      expect(layout!.getAttribute('data-width')).toBe('wide');
      expect(el.querySelector('.gbt-page-layout__aside .issue-list__filters')).toBeTruthy();
      expect(el.querySelector('.gbt-page-layout__main gbt-list-card')).toBeTruthy();
    });

    it('titles the page "Tickets" in the page header (the page h1)', () => {
      const { el } = loaded([]);

      expect(text(el.querySelector('gbt-page-header h1'))).toBe('Tickets');
    });

    it('puts "Tickets" in the shell header', () => {
      loaded([]);

      expect(TestBed.inject(PageTitleService).title()).toBe('Tickets');
    });

    it('opens the kanban board from the header', () => {
      const { fixture, el } = loaded([]);
      const navigate = vi.spyOn(TestBed.inject(Router), 'navigate').mockResolvedValue(true);

      buttonByText(el.querySelector('.gbt-page-header__actions')!, 'Vue kanban')!.click();
      fixture.detectChanges();

      expect(navigate).toHaveBeenCalledWith(['/repositories', 'acme', 'widget', '-', 'issues', 'board']);
    });

    it('offers "Nouveau ticket" as the one primary button of the page for a writer', () => {
      const { el } = loaded([issue()], { role: 'contributor' });

      const header = el.querySelector('.gbt-page-header__actions')!;
      expect(buttonByText(header, 'Nouveau ticket')).toBeTruthy();
      const primaries = Array.from(el.querySelectorAll('.gbt-button--primary'));
      expect(primaries.length).toBe(1);
      expect(text(primaries[0])).toBe('Nouveau ticket');
    });

    it('has no "Nouveau ticket" button and no row menu for a reader', () => {
      const { el } = loaded([issue()], { role: 'reader' });

      expect(buttonByText(el, 'Nouveau ticket')).toBeUndefined();
      expect(el.querySelector('.gbt-menu__trigger')).toBeNull();
      expect(el.querySelector('.gbt-button--primary')).toBeNull();
    });
  });

  describe('loading and empty states', () => {
    it('shows skeleton rows until the issues arrive, then the rows', () => {
      const { fixture, http, el } = setup();
      fixture.detectChanges();

      const card = el.querySelector('gbt-list-card')!;
      expect(card.querySelector('.gbt-list-card')!.getAttribute('data-state')).toBe('loading');
      const busy = card.querySelector('[aria-busy="true"]')!;
      expect(busy).toBeTruthy();
      expect(busy.querySelectorAll('gbt-skeleton').length).toBeGreaterThan(0);
      expect(text(card.querySelector('[role="status"]'))).toBe('Chargement des tickets…');
      expect(busy.querySelector('[role="status"]')).toBeNull();
      expect(el.querySelector('gbt-empty-state')).toBeNull();

      http.expectOne(ISSUES_URL).flush([issue()]);
      http.expectOne('/api/repositories/repo-1/labels').flush([]);
      http.expectOne('/api/repositories/repo-1/milestones').flush([]);
      fixture.detectChanges();

      expect(el.querySelector('gbt-list-card .gbt-list-card')!.getAttribute('data-state')).toBe('ready');
      expect(el.querySelector('gbt-list-card [aria-busy="true"]')).toBeNull();
      expect(text(el.querySelector('gbt-list-card [role="status"]'))).toBe('');
      expect(rows(el).length).toBe(1);
    });

    it('shows the illustrated empty state, without a create button, to a reader when there are no issues', () => {
      const { fixture, el } = loaded([], { role: 'reader' });

      const emptyState = el.querySelector('gbt-empty-state');
      expect(emptyState).toBeTruthy();
      expect(fixture.debugElement.query(By.directive(EmptyState)).componentInstance.illustration()).toBe('checklist');
      expect(el.querySelector('gbt-list-card .gbt-list-card')!.getAttribute('data-state')).toBe('empty');
      expect(text(emptyState)).toContain("Aucun ticket pour l'instant");
      expect(emptyState!.querySelector('gbt-button')).toBeNull();
      expect(el.querySelector('.issue-list__tabs')).toBeNull();
      expect(el.querySelector('.issue-list__filters')).toBeNull();
    });

    it('says the list could not be loaded in one alert (no toast on top of it) and no tabs, when the request fails', () => {
      const { fixture, http, el } = setup();
      fixture.detectChanges();
      http.expectOne(ISSUES_URL).flush('boom', { status: 500, statusText: 'Server Error' });
      fixture.detectChanges();

      const card = el.querySelector('gbt-list-card')!;
      expect(card.querySelector('.gbt-list-card')!.getAttribute('data-state')).toBe('failed');
      // The card's failed block is the announcement: exactly one alert, and no error toast.
      const alerts = el.querySelectorAll('[role="alert"]');
      expect(alerts).toHaveLength(1);
      expect(card.contains(alerts[0])).toBe(true);
      expect(text(alerts[0].querySelector('.gbt-empty-state__heading'))).toBe("Les tickets n'ont pas pu être chargés.");
      expect(alerts[0].querySelector('.gbt-empty-state')!.getAttribute('data-tone')).toBe('error');
      expect(card.querySelector('[aria-live]')).toBeNull();
      expect(TestBed.inject(GbtToastService).toasts()).toEqual([]);
      expect(text(card.querySelector('button'))).toBe('Réessayer');
      expect(el.querySelector('.issue-list__tabs')).toBeNull();
      expect(rows(el)).toHaveLength(0);
    });

    it('reloads on "Réessayer", and a second failure gets a toast (the alert itself does not re-announce)', () => {
      const { fixture, http, el } = setup();
      fixture.detectChanges();
      http.expectOne(ISSUES_URL).flush('boom', { status: 500, statusText: 'Server Error' });
      fixture.detectChanges();

      el.querySelector('gbt-list-card button')?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
      fixture.detectChanges();
      http.expectOne(ISSUES_URL).flush('boom again', { status: 500, statusText: 'Server Error' });
      fixture.detectChanges();

      expect(el.querySelector('gbt-list-card .gbt-list-card')!.getAttribute('data-state')).toBe('failed');
      expect(TestBed.inject(GbtToastService).toasts().at(-1)).toMatchObject({ message: 'Impossible de charger les tickets. Réessayez plus tard.', variant: 'error' });
    });

    it('offers a secondary "Créer un ticket" in the empty state to a writer (the header keeps the only primary)', () => {
      const { fixture, el } = loaded([], { role: 'owner' });

      const emptyState = el.querySelector('gbt-empty-state')!;
      const create = buttonByText(emptyState, 'Créer un ticket');
      expect(create).toBeTruthy();
      expect(create!.classList).toContain('gbt-button--secondary');
      expect(el.querySelectorAll('.gbt-button--primary').length).toBe(1);

      create!.click();
      fixture.detectChanges();
      expect(dialog()).toBeTruthy();
    });

    it('says that nothing matches, rather than "no issues yet", when a server-side filter returns nothing', () => {
      const { fixture, http, el } = loaded([issue()], { milestones: [MILESTONE_V1] });

      openSelectAndPick(fixture, 2, 'v1.0');
      http.expectOne((r) => r.url === ISSUES_URL && r.params.get('milestoneId') === 'm1').flush([]);
      fixture.detectChanges();

      expect(el.querySelector('gbt-empty-state')).toBeNull();
      expect(text(el.querySelector('[list-card-message]'))).toContain('ne correspond à ces filtres');
    });

    it('says that a tab is empty when the other one is not', () => {
      const { el } = loaded([issue({ status: 'done', closedAt: '2026-01-02T00:00:00Z' })]);

      expect(rows(el).length).toBe(0);
      expect(text(el.querySelector('[list-card-message]'))).toBe('Aucun ticket ouvert');
    });
  });

  describe('state tabs', () => {
    const MIXED = [
      issue({ id: 'i1', number: 1, title: 'Fix login', status: 'todo' }),
      issue({ id: 'i2', number: 2, title: 'Add export', status: 'in_progress', createdAt: '2026-01-02T00:00:00Z' }),
      issue({ id: 'i3', number: 3, title: 'Write docs', status: 'done', closedAt: '2026-01-05T00:00:00Z', createdAt: '2026-01-03T00:00:00Z' }),
    ];

    it('labels the tabs with the open and closed counts and shows the open issues first', () => {
      const { el } = loaded(MIXED);

      expect(tabButtons(el).map(text)).toEqual(['Ouverts (2)', 'Fermés (1)']);
      expect(tabButtons(el)[0].getAttribute('aria-checked')).toBe('true');
      expect(rowTitles(el)).toEqual(['#2 Add export', '#1 Fix login']);
    });

    it('shows only the closed issues on the "Fermés" tab', () => {
      const { fixture, el } = loaded(MIXED);

      tabButtons(el)[1].click();
      fixture.detectChanges();

      expect(rowTitles(el)).toEqual(['#3 Write docs']);
    });

    it('counts only the issues that match the search', () => {
      const { fixture, el } = loaded(MIXED);

      type(searchInput(el), 'docs', fixture);

      expect(tabButtons(el).map(text)).toEqual(['Ouverts (0)', 'Fermés (1)']);
    });
  });

  describe('rows', () => {
    it('renders the status icon, the numbered title link, the kind, a label chip and the milestone', () => {
      const { el } = loaded(
        [issue({ number: 3, title: "Écrire la doc d'onboarding", milestoneId: 'm1', labels: [LABEL_URGENT] })],
        { milestones: [MILESTONE_V1] },
      );

      const [row] = rows(el);
      const link = row.querySelector<HTMLAnchorElement>('.gbt-list-row__title > a')!;
      expect(link.getAttribute('href')).toBe('/repositories/acme/widget/-/issues/3');
      expect(link.getAttribute('title')).toBe("Écrire la doc d'onboarding");
      expect(text(link.querySelector('.issue-list__number'))).toBe('#3');
      expect(text(link)).toBe("#3 Écrire la doc d'onboarding");

      const status = row.querySelector('[row-leading]')!;
      expect(row.querySelector('.gbt-list-row__leading')!.getAttribute('data-tone')).toBe('neutral');
      expect(text(status)).toBe('À faire');

      expect(text(row.querySelector('.issue-list__kind'))).toBe('Bug');
      const tag = el.querySelector('gbt-list-card ul gbt-tag');
      expect(tag?.textContent?.trim()).toBe('Urgent');
      expect(text(row.querySelector('.issue-list__milestone'))).toBe('v1.0');
    });

    it('writes the meta line: when it was opened, by whom, with the exact date on hover, and the comment count', () => {
      const createdAt = '2026-01-10T08:30:00Z';
      const { el } = loaded([issue({ createdAt, commentCount: 3 })]);

      const meta = rows(el)[0].querySelector('.issue-list__meta')!;
      const time = meta.querySelector('time')!;
      expect(time.getAttribute('datetime')).toBe(createdAt);
      expect(time.getAttribute('title')).toBe(absoluteDateTime(createdAt));
      expect(text(time)).toBe(relativeTime(createdAt));
      expect(text(meta)).toBe(`ouvert ${relativeTime(createdAt)} par alice · 3 commentaires`);
    });

    it('says "1 commentaire" in the singular and leaves the count out when there is none', () => {
      const { el } = loaded([
        issue({ id: 'i1', number: 1, commentCount: 1 }),
        issue({ id: 'i2', number: 2, commentCount: 0, createdAt: '2026-01-02T00:00:00Z' }),
      ]);

      const [none, one] = rows(el).map((row) => text(row.querySelector('.issue-list__meta')));
      expect(one).toMatch(/· 1 commentaire$/);
      expect(none).not.toContain('commentaire');
    });

    it('leaves out "par …" when the author no longer resolves', () => {
      const { el } = loaded([issue({ author: null })]);

      const meta = text(rows(el)[0].querySelector('.issue-list__meta'));
      expect(meta).toMatch(/^ouvert /);
      expect(meta).not.toContain('par');
    });

    it('adds when a closed issue was closed', () => {
      const closedAt = '2026-01-20T10:00:00Z';
      const { fixture, el } = loaded([issue({ status: 'done', closedAt })]);
      tabButtons(el)[1].click();
      fixture.detectChanges();

      const meta = rows(el)[0].querySelector('.issue-list__meta')!;
      expect(text(meta)).toContain(`· fermé ${relativeTime(closedAt)}`);
      const times = Array.from(meta.querySelectorAll('time'));
      expect(times[1].getAttribute('title')).toBe(absoluteDateTime(closedAt));
    });

    it('shows the assignee as a user chip in the trailing column, and none when unassigned', () => {
      const { el } = loaded([
        issue({ id: 'i1', number: 1, assigneeId: 'u2', assignee: { id: 'u2', username: 'bob' } }),
        issue({ id: 'i2', number: 2, createdAt: '2026-01-02T00:00:00Z' }),
      ]);

      const [unassigned, assigned] = rows(el);
      const chip = assigned.querySelector('[row-trailing] gbt-user-chip');
      expect(chip).toBeTruthy();
      expect(text(chip!.querySelector('.gbt-user-chip__name'))).toBe('bob');
      expect(text(assigned.querySelector('[row-trailing] .issue-list__assignee'))).toContain('Assigné à');
      expect(unassigned.querySelector('gbt-user-chip')).toBeNull();
    });
  });

  describe('pagination', () => {
    it('shows no pager for 25 issues', () => {
      const { el } = loaded(manyIssues(25));

      expect(rows(el).length).toBe(25);
      expect(el.querySelector('gbt-pagination')).toBeNull();
    });

    it('pages 26 issues 25 at a time', () => {
      const { fixture, el } = loaded(manyIssues(26));

      expect(rows(el).length).toBe(25);
      expect(el.querySelector('gbt-pagination')).toBeTruthy();

      el.querySelector<HTMLButtonElement>('gbt-pagination [aria-label="Page 2"]')!.click();
      fixture.detectChanges();

      expect(rowTitles(el)).toEqual(['#1 Ticket 1']);
      expect(el.querySelector('gbt-pagination [aria-current="page"]')?.getAttribute('aria-label')).toBe('Page 2');
    });

    it('goes back to page 1 when the search changes', () => {
      const { fixture, el } = loaded(manyIssues(30));
      el.querySelector<HTMLButtonElement>('gbt-pagination [aria-label="Page suivante"]')!.click();
      fixture.detectChanges();
      expect(rows(el).length).toBe(5);

      type(searchInput(el), 'Ticket', fixture);

      expect(rows(el).length).toBe(25);
      expect(rowTitles(el)[0]).toBe('#30 Ticket 30');
    });

    it('goes back to page 1 when the tab changes', () => {
      const closed = manyIssues(27).map((i) => ({ ...i, status: 'done' as const, closedAt: '2026-02-01T00:00:00Z' }));
      const { fixture, el } = loaded([...manyIssues(26).map((i) => ({ ...i, id: `o${i.number}`, number: i.number + 100 })), ...closed]);
      el.querySelector<HTMLButtonElement>('gbt-pagination [aria-label="Page 2"]')!.click();
      fixture.detectChanges();

      tabButtons(el)[1].click();
      fixture.detectChanges();

      expect(rows(el).length).toBe(25);
      expect(el.querySelector('gbt-pagination [aria-current="page"]')?.getAttribute('aria-label')).toBe('Page 1');
    });

    const closedVersion = (i: Issue): Issue => ({ ...i, status: 'done', closedAt: '2026-03-01T00:00:00Z' });
    const goToPage = (fixture: { detectChanges(): void }, el: HTMLElement, page: number) => {
      el.querySelector<HTMLButtonElement>(`gbt-pagination [aria-label="Page ${page}"]`)!.click();
      fixture.detectChanges();
    };

    it('falls back to the last page when closing the only issue of the last page unmounts the pager', async () => {
      const open = manyIssues(26);
      const { fixture, http, el } = loaded(open, { role: 'contributor' });
      goToPage(fixture, el, 2);
      expect(rowTitles(el)).toEqual(['#1 Ticket 1']);

      openRowMenu(fixture, rows(el)[0]).find((item) => text(item) === 'Fermer')!.click();
      http.expectOne(`${ISSUES_URL}/1/close`).flush(closedVersion(open[0]));
      fixture.detectChanges();
      await fixture.whenStable();
      fixture.detectChanges();

      expect(tabButtons(el).map(text)[0]).toBe('Ouverts (25)');
      expect(el.querySelector('gbt-pagination')).toBeNull();
      expect(rows(el).length).toBe(25);
      expect(el.querySelector('[list-card-message]')).toBeNull();
    });

    it('does the same on the "Fermés" tab when reopening the only issue of its last page', async () => {
      const closed = manyIssues(26).map(closedVersion);
      const { fixture, http, el } = loaded(closed, { role: 'contributor' });
      tabButtons(el)[1].click();
      fixture.detectChanges();
      goToPage(fixture, el, 2);
      expect(rows(el).length).toBe(1);
      const [last] = rows(el);
      const lastNumber = text(last.querySelector('.issue-list__number')).slice(1);

      openRowMenu(fixture, last).find((item) => text(item) === 'Rouvrir')!.click();
      const reopened = closed.find((i) => String(i.number) === lastNumber)!;
      http.expectOne(`${ISSUES_URL}/${lastNumber}/reopen`).flush({ ...reopened, status: 'todo', closedAt: null });
      fixture.detectChanges();
      await fixture.whenStable();
      fixture.detectChanges();

      expect(tabButtons(el).map(text)).toEqual(['Ouverts (1)', 'Fermés (25)']);
      expect(rows(el).length).toBe(25);
      expect(el.querySelector('[list-card-message]')).toBeNull();
    });

    it('lets the pager clamp its current page when it stays mounted (51 → 50 issues)', async () => {
      const open = manyIssues(51);
      const { fixture, http, el } = loaded(open, { role: 'contributor' });
      goToPage(fixture, el, 3);
      expect(rowTitles(el)).toEqual(['#1 Ticket 1']);

      openRowMenu(fixture, rows(el)[0]).find((item) => text(item) === 'Fermer')!.click();
      http.expectOne(`${ISSUES_URL}/1/close`).flush(closedVersion(open[0]));
      fixture.detectChanges();
      await fixture.whenStable();
      fixture.detectChanges();

      expect(el.querySelector('gbt-pagination')).toBeTruthy();
      expect(el.querySelector('gbt-pagination [aria-current="page"]')?.getAttribute('aria-label')).toBe('Page 2');
      expect(rows(el).length).toBe(25);
      expect(rowTitles(el)[24]).toBe('#2 Ticket 2');
    });
  });

  describe('aside filters', () => {
    it('filters issues by title via the search field', () => {
      const { fixture, el } = loaded([
        issue({ id: 'i1', number: 1, title: 'Fix login' }),
        issue({ id: 'i2', number: 2, title: 'Add export', kind: 'feature', createdAt: '2026-01-02T00:00:00Z' }),
      ]);

      type(searchInput(el), 'export', fixture);

      const list = text(el.querySelector('gbt-list-card ul'));
      expect(list).toContain('Add export');
      expect(list).not.toContain('Fix login');
    });

    it('sorts newest first by default, by title on demand, and reverses with the direction control', async () => {
      const { fixture, el } = loaded([
        issue({ id: 'i1', number: 1, title: 'Charlie', createdAt: '2026-01-01T00:00:00Z' }),
        issue({ id: 'i2', number: 2, title: 'Alpha', createdAt: '2026-01-02T00:00:00Z' }),
        issue({ id: 'i3', number: 3, title: 'Bravo', createdAt: '2026-01-03T00:00:00Z' }),
      ]);
      expect(rowTitles(el)).toEqual(['#3 Bravo', '#2 Alpha', '#1 Charlie']);
      await fixture.whenStable();
      fixture.detectChanges();
      expect(text(el.querySelector('.issue-list__sort .gbt-select__trigger'))).toBe('Date de création');
      const directionOption = (label: string) => Array.from(el.querySelectorAll<HTMLButtonElement>('.issue-list__direction [role="radio"]')).find((b) => text(b) === label)!;
      expect(directionOption('Décroissant').getAttribute('aria-checked')).toBe('true');

      directionOption('Croissant').click();
      fixture.detectChanges();
      expect(rowTitles(el)).toEqual(['#1 Charlie', '#2 Alpha', '#3 Bravo']);

      openSelectAndPick(fixture, 0, 'Titre');
      expect(rowTitles(el)).toEqual(['#2 Alpha', '#3 Bravo', '#1 Charlie']);

      directionOption('Décroissant').click();
      fixture.detectChanges();
      expect(rowTitles(el)).toEqual(['#1 Charlie', '#3 Bravo', '#2 Alpha']);
    });

    it('re-fetches issues with the selected label id as a query parameter', () => {
      const { fixture, http } = loaded([issue()], { labels: [{ ...LABEL_URGENT, name: 'Bug' }] });

      openSelectAndPick(fixture, 1, 'Bug');

      const req = http.expectOne((r) => r.url === ISSUES_URL && r.params.get('labelIds') === 'label-1');
      req.flush([]);
    });

    it('re-fetches issues with the selected milestone id as a query parameter', () => {
      const { fixture, http } = loaded([issue()], { milestones: [MILESTONE_V1] });

      openSelectAndPick(fixture, 2, 'v1.0');

      const req = http.expectOne((r) => r.url === ISSUES_URL && r.params.get('milestoneId') === 'm1');
      req.flush([]);
    });

    it('clears the milestone filter via "Tous les milestones", re-fetching without a milestoneId param', () => {
      const { fixture, http } = loaded([issue()], { milestones: [MILESTONE_V1] });

      openSelectAndPick(fixture, 2, 'v1.0');
      http.expectOne((r) => r.url === ISSUES_URL && r.params.get('milestoneId') === 'm1').flush([]);

      openSelectAndPick(fixture, 2, 'Tous les milestones');

      const req = http.expectOne((r) => r.url === ISSUES_URL);
      expect(req.request.params.has('milestoneId')).toBe(false);
      req.flush([]);
    });

    it('shows "Réinitialiser" only while a filter is active', () => {
      const { fixture, el } = loaded([issue()]);
      const resetButton = () => buttonByText(el.querySelector('.issue-list__filters')!, 'Réinitialiser');
      expect(resetButton()).toBeUndefined();

      type(searchInput(el), 'bug', fixture);
      expect(resetButton()).toBeTruthy();

      resetButton()!.click();
      fixture.detectChanges();
      expect(resetButton()).toBeUndefined();
      expect(rows(el).length).toBe(1);
    });

    it('resets the search, labels and milestone at once, re-fetching without any filter', async () => {
      const { fixture, http, el } = loaded([issue()], { labels: [LABEL_URGENT], milestones: [MILESTONE_V1] });
      type(searchInput(el), 'zzz', fixture);
      openSelectAndPick(fixture, 1, 'Urgent');
      http.expectOne((r) => r.url === ISSUES_URL && r.params.get('labelIds') === 'label-1').flush([]);
      openSelectAndPick(fixture, 2, 'v1.0');
      http.expectOne((r) => r.url === ISSUES_URL && r.params.get('milestoneId') === 'm1').flush([]);

      buttonByText(el.querySelector('.issue-list__filters')!, 'Réinitialiser')!.click();
      fixture.detectChanges();

      const req = http.expectOne((r) => r.url === ISSUES_URL);
      expect(req.request.params.keys()).toEqual([]);
      req.flush([issue()]);
      fixture.detectChanges();
      await fixture.whenStable();
      fixture.detectChanges();

      expect(rows(el).length).toBe(1);
      expect(searchInput(el).value).toBe('');
      expect(buttonByText(el.querySelector('.issue-list__filters')!, 'Réinitialiser')).toBeUndefined();
    });
  });

  describe('row menu', () => {
    it('shows no kebab for a reader (no write access)', () => {
      const { el } = loaded([issue()], { role: 'reader' });

      expect(el.querySelector('.gbt-menu__trigger')).toBeNull();
    });

    it('closes an open issue (it moves to "Fermés") and re-opens it from there', () => {
      const { fixture, http, el } = loaded([issue()], { role: 'contributor' });
      const toasts = TestBed.inject(GbtToastService);

      openRowMenu(fixture, rows(el)[0]).find((item) => text(item) === 'Fermer')!.click();
      http.expectOne(`${ISSUES_URL}/1/close`).flush(issue({ status: 'done', closedAt: '2026-01-02T00:00:00Z' }));
      fixture.detectChanges();

      expect(toasts.toasts().at(-1)).toMatchObject({ message: 'Ticket fermé.', variant: 'success' });
      expect(rows(el).length).toBe(0);
      expect(tabButtons(el).map(text)).toEqual(['Ouverts (0)', 'Fermés (1)']);

      tabButtons(el)[1].click();
      fixture.detectChanges();
      const items = openRowMenu(fixture, rows(el)[0]);
      expect(items.some((item) => text(item) === 'Rouvrir')).toBe(true);
      items.find((item) => text(item) === 'Rouvrir')!.click();
      http.expectOne(`${ISSUES_URL}/1/reopen`).flush(issue());
      fixture.detectChanges();

      expect(toasts.toasts().at(-1)).toMatchObject({ message: 'Ticket rouvert.', variant: 'success' });
      expect(tabButtons(el).map(text)).toEqual(['Ouverts (1)', 'Fermés (0)']);
    });

    it('shows an error toast when closing fails, leaving the row as it was', () => {
      const { fixture, http, el } = loaded([issue()], { role: 'contributor' });

      openRowMenu(fixture, rows(el)[0]).find((item) => text(item) === 'Fermer')!.click();
      http.expectOne(`${ISSUES_URL}/1/close`).flush('boom', { status: 500, statusText: 'Server Error' });
      fixture.detectChanges();

      expect(TestBed.inject(GbtToastService).toasts().at(-1)?.variant).toBe('error');
      expect(rows(el).length).toBe(1);
    });

    it('assigns the issue to the current user', () => {
      const { fixture, http, el } = loaded([issue()], { role: 'contributor', meId: 'me-1' });

      openRowMenu(fixture, rows(el)[0]).find((item) => text(item) === "M'assigner")!.click();

      const req = http.expectOne(`${ISSUES_URL}/1/assign`);
      expect(req.request.body).toEqual({ assigneeId: 'me-1' });
      req.flush(issue({ assigneeId: 'me-1', assignee: { id: 'me-1', username: 'moi' } }));
      fixture.detectChanges();

      expect(TestBed.inject(GbtToastService).toasts().at(-1)).toMatchObject({ message: 'Ticket assigné.', variant: 'success' });
      expect(text(rows(el)[0].querySelector('gbt-user-chip .gbt-user-chip__name'))).toBe('moi');
    });
  });

  describe('creation modal', () => {
    // NgForm registers its ngModel controls (and writes their values) a microtask later.
    async function openModal(fixture: { detectChanges(): void; whenStable(): Promise<unknown> }, el: HTMLElement): Promise<HTMLElement> {
      buttonByText(el.querySelector('.gbt-page-header__actions')!, 'Nouveau ticket')!.click();
      fixture.detectChanges();
      await fixture.whenStable();
      fixture.detectChanges();
      return dialog()!;
    }
    const submitButton = (modal: HTMLElement) => buttonByText(modal, 'Créer le ticket')!;
    const titleInput = (modal: HTMLElement) => modal.querySelector<HTMLInputElement>('.issue-list__create-title input')!;
    const descriptionInput = (modal: HTMLElement) => modal.querySelector<HTMLTextAreaElement>('.issue-list__create-description textarea')!;
    const kindTrigger = (modal: HTMLElement) => text(modal.querySelector('.issue-list__create-kind .gbt-select__trigger'));

    async function fillDraft(fixture: { detectChanges(): void; nativeElement: HTMLElement }, modal: HTMLElement) {
      type(titleInput(modal), 'Brouillon', fixture);
      type(descriptionInput(modal), 'Texte du brouillon', fixture);
      openSelectAndPick(fixture, 0, 'Fonctionnalité', modal);
      titleInput(modal).dispatchEvent(new Event('blur'));
      fixture.detectChanges();
    }

    async function expectFreshForm(fixture: { detectChanges(): void; whenStable(): Promise<unknown> }, el: HTMLElement) {
      const reopened = await openModal(fixture, el);
      expect(titleInput(reopened).value).toBe('');
      expect(descriptionInput(reopened).value).toBe('');
      expect(kindTrigger(reopened)).toBe('Tâche');
      expect(reopened.querySelector('.gbt-input__error')).toBeNull();
      expect(submitButton(reopened).disabled).toBe(true);
    }

    it('has no always-visible creation form', async () => {
      const { el } = loaded([issue()], { role: 'owner' });

      expect(el.querySelector('form')).toBeNull();
      expect(dialog()).toBeNull();
    });

    it('opens a "Nouveau ticket" dialog with the title, description and type fields', async () => {
      const { fixture, el } = loaded([issue()], { role: 'owner' });

      const modal = await openModal(fixture, el);

      expect(modal).toBeTruthy();
      expect(text(modal.querySelector('.gbt-modal__title'))).toBe('Nouveau ticket');
      expect(modal.querySelector('.issue-list__create-title input')).toBeTruthy();
      expect(modal.querySelector('.issue-list__create-description textarea')).toBeTruthy();
      expect(text(modal.querySelector('.issue-list__create-kind .gbt-select__trigger'))).toBe('Tâche');
    });

    it('keeps the submit button disabled while the title is empty or blank', async () => {
      const { fixture, el } = loaded([issue()], { role: 'owner' });
      const modal = await openModal(fixture, el);

      expect(submitButton(modal).disabled).toBe(true);
      type(titleInput(modal), '   ', fixture);
      expect(submitButton(modal).disabled).toBe(true);
      type(titleInput(modal), 'Un titre', fixture);
      expect(submitButton(modal).disabled).toBe(false);
    });

    it('says the title is required once the empty title field is left', async () => {
      const { fixture, el } = loaded([issue()], { role: 'owner' });
      const modal = await openModal(fixture, el);

      expect(modal.querySelector('.gbt-input__error')).toBeNull();
      titleInput(modal).dispatchEvent(new Event('blur'));
      fixture.detectChanges();

      expect(text(modal.querySelector('.gbt-input__error'))).toBe('Le titre est requis');
    });

    it('creates the issue with the same payload as before, refreshes the list, confirms and closes', async () => {
      const { fixture, http, el } = loaded([issue()], { role: 'owner' });
      const modal = await openModal(fixture, el);

      type(titleInput(modal), 'Crash au démarrage', fixture);
      type(modal.querySelector<HTMLTextAreaElement>('.issue-list__create-description textarea')!, 'Voir les logs', fixture);
      openSelectAndPick(fixture as never, 0, 'Bug', modal);
      submitButton(modal).click();
      fixture.detectChanges();

      const req = http.expectOne((r) => r.method === 'POST' && r.url === ISSUES_URL);
      expect(req.request.body).toEqual({ title: 'Crash au démarrage', description: 'Voir les logs', kind: 'bug' });
      req.flush(issue({ id: 'i2', number: 2, title: 'Crash au démarrage' }));
      http.expectOne((r) => r.method === 'GET' && r.url === ISSUES_URL).flush([issue(), issue({ id: 'i2', number: 2, title: 'Crash au démarrage', createdAt: '2026-01-02T00:00:00Z' })]);
      fixture.detectChanges();

      expect(TestBed.inject(GbtToastService).toasts().at(-1)).toMatchObject({ message: 'Ticket créé.', variant: 'success' });
      expect(dialog()).toBeNull();
      expect(rowTitles(el)).toContain('#2 Crash au démarrage');

      const reopened = await openModal(fixture, el);
      expect(titleInput(reopened).value).toBe('');
    });

    it('shows an error toast and keeps the dialog and what was typed when the creation fails', async () => {
      const { fixture, http, el } = loaded([issue()], { role: 'owner' });
      const modal = await openModal(fixture, el);
      type(titleInput(modal), 'Crash', fixture);
      type(descriptionInput(modal), 'Voir les logs', fixture);
      openSelectAndPick(fixture as never, 0, 'Bug', modal);

      submitButton(modal).click();
      fixture.detectChanges();
      http.expectOne((r) => r.method === 'POST' && r.url === ISSUES_URL).flush('boom', { status: 500, statusText: 'Server Error' });
      fixture.detectChanges();

      expect(TestBed.inject(GbtToastService).toasts().at(-1)).toMatchObject({ message: 'Impossible de créer le ticket.', variant: 'error' });
      expect(dialog()).toBeTruthy();
      expect(titleInput(dialog()!).value).toBe('Crash');
      expect(descriptionInput(dialog()!).value).toBe('Voir les logs');
      expect(kindTrigger(dialog()!)).toBe('Bug');
      expect(submitButton(dialog()!).disabled).toBe(false);
    });

    it('closes without creating anything on "Annuler", and reopens on a fresh form', async () => {
      const { fixture, http, el } = loaded([issue()], { role: 'owner' });
      const modal = await openModal(fixture, el);
      await fillDraft(fixture, modal);

      buttonByText(modal, 'Annuler')!.click();
      fixture.detectChanges();

      expect(dialog()).toBeNull();
      http.expectNone((r) => r.method === 'POST');
      await expectFreshForm(fixture, el);
    });

    it('ignores Escape, the backdrop, × and "Annuler" while the creation runs: the draft is never dropped in flight', async () => {
      const { fixture, http, el } = loaded([issue()], { role: 'owner' });
      const modal = await openModal(fixture, el);
      type(titleInput(modal), 'Crash', fixture);
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
      expect(titleInput(dialog()!).value).toBe('Crash');

      http.expectOne((r) => r.method === 'POST' && r.url === ISSUES_URL).flush('boom', { status: 500, statusText: 'Server Error' });
      fixture.detectChanges();
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
      fixture.detectChanges();
      expect(dialog()).toBeNull();
    });

    it('resets the form when the dialog is closed with its × button', async () => {
      const { fixture, el } = loaded([issue()], { role: 'owner' });
      const modal = await openModal(fixture, el);
      await fillDraft(fixture, modal);

      modal.querySelector<HTMLButtonElement>('.gbt-modal__close')!.click();
      fixture.detectChanges();

      expect(dialog()).toBeNull();
      await expectFreshForm(fixture, el);
    });

    it('resets the form when the dialog is closed with Escape', async () => {
      const { fixture, el } = loaded([issue()], { role: 'owner' });
      const modal = await openModal(fixture, el);
      await fillDraft(fixture, modal);

      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
      fixture.detectChanges();

      expect(dialog()).toBeNull();
      await expectFreshForm(fixture, el);
    });

    it('keeps the draft after a failed creation, and drops it once the dialog is cancelled', async () => {
      const { fixture, http, el } = loaded([issue()], { role: 'owner' });
      const modal = await openModal(fixture, el);
      await fillDraft(fixture, modal);
      submitButton(modal).click();
      fixture.detectChanges();
      http.expectOne((r) => r.method === 'POST' && r.url === ISSUES_URL).flush('boom', { status: 500, statusText: 'Server Error' });
      fixture.detectChanges();
      expect(titleInput(dialog()!).value).toBe('Brouillon');

      buttonByText(dialog()!, 'Annuler')!.click();
      fixture.detectChanges();

      await expectFreshForm(fixture, el);
    });
  });
});
