import { LOCALE_ID } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { provideRouter } from '@angular/router';
import { Icon, GbtToastService } from '@masmarino/gabarit';
import { HomePage } from './home-page';
import { DashboardResponse } from '../dashboard.service';
import { Notification } from '../../notifications/notifications.service';
import { SearchIssueResult, SearchMergeRequestResult } from '../../search/search.service';
import { MeService } from '../../shell/me.service';

const DAY = 24 * 60 * 60 * 1000;
const daysAgo = (days: number) => new Date(Date.now() - days * DAY).toISOString();
const hoursAgo = (hours: number) => new Date(Date.now() - hours * 60 * 60 * 1000).toISOString();

const REPO = { id: 'r1', name: 'hello', path: ['alice', 'hello'] };

function issue(overrides: Partial<SearchIssueResult> = {}): SearchIssueResult {
  return { id: 'i1', number: 5, title: 'Fix the bug', status: 'todo', kind: 'bug', createdAt: daysAgo(2), repository: REPO, ...overrides };
}

function mergeRequest(overrides: Partial<SearchMergeRequestResult> = {}): SearchMergeRequestResult {
  return { id: 'mr1', title: 'Add search filters', status: 'open', sourceBranch: 'feat/filters', targetBranch: 'main', createdAt: hoursAgo(3), repository: REPO, ...overrides };
}

function notification(overrides: Partial<Notification> = {}): Notification {
  return {
    id: 'n1',
    kind: 'issue_closed',
    repositoryOwner: 'alice',
    repositoryName: 'hello',
    actorUsername: 'bob',
    mergeRequestId: null,
    mergeRequestTitle: null,
    pipelineId: null,
    commitSha: null,
    role: null,
    issueId: 'i1',
    issueNumber: 3,
    issueTitle: 'Fixed',
    read: false,
    createdAt: hoursAgo(5),
    ...overrides,
  };
}

const text = (node: Element | null | undefined) => (node?.textContent ?? '').replace(/\s+/g, ' ').trim();

describe('HomePage', () => {
  function setup(options: { username?: string } = {}) {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([]), { provide: LOCALE_ID, useValue: 'fr' }] });
    if (options.username !== undefined) {
      TestBed.inject(MeService).username.set(options.username);
    }
    const fixture = TestBed.createComponent(HomePage);
    const http = TestBed.inject(HttpTestingController);
    const el: HTMLElement = fixture.nativeElement;
    return { fixture, http, el };
  }

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  function flushDashboard(http: HttpTestingController, overrides: Partial<DashboardResponse> = {}) {
    http.expectOne('/api/dashboard').flush({
      assignedIssues: [],
      authoredIssues: [],
      authoredMergeRequests: [],
      mergeRequestsToReview: [],
      activity: [],
      ...overrides,
    });
  }

  function loaded(overrides: Partial<DashboardResponse> = {}, options: { username?: string } = {}) {
    const ctx = setup(options);
    ctx.fixture.detectChanges();
    flushDashboard(ctx.http, overrides);
    ctx.fixture.detectChanges();
    return ctx;
  }

  const tiles = (el: HTMLElement) => Array.from(el.querySelectorAll<HTMLElement>('gbt-stat-tile'));
  const tile = (el: HTMLElement, label: string) => tiles(el).find((t) => text(t.querySelector('.gbt-stat-tile__label')) === label);
  const tileCount = (el: HTMLElement, label: string) => text(tile(el, label)?.querySelector('.gbt-stat-tile__value'));
  const group = (el: HTMLElement, heading: string) =>
    Array.from(el.querySelectorAll<HTMLElement>('.home-page__group')).find((g) => text(g.querySelector('h3')).startsWith(heading));
  const rows = (root: Element | undefined) => Array.from(root?.querySelectorAll<HTMLElement>('gbt-list-row') ?? []);
  const events = (el: HTMLElement) => Array.from(el.querySelectorAll<HTMLElement>('.home-page__event'));

  describe('header', () => {
    it('titles the page "Accueil" and greets the current user', () => {
      const { el } = loaded({}, { username: 'alice' });

      expect(text(el.querySelector('h1'))).toBe('Accueil');
      expect(text(el.querySelector('.home-page__greeting'))).toBe('Bonjour, alice');
    });

    it('shows no greeting while the current user is not known yet', () => {
      const { el } = loaded({}, { username: '' });

      expect(text(el.querySelector('h1'))).toBe('Accueil');
      expect(el.querySelector('.home-page__greeting')).toBeNull();
    });
  });

  describe('stat tiles', () => {
    it('shows one tile per category, with its count, under the renamed labels', () => {
      const { el } = loaded({
        assignedIssues: [issue({ id: '1' }), issue({ id: '2' })],
        authoredIssues: [issue({ id: '3' })],
        authoredMergeRequests: [mergeRequest({ id: 'a' }), mergeRequest({ id: 'b' }), mergeRequest({ id: 'c' })],
        mergeRequestsToReview: [],
      });

      expect(tiles(el).map((t) => text(t.querySelector('.gbt-stat-tile__label')))).toEqual(['Tickets assignés', 'Tickets créés', 'Mes demandes de fusion', 'À relire']);
      expect(tileCount(el, 'Tickets assignés')).toBe('2');
      expect(tileCount(el, 'Tickets créés')).toBe('1');
      expect(tileCount(el, 'Mes demandes de fusion')).toBe('3');
      expect(tileCount(el, 'À relire')).toBe('0');
    });

    it('shows "20+" when a category is at its cap (20), the exact count below it', () => {
      const twenty = Array.from({ length: 20 }, (_, i) => issue({ id: String(i), number: i, title: `issue ${i}` }));
      const nineteen = twenty.slice(1);
      const { el } = loaded({ assignedIssues: twenty, authoredIssues: nineteen });

      expect(tileCount(el, 'Tickets assignés')).toBe('20+');
      expect(tileCount(el, 'Tickets créés')).toBe('19');
    });

    it('shows a type icon on each stat tile', () => {
      const { fixture } = loaded();

      const iconOf = (label: string) => {
        const de = fixture.debugElement.queryAll(By.css('gbt-stat-tile')).find((t) => text(t.nativeElement.querySelector('.gbt-stat-tile__label')) === label);
        return (de?.query(By.directive(Icon))?.componentInstance as Icon | undefined)?.name();
      };
      expect(iconOf('Tickets assignés')).toBe('circle-dot');
      expect(iconOf('Tickets créés')).toBe('pencil');
      expect(iconOf('Mes demandes de fusion')).toBe('git-pull-request');
      expect(iconOf('À relire')).toBe('eye');
    });

    it('lays the tiles out as a list', () => {
      const { el } = loaded();

      const list = el.querySelector('gbt-stat-grid [role="list"]');
      expect(list?.getAttribute('aria-label')).toBe('Résumé');
      expect(tiles(el).every((t) => t.getAttribute('role') === 'listitem' && list?.contains(t))).toBe(true);
    });

    it('steps a tile back (muted) when its category is empty, and only then', () => {
      const { el } = loaded({ assignedIssues: [issue({ id: '1' })], authoredIssues: [], authoredMergeRequests: [mergeRequest({ id: 'a' })], mergeRequestsToReview: [] });

      expect(tiles(el).map((t) => [text(t.querySelector('.gbt-stat-tile__label')), t.hasAttribute('data-muted')])).toEqual([
        ['Tickets assignés', false],
        ['Tickets créés', true],
        ['Mes demandes de fusion', false],
        ['À relire', true],
      ]);
    });

    it('marks each tile with its category', () => {
      const { el } = loaded();

      expect(tiles(el).map((t) => t.getAttribute('data-kind'))).toEqual(['issue', 'issue', 'merge-request', 'merge-request']);
    });

    it('never uses the old "Issues" / "Merge Requests" labels', () => {
      const { el } = loaded({ assignedIssues: [issue()], authoredIssues: [issue({ id: '2' })], authoredMergeRequests: [mergeRequest()], mergeRequestsToReview: [mergeRequest({ id: 'mr2' })] });

      expect(el.textContent).not.toContain('Issues assignées');
      expect(el.textContent).not.toContain('Issues créées');
      expect(el.textContent).not.toContain('Merge Request');
      expect(el.textContent).not.toContain('Vos demandes de fusion');
    });
  });

  describe('"À traiter" card', () => {
    it('groups the items under one heading per category, in the tiles order', () => {
      const { el } = loaded({ assignedIssues: [issue()] });

      const card = el.querySelector('.home-page__todo');
      expect(text(card?.querySelector('h2'))).toContain('À traiter');
      const header = card?.querySelector(':scope > .gbt-card__header');
      const box = card?.querySelector(':scope > .gbt-card');
      expect(Array.from(card!.children)).toEqual([header, box]);
      expect(box?.getAttribute('data-variant')).toBe('outlined');
      expect(box?.hasAttribute('data-flush')).toBe(true);
      expect(header?.querySelector('.gbt-card__title')?.textContent).toContain('À traiter');
      expect(header?.querySelector('.gbt-card__icon gbt-icon')).toBeTruthy();
      expect(Array.from(card!.querySelectorAll('.home-page__group h3')).map((h) => text(h.querySelector('.home-page__group-label')))).toEqual([
        'Tickets assignés',
        'Tickets créés',
        'Mes demandes de fusion',
        'À relire',
      ]);
    });

    it('lists an assigned issue with its number, a working title link as a direct child of the row, and the full title on hover', () => {
      const { el } = loaded({ assignedIssues: [issue()] });

      const [row] = rows(group(el, 'Tickets assignés'));
      const link = el.querySelector<HTMLAnchorElement>('a[href="/repositories/alice/hello/-/issues/5"]');
      expect(link).toBeTruthy();
      expect(link!.parentElement!.classList).toContain('gbt-list-row__title');
      expect(link!.closest('gbt-list-row')).toBe(row);
      expect(text(link)).toBe('#5 Fix the bug');
      expect(link!.getAttribute('title')).toBe('Fix the bug');
    });

    it('shows the repository as a chip linking to the repository', () => {
      const { el } = loaded({ assignedIssues: [issue({ repository: { id: 'r2', name: 'api', path: ['acme', 'backend', 'api'] } })] });

      const [row] = rows(group(el, 'Tickets assignés'));
      const chip = row.querySelector<HTMLAnchorElement>('.home-page__repo');
      expect(text(chip)).toBe('acme/backend/api');
      expect(chip!.querySelector('gbt-badge .gbt-badge__label')).toBeTruthy();
      expect(chip!.getAttribute('href')).toBe('/repositories/acme/backend/api');
    });

    it('shows when the item was opened as a relative date, with the exact date on hover', () => {
      const created = daysAgo(2);
      const { el } = loaded({ assignedIssues: [issue({ createdAt: created })], authoredMergeRequests: [mergeRequest({ createdAt: hoursAgo(3) })] });

      const issueTime = rows(group(el, 'Tickets assignés'))[0].querySelector('time');
      // "avant-hier" is Gabarit's own wording for a gap of exactly 2 days.
      expect(text(issueTime?.parentElement)).toContain('ouvert avant-hier');
      expect(issueTime!.getAttribute('datetime')).toBe(created);
      expect(issueTime!.getAttribute('title')).toMatch(/^\d{2}\/\d{2}\/\d{4} \d{2}:\d{2}$/);

      const mrTime = rows(group(el, 'Mes demandes de fusion'))[0].querySelector('time');
      expect(text(mrTime?.parentElement)).toContain('ouverte il y a 3 h');
    });

    it('ends each row with the status badge of its kind', () => {
      const { el } = loaded({ authoredIssues: [issue({ status: 'in_review' })], mergeRequestsToReview: [mergeRequest({ id: 'mr2', status: 'open' })] });

      const issueBadge = rows(group(el, 'Tickets créés'))[0].querySelector('.gbt-list-row__trailing fg-status-badge');
      expect(text(issueBadge)).toBe('En revue');
      const mrBadge = rows(group(el, 'À relire'))[0].querySelector('.gbt-list-row__trailing fg-status-badge');
      expect(text(mrBadge)).toBe('Ouverte');
    });

    it('links merge requests to their page', () => {
      const { el } = loaded({ authoredMergeRequests: [mergeRequest()], mergeRequestsToReview: [mergeRequest({ id: 'mr2', title: 'Fix pagination' })] });

      const mine = el.querySelector<HTMLAnchorElement>('a[href="/repositories/alice/hello/-/merge-requests/mr1"]');
      expect(text(mine)).toBe('Add search filters');
      expect(mine!.parentElement!.classList).toContain('gbt-list-row__title');
      expect(text(group(el, 'À relire')?.querySelector('a[href="/repositories/alice/hello/-/merge-requests/mr2"]'))).toBe('Fix pagination');
    });

    it('shows the issue kind icon, and the merge request icon, at the start of each row', () => {
      const { fixture } = loaded({ assignedIssues: [issue({ kind: 'feature' })], authoredMergeRequests: [mergeRequest()] });

      const leadingIcons = fixture.debugElement
        .queryAll(By.css('.gbt-list-row__leading'))
        .map((de) => (de.query(By.directive(Icon))?.componentInstance as Icon | undefined)?.name());
      expect(leadingIcons).toEqual(['sparkles', 'git-pull-request']);
    });

    it('shows the count of each group in its heading, capped like the tiles', () => {
      const twenty = Array.from({ length: 20 }, (_, i) => mergeRequest({ id: `mr${i}` }));
      const { el } = loaded({ assignedIssues: [issue()], mergeRequestsToReview: twenty });

      expect(text(group(el, 'Tickets assignés')?.querySelector('.home-page__group-count'))).toBe('1');
      expect(text(group(el, 'À relire')?.querySelector('.home-page__group-count'))).toBe('20+');
      expect(rows(group(el, 'À relire')).length).toBe(20);
    });

    it('says a category with nothing in it is up to date', () => {
      const { el } = loaded({ assignedIssues: [issue()] });

      expect(rows(group(el, 'Tickets assignés')).length).toBe(1);
      expect(group(el, 'Tickets assignés')?.querySelector('.home-page__group-empty')).toBeNull();
      for (const heading of ['Tickets créés', 'Mes demandes de fusion', 'À relire']) {
        expect(rows(group(el, heading)).length).toBe(0);
        expect(text(group(el, heading)?.querySelector('.home-page__group-empty'))).toBe('Tout est à jour');
      }
    });

    it('replaces the card with one friendly empty state when there is nothing at all to do', () => {
      const { el } = loaded();

      expect(el.querySelector('.home-page__todo')).toBeNull();
      const emptyState = el.querySelector('gbt-empty-state');
      expect(emptyState).toBeTruthy();
      expect(emptyState!.getAttribute('illustration')).toBe('checklist');
      expect(text(emptyState)).toContain("Rien à traiter pour l'instant");
      const host = emptyState!.closest('gbt-card')!;
      const box = host.querySelector(':scope > .gbt-card');
      expect(Array.from(host.children)).toEqual([box]);
      expect(box?.getAttribute('data-variant')).toBe('outlined');
      expect(box?.hasAttribute('data-flush')).toBe(true);
      expect(tiles(el).map((t) => text(t.querySelector('.gbt-stat-tile__value')))).toEqual(['0', '0', '0', '0']);
    });
  });

  describe('recent activity', () => {
    it('renders an activity entry via the shared notification sentence, in the aside', () => {
      const { el } = loaded({ activity: [notification()] });

      const aside = el.querySelector('[page-aside]');
      expect(text(aside?.querySelector('h2'))).toBe('Activité récente');
      expect(text(aside)).toContain('bob a fermé un ticket sur alice/hello');
      const link = aside!.querySelector<HTMLAnchorElement>('a[href="/repositories/alice/hello/-/issues/3"]');
      expect(link?.textContent).toContain('bob a fermé un ticket sur alice/hello');
    });

    it('links a collaborator_added activity entry to the collaborators section of the repository settings', () => {
      const { el } = loaded({ activity: [notification({ id: 'n2', kind: 'collaborator_added', role: 'reader', issueId: null, issueNumber: null, issueTitle: null })] });

      const link: HTMLAnchorElement = el.querySelector('a[href="/repositories/alice/hello/-/settings?section=collaborators"]')!;
      expect(link?.textContent).toContain('bob vous a ajouté comme lecteur sur alice/hello');
    });

    it('keeps the « guillemets » attached to the quoted title (non-breaking spaces)', () => {
      const { el } = loaded({ activity: [notification({ kind: 'merge_request_approved', mergeRequestId: 'mr9', mergeRequestTitle: 'Refonte', issueId: null, issueNumber: null })] });

      expect(events(el)[0].querySelector('a')!.textContent).toContain('bob a approuvé votre demande de fusion « Refonte »');
    });

    it('shows when each entry happened as a relative date', () => {
      const created = hoursAgo(5);
      const { el } = loaded({ activity: [notification({ createdAt: created })] });

      const time = events(el)[0].querySelector('time');
      expect(text(time)).toBe('il y a 5 h');
      expect(time!.getAttribute('datetime')).toBe(created);
      expect(time!.getAttribute('title')).toMatch(/^\d{2}\/\d{2}\/\d{4} \d{2}:\d{2}$/);
    });

    it('emphasises unread entries, not by colour alone, and counts them in the panel heading row', () => {
      const { el } = loaded({ activity: [notification({ id: 'a', read: false }), notification({ id: 'b', read: true }), notification({ id: 'c', read: false })] });

      const [first, second] = events(el);
      expect(first.classList).toContain('home-page__event--unread');
      expect(text(first.querySelector('.sr-only'))).toBe('Non lue');
      expect(second.classList).not.toContain('home-page__event--unread');
      expect(second.querySelector('.sr-only')).toBeNull();
      expect(text(el.querySelector('.home-page__unread-count'))).toBe('2 non lues');
    });

    it('says "1 non lue" in the singular, and nothing when everything is read', () => {
      const one = loaded({ activity: [notification({ read: false })] });
      expect(text(one.el.querySelector('.home-page__unread-count'))).toBe('1 non lue');
      TestBed.resetTestingModule();

      const none = loaded({ activity: [notification({ read: true })] });
      expect(none.el.querySelector('.home-page__unread-count')).toBeNull();
    });

    it('says there is no recent activity when the feed is empty', () => {
      const { el } = loaded();

      expect(events(el).length).toBe(0);
      expect(text(el.querySelector('.home-page__activity-empty'))).toBe('Aucune activité récente');
    });
  });

  describe('loading and errors', () => {
    it('shows placeholder tiles and rows, announced once, until the dashboard arrives', () => {
      const { fixture, http, el } = setup();
      fixture.detectChanges();

      const loading = el.querySelector('.home-page__loading');
      // One polite status (the stat grid's), outside any aria-busy region, since a busy ancestor can hold back
      // the announcement. The card's placeholder rows stay silent.
      expect(loading?.querySelector('[aria-busy="true"]')).toBeNull();
      // The skeleton list's own status is a second region, required but silenced with an empty `loadingLabel`. Gabarit gap: it can't be left out.
      const statuses = Array.from(loading!.querySelectorAll('[role="status"]'), (status) => text(status));
      expect(statuses[0]).toBe('Chargement du tableau de bord…');
      expect(statuses.slice(1).every((status) => status === '')).toBe(true);
      expect(loading!.querySelectorAll('.gbt-stat-grid__placeholder').length).toBe(4);
      expect(loading!.querySelectorAll('gbt-skeleton-list .gbt-skeleton-list__row').length).toBe(4);
      expect(tiles(el).length).toBe(0);
      const skeleton = loading!.querySelector('gbt-card[aria-hidden="true"]');
      expect(skeleton?.querySelector(':scope > .gbt-card')?.getAttribute('data-variant')).toBe('outlined');
      expect(skeleton?.querySelector(':scope > .gbt-card__header gbt-skeleton')).toBeTruthy();

      flushDashboard(http, { assignedIssues: [issue()] });
      fixture.detectChanges();

      expect(el.querySelector('.home-page__loading')).toBeNull();
      expect(tiles(el).length).toBe(4);
    });

    it('shows an error toast and an error card with a retry when the dashboard request fails', () => {
      const { fixture, http, el } = setup();
      const showSpy = vi.spyOn(TestBed.inject(GbtToastService), 'show');
      fixture.detectChanges();
      http.expectOne('/api/dashboard').flush(null, { status: 500, statusText: 'Server Error' });
      fixture.detectChanges();

      expect(showSpy).toHaveBeenCalledWith('Impossible de charger le tableau de bord. Réessayez plus tard.', 'error');
      const error = el.querySelector('gbt-alert');
      expect(text(error)).toContain("Le tableau de bord n'a pas pu être chargé");
      expect(text(error)).toContain('Vérifiez votre connexion, puis réessayez.');
      // The toast announces the failure and the card shows it silently, so there is one live region, not two.
      const box = error!.querySelector('.gbt-alert')!;
      expect(box.getAttribute('data-variant')).toBe('error');
      expect(box.getAttribute('role')).toBeNull();
      expect(box.getAttribute('aria-live')).toBeNull();
      expect(tiles(el).length).toBe(0);
      expect(el.querySelector('gbt-empty-state')).toBeNull();
      expect(el.querySelector('.home-page__loading')).toBeNull();

      const retry = Array.from(error!.querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === 'Réessayer');
      expect(retry?.classList).toContain('gbt-button--secondary');
      retry!.click();
      fixture.detectChanges();
      expect(el.querySelector('.home-page__loading')).toBeTruthy();

      flushDashboard(http, { assignedIssues: [issue()] });
      fixture.detectChanges();
      expect(el.querySelector('gbt-alert')).toBeNull();
      expect(tileCount(el, 'Tickets assignés')).toBe('1');
    });

    it('keeps the same row link arrays across change detections (no fresh literal bound to routerLink)', () => {
      const { fixture } = loaded({ assignedIssues: [issue()], activity: [notification()] });
      const component = fixture.componentInstance as unknown as { todoGroups: () => { rows: { link: string[] }[] }[]; activityRows: () => { link: string[] }[] };

      const before = [component.todoGroups()[0].rows[0].link, component.activityRows()[0].link];
      fixture.detectChanges();
      expect([component.todoGroups()[0].rows[0].link, component.activityRows()[0].link]).toEqual(before);
      expect(component.todoGroups()[0].rows[0].link).toBe(before[0]);
      expect(component.activityRows()[0].link).toBe(before[1]);
    });
  });
});
