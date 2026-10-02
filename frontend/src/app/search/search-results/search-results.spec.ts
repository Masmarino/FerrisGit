import { LOCALE_ID } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { ActivatedRoute, convertToParamMap, provideRouter } from '@angular/router';
import { of, Subject, throwError } from 'rxjs';
import { Icon, SegmentedControl, UserChip, GbtToastService } from '@masmarino/gabarit';
import { SearchResults } from './search-results';
import { SearchIssueResult, SearchMergeRequestResult, SearchRepositoryRef, SearchRepositoryResult, SearchResponse, SearchService } from '../search.service';
import { StatusBadge } from '../../shared/layout/status-badge/status-badge';
import { PageTitleService } from '../../shell/page-title.service';

function response(overrides: Partial<SearchResponse> = {}): SearchResponse {
  return { repositories: [], issues: [], mergeRequests: [], users: [], ...overrides };
}

const ALICE_WIDGET: SearchRepositoryRef = { id: 'r1', name: 'widget', path: ['alice', 'widget'] };
// A group repository: three path segments, and every link has to keep all of them.
const GROUP_RUNNER: SearchRepositoryRef = { id: 'r2', name: 'runner', path: ['plateforme', 'infra', 'runner'] };

const hoursAgo = (hours: number) => new Date(Date.now() - hours * 3_600_000).toISOString();
const daysAgo = (days: number) => hoursAgo(days * 24);

function repository(fields: Partial<SearchRepositoryResult> = {}): SearchRepositoryResult {
  return { id: 'r1', name: 'widget', description: 'a widget repo', path: ['alice', 'widget'], visibility: 'private', ...fields };
}

function issue(fields: Partial<SearchIssueResult> = {}): SearchIssueResult {
  return { id: 'i1', number: 3, title: 'Crash', status: 'todo', kind: 'bug', createdAt: daysAgo(2), repository: ALICE_WIDGET, ...fields };
}

function mergeRequest(fields: Partial<SearchMergeRequestResult> = {}): SearchMergeRequestResult {
  return { id: 'm1', title: 'Fix', status: 'open', sourceBranch: 'fix', targetBranch: 'main', createdAt: hoursAgo(3), repository: ALICE_WIDGET, ...fields };
}

const FULL = response({
  repositories: [repository(), repository({ id: 'r2', name: 'runner', description: '', path: ['plateforme', 'infra', 'runner'], visibility: 'public' })],
  issues: [issue(), issue({ id: 'i2', number: 12, title: 'Runner lost', kind: 'feature', status: 'in_review', repository: GROUP_RUNNER })],
  mergeRequests: [mergeRequest(), mergeRequest({ id: 'm2', title: 'Retry jobs', status: 'merged', sourceBranch: 'feat/retry', targetBranch: 'develop', repository: GROUP_RUNNER })],
  users: [{ id: 'u1', username: 'alice' }],
});

describe('SearchResults', () => {
  function setup(initialQuery: string | null) {
    const queryParamMap = new Subject<ReturnType<typeof convertToParamMap>>();
    const searchServiceStub = { search: vi.fn(() => of(response())) };
    TestBed.configureTestingModule({
      providers: [provideRouter([]), { provide: SearchService, useValue: searchServiceStub }, { provide: ActivatedRoute, useValue: { queryParamMap } }, { provide: LOCALE_ID, useValue: 'fr' }],
    });
    const fixture = TestBed.createComponent(SearchResults);
    fixture.detectChanges();
    if (initialQuery !== null) {
      queryParamMap.next(convertToParamMap({ q: initialQuery }));
    } else {
      queryParamMap.next(convertToParamMap({}));
    }
    fixture.detectChanges();
    return { fixture, component: fixture.componentInstance, searchServiceStub, queryParamMap };
  }

  function searchFor(q: string, result: SearchResponse) {
    const context = setup(null);
    context.searchServiceStub.search.mockReturnValue(of(result));
    context.queryParamMap.next(convertToParamMap({ q }));
    context.fixture.detectChanges();
    return context;
  }

  const el = (fixture: { nativeElement: HTMLElement }) => fixture.nativeElement as HTMLElement;
  const text = (node: Element | null | undefined) => node?.textContent?.replace(/\s+/g, ' ').trim() ?? '';
  const sectionTitles = (fixture: { nativeElement: HTMLElement }) => Array.from(el(fixture).querySelectorAll('.search-results__section h2'), (h2) => text(h2));
  const section = (fixture: { nativeElement: HTMLElement }, title: string) =>
    Array.from(el(fixture).querySelectorAll<HTMLElement>('.search-results__section')).find((s) => text(s.querySelector('h2')) === title) ?? null;
  const sectionDe = (fixture: ComponentFixture<SearchResults>, title: string) =>
    fixture.debugElement.queryAll(By.css('.search-results__section')).find((de) => text(de.nativeElement.querySelector('h2')) === title)!;
  const tabs = (fixture: { nativeElement: HTMLElement }) => Array.from(el(fixture).querySelectorAll<HTMLButtonElement>('.search-results__tabs [role="radio"]'));
  const hrefs = (root: Element | null, selector: string) => Array.from(root?.querySelectorAll(selector) ?? [], (a) => a.getAttribute('href'));

  it('builds the page on the shared header and layout', () => {
    const { fixture } = setup('widget');
    expect(el(fixture).querySelector('gbt-page-header')).toBeTruthy();
    expect(el(fixture).querySelector('gbt-page-layout')).toBeTruthy();
  });

  it('titles the page with the query in guillemets', () => {
    const { fixture } = searchFor('widget', FULL);
    expect(text(el(fixture).querySelector('gbt-page-header h1'))).toBe('Résultats pour « widget »');
  });

  it('shows the same title in the shell header', () => {
    const { fixture } = searchFor('widget', FULL);
    fixture.detectChanges();
    expect(TestBed.inject(PageTitleService).title().replace(/\s+/g, ' ')).toBe('Résultats pour « widget »');
  });

  it('echoes the current query back to the user even when there are results', () => {
    const { fixture } = searchFor('widget', response({ users: [{ id: 'u1', username: 'alice' }] }));
    expect(el(fixture).textContent).toContain('widget');
  });

  it('says how many results were found under the title, with the right plural', () => {
    expect(text(el(searchFor('widget', FULL).fixture).querySelector('.gbt-page-header__meta'))).toBe('7 résultats');
    TestBed.resetTestingModule();
    expect(text(el(searchFor('widget', response({ users: [{ id: 'u1', username: 'alice' }] })).fixture).querySelector('.gbt-page-header__meta'))).toBe('1 résultat');
  });

  it('shows a hint and does not call the search service when there is no query', () => {
    const { fixture, searchServiceStub } = setup(null);
    expect(searchServiceStub.search).not.toHaveBeenCalled();
    expect(text(el(fixture).querySelector('gbt-empty-state'))).toContain('Saisissez un terme');
    expect(text(el(fixture).querySelector('gbt-page-header h1'))).toBe('Recherche');
    expect(el(fixture).querySelector('.search-results__tabs')).toBeNull();
  });

  it('treats a blank query like no query', () => {
    const { fixture, searchServiceStub } = setup('   ');
    expect(searchServiceStub.search).not.toHaveBeenCalled();
    expect(text(el(fixture).querySelector('gbt-empty-state'))).toContain('Saisissez un terme');
  });

  it('shows a no-results message naming the query, with suggestions, when every section is empty', () => {
    const { fixture } = setup('nonexistent');
    const message = el(fixture).querySelector('gbt-empty-state');
    expect(text(message?.querySelector('h2'))).toBe('Aucun résultat pour « nonexistent »');
    expect(message?.closest('section')?.getAttribute('aria-labelledby')).toBe(message?.querySelector('h2')?.id);
    expect(message?.closest('gbt-card')?.querySelector('.gbt-card')?.getAttribute('data-variant')).toBe('outlined');
    expect(message?.querySelectorAll('.search-results__suggestions li').length).toBeGreaterThanOrEqual(2);
    expect(el(fixture).querySelector('.search-results__tabs')).toBeNull();
    expect(el(fixture).querySelector('.search-results__section')).toBeNull();
  });

  it('offers a tab per category with its count, "Tout" first and selected', () => {
    const { fixture } = searchFor('widget', FULL);
    expect(tabs(fixture).map((tab) => text(tab))).toEqual(['Tout (7)', 'Dépôts (2)', 'Tickets (2)', 'Demandes de fusion (2)', 'Utilisateurs (1)']);
    expect(tabs(fixture)[0].getAttribute('aria-checked')).toBe('true');
    expect(el(fixture).querySelector('.search-results__tabs [role="radiogroup"]')?.getAttribute('aria-label')).toBe('Catégorie de résultats');
  });

  it('disables the tab of a category without any result', () => {
    const { fixture } = searchFor('widget', response({ users: [{ id: 'u1', username: 'alice' }] }));
    const byLabel = new Map(tabs(fixture).map((tab) => [text(tab), tab.disabled]));
    expect(byLabel.get('Tout (1)')).toBe(false);
    expect(byLabel.get('Dépôts (0)')).toBe(true);
    expect(byLabel.get('Utilisateurs (1)')).toBe(false);
  });

  it('shows every non-empty category under "Tout", and only the chosen one under a category tab', () => {
    const { fixture } = searchFor('widget', FULL);
    expect(sectionTitles(fixture)).toEqual(['Dépôts', 'Tickets', 'Demandes de fusion', 'Utilisateurs']);

    tabs(fixture)[2].click();
    fixture.detectChanges();
    expect(sectionTitles(fixture)).toEqual(['Tickets']);
    expect(tabs(fixture)[2].getAttribute('aria-checked')).toBe('true');

    tabs(fixture)[0].click();
    fixture.detectChanges();
    expect(sectionTitles(fixture)).toHaveLength(4);
  });

  it('goes back to "Tout" when a new search comes in', () => {
    const { fixture, searchServiceStub, queryParamMap } = searchFor('widget', FULL);
    tabs(fixture)[4].click();
    fixture.detectChanges();
    expect(sectionTitles(fixture)).toEqual(['Utilisateurs']);

    searchServiceStub.search.mockReturnValue(of(FULL));
    queryParamMap.next(convertToParamMap({ q: 'runner' }));
    fixture.detectChanges();
    expect(tabs(fixture)[0].getAttribute('aria-checked')).toBe('true');
    expect(sectionTitles(fixture)).toHaveLength(4);
  });

  it('reads "8+" for a category at the per-category cap, and notes that only the most relevant are shown', () => {
    const repositories = Array.from({ length: 8 }, (_, i) => repository({ id: `r${i}`, name: `widget-${i}`, path: ['alice', `widget-${i}`] }));
    const { fixture } = searchFor('widget', response({ repositories, users: [{ id: 'u1', username: 'alice' }] }));
    expect(tabs(fixture).map((tab) => text(tab)).slice(0, 2)).toEqual(['Tout (9+)', 'Dépôts (8+)']);
    expect(text(el(fixture).querySelector('.gbt-page-header__meta'))).toBe('9+ résultats');
    expect(text(section(fixture, 'Dépôts')?.querySelector('.search-results__count'))).toBe('8+');
    expect(text(section(fixture, 'Dépôts')?.querySelector('.search-results__capped'))).toContain('8 résultats les plus pertinents');
    expect(section(fixture, 'Utilisateurs')?.querySelector('.search-results__capped')).toBeNull();
  });

  it('renders each non-empty section and hides empty ones', () => {
    const { fixture } = searchFor('widget', response({ repositories: [repository()], users: [{ id: 'u1', username: 'alice' }] }));
    expect(sectionTitles(fixture)).toEqual(['Dépôts', 'Utilisateurs']);
    expect(el(fixture).textContent).toContain('alice/widget');
  });

  it('titles the issue and merge request sections "Tickets" and "Demandes de fusion", like the pages', () => {
    const { fixture } = searchFor('widget', response({ issues: [issue()], mergeRequests: [mergeRequest()] }));
    expect(sectionTitles(fixture)).toEqual(['Tickets', 'Demandes de fusion']);
  });

  it('shows a type icon and the count in the header of each populated section', () => {
    const { fixture } = searchFor('widget', FULL);
    const icons = fixture.debugElement.queryAll(By.css('.search-results__section')).map((de) => (de.query(By.directive(Icon))?.componentInstance as Icon | undefined)?.name());
    expect(icons).toEqual(['folder-git-2', 'circle-dot', 'git-pull-request', 'user']);
    expect(Array.from(el(fixture).querySelectorAll('.search-results__section-header .search-results__count'), (c) => text(c))).toEqual(['2', '2', '2', '1']);
    // The count sits outside the heading, so the section is named "Dépôts", not "Dépôts 2".
    for (const s of Array.from(el(fixture).querySelectorAll('.search-results__section'))) {
      expect(s.getAttribute('aria-labelledby')).toBe(s.querySelector('h2')?.id);
      const host = s.querySelector('gbt-card')!;
      const header = host.querySelector(':scope > .gbt-card__header');
      const box = host.querySelector(':scope > .gbt-card');
      expect(Array.from(host.children)).toEqual([header, box]);
      expect(box?.getAttribute('data-variant')).toBe('outlined');
      expect(box?.hasAttribute('data-flush')).toBe(true);
      expect(header?.querySelector('h2')).toBe(s.querySelector('h2'));
    }
  });

  it('lists the results as ul > li > gbt-list-row, with the title link as a direct child of the row', () => {
    const { fixture } = searchFor('widget', FULL);
    for (const title of ['Dépôts', 'Tickets', 'Demandes de fusion']) {
      const rows = section(fixture, title)!.querySelectorAll('ul.search-results__rows > li > gbt-list-row');
      expect(rows.length).toBe(2);
      for (const row of Array.from(rows)) {
        expect(row.querySelector('.gbt-list-row__title > a')).toBeTruthy();
      }
    }
  });

  it('links each repository by its full path, group repositories included, with its visibility', () => {
    const { fixture } = searchFor('widget', FULL);
    const repos = section(fixture, 'Dépôts');
    expect(hrefs(repos, '.gbt-list-row__title > a')).toEqual(['/repositories/alice/widget', '/repositories/plateforme/infra/runner']);
    expect(Array.from(repos!.querySelectorAll('.gbt-list-row__title > a'), (a) => text(a))).toEqual(['alice/widget', 'plateforme/infra/runner']);
    expect(text(repos!.querySelector('.search-results__description'))).toBe('a widget repo');
    expect(Array.from(repos!.querySelectorAll('.gbt-list-row__trailing gbt-badge'), (b) => text(b))).toEqual(['Privé', 'Public']);
    const leading = sectionDe(fixture, 'Dépôts').queryAll(By.css('.gbt-list-row__leading gbt-icon'));
    expect(leading.map((de) => (de.componentInstance as Icon).name())).toEqual(['lock', 'globe']);
  });

  it('links each issue by number, with its repository chip, kind icon, status badge and relative date', () => {
    const { fixture } = searchFor('widget', FULL);
    const issues = section(fixture, 'Tickets')!;
    expect(hrefs(issues, '.gbt-list-row__title > a')).toEqual(['/repositories/alice/widget/-/issues/3', '/repositories/plateforme/infra/runner/-/issues/12']);
    expect(Array.from(issues.querySelectorAll('.gbt-list-row__title > a'), (a) => text(a))).toEqual(['#3 Crash', '#12 Runner lost']);
    expect(issues.querySelector('.gbt-list-row__title > a')?.getAttribute('title')).toBe('Crash');
    expect(hrefs(issues, '.search-results__repo')).toEqual(['/repositories/alice/widget', '/repositories/plateforme/infra/runner']);
    expect(Array.from(issues.querySelectorAll('.search-results__repo'), (a) => text(a))).toEqual(['alice/widget', 'plateforme/infra/runner']);

    const badges = sectionDe(fixture, 'Tickets').queryAll(By.directive(StatusBadge));
    expect(badges.map((de) => [(de.componentInstance as StatusBadge).kind(), (de.componentInstance as StatusBadge).status()])).toEqual([
      ['issue', 'todo'],
      ['issue', 'in_review'],
    ]);
    expect(Array.from(issues.querySelectorAll('.gbt-list-row__trailing'), (t) => text(t))).toEqual(['À faire', 'En revue']);

    const kinds = Array.from(issues.querySelectorAll('.gbt-list-row__leading'), (l) => text(l));
    expect(kinds).toEqual(['Bug', 'Fonctionnalité']);

    const time = issues.querySelector('.search-results__opened time');
    // "avant-hier" is Gabarit's wording for exactly 2 days ago.
    expect(text(issues.querySelector('.search-results__opened'))).toBe('ouvert avant-hier');
    expect(time?.getAttribute('datetime')).toBe(FULL.issues[0].createdAt);
    expect(time?.getAttribute('title')).toMatch(/^\d{2}\/\d{2}\/\d{4}/);
  });

  it('links each merge request, with its repository chip, branches, status badge and relative date', () => {
    const { fixture } = searchFor('widget', FULL);
    const mergeRequests = section(fixture, 'Demandes de fusion')!;
    expect(hrefs(mergeRequests, '.gbt-list-row__title > a')).toEqual(['/repositories/alice/widget/-/merge-requests/m1', '/repositories/plateforme/infra/runner/-/merge-requests/m2']);
    expect(Array.from(mergeRequests.querySelectorAll('.gbt-list-row__title > a'), (a) => text(a))).toEqual(['Fix', 'Retry jobs']);
    expect(hrefs(mergeRequests, '.search-results__repo')).toEqual(['/repositories/alice/widget', '/repositories/plateforme/infra/runner']);
    expect(Array.from(mergeRequests.querySelectorAll('.search-results__branch'), (b) => text(b))).toEqual(['fix', 'main', 'feat/retry', 'develop']);
    expect(mergeRequests.querySelector('.search-results__branches')?.getAttribute('title')).toBe('fix → main');
    expect(Array.from(mergeRequests.querySelectorAll('.gbt-list-row__trailing'), (t) => text(t))).toEqual(['Ouverte', 'Fusionnée']);
    expect(text(mergeRequests.querySelector('.search-results__opened'))).toBe('ouverte il y a 3 h');
  });

  it('reads "ouvert le …" for an item older than a month (an absolute date)', () => {
    const { fixture } = searchFor('widget', response({ issues: [issue({ createdAt: '2020-03-04T10:00:00Z' })], mergeRequests: [mergeRequest({ createdAt: '2020-03-05T10:00:00Z' })] }));
    const opened = Array.from(el(fixture).querySelectorAll('.search-results__opened'), (o) => text(o));
    expect(opened).toEqual(['ouvert le 04/03/2020', 'ouverte le 05/03/2020']);
  });

  it('shows each user as a user chip, without a link (there is no profile page)', () => {
    const { fixture } = searchFor('widget', FULL);
    const users = section(fixture, 'Utilisateurs')!;
    const chips = fixture.debugElement.queryAll(By.directive(UserChip));
    expect(chips.map((de) => (de.componentInstance as UserChip).name())).toEqual(['alice']);
    expect(users.querySelector('ul.search-results__rows > li > gbt-list-row')).toBeTruthy();
    expect(users.querySelector('a')).toBeNull();
  });

  it('keeps the row link arrays across change detections (no fresh array bound to routerLink)', () => {
    const { fixture, component } = searchFor('widget', FULL);
    const before = (component as unknown as { issueRows: () => { link: string[] }[] }).issueRows();
    fixture.detectChanges();
    const after = (component as unknown as { issueRows: () => { link: string[] }[] }).issueRows();
    expect(after).toBe(before);
    expect(after[0].link).toBe(before[0].link);
  });

  it('shows a loading indicator while a search is in flight, announced to screen readers', () => {
    const { fixture, searchServiceStub, queryParamMap } = setup(null);
    const pending = new Subject<SearchResponse>();
    searchServiceStub.search.mockReturnValue(pending);

    queryParamMap.next(convertToParamMap({ q: 'widget' }));
    fixture.detectChanges();

    const status = el(fixture).querySelector('[role="status"]');
    expect(status).toBeTruthy();
    expect(status!.textContent!.toLowerCase()).toContain('recherche');
    const loading = el(fixture).querySelector('.search-results__loading');
    expect(loading?.getAttribute('aria-busy')).toBe('true');
    expect(loading?.querySelectorAll('gbt-skeleton').length).toBeGreaterThan(0);
    expect(loading?.querySelector('gbt-card[aria-hidden="true"] > .gbt-card__header gbt-skeleton')).toBeTruthy();
    expect(loading?.querySelector('gbt-card[aria-hidden="true"] > .gbt-card')?.getAttribute('data-variant')).toBe('outlined');
    // Its rows are silent: the page's status region is the only announcement.
    expect(loading?.querySelectorAll('gbt-card gbt-skeleton-list .gbt-skeleton-list__row').length).toBe(4);
    expect(text(loading?.querySelector('gbt-skeleton-list [role="status"]'))).toBe('');
    expect(el(fixture).querySelector('.search-results__tabs')).toBeNull();
    expect(text(el(fixture).querySelector('gbt-page-header h1'))).toBe('Résultats pour « widget »');
  });

  it('announces the outcome in the status region once the search is done', () => {
    const { fixture } = searchFor('widget', FULL);
    expect(text(el(fixture).querySelector('[role="status"]'))).toBe('7 résultats trouvés.');
  });

  it('does not show stale results from a previous query while the next search is loading', () => {
    const { fixture, searchServiceStub, queryParamMap } = setup(null);
    searchServiceStub.search.mockReturnValue(of(response({ users: [{ id: 'u1', username: 'first-query-result' }] })));
    queryParamMap.next(convertToParamMap({ q: 'first' }));
    fixture.detectChanges();
    expect(el(fixture).textContent).toContain('first-query-result');

    const pending = new Subject<SearchResponse>();
    searchServiceStub.search.mockReturnValue(pending);
    queryParamMap.next(convertToParamMap({ q: 'second' }));
    fixture.detectChanges();

    expect(el(fixture).textContent).not.toContain('first-query-result');
  });

  it('cancels a slower in-flight search when a newer query is submitted', () => {
    const { fixture, searchServiceStub, queryParamMap } = setup(null);
    const first = new Subject<SearchResponse>();
    const second = new Subject<SearchResponse>();
    searchServiceStub.search.mockReturnValueOnce(first).mockReturnValueOnce(second);

    queryParamMap.next(convertToParamMap({ q: 'first' }));
    queryParamMap.next(convertToParamMap({ q: 'second' }));
    second.next(response({ users: [{ id: 'u1', username: 'second-result' }] }));
    first.next(response({ users: [{ id: 'u2', username: 'first-result' }] }));
    fixture.detectChanges();

    expect(el(fixture).textContent).toContain('second-result');
    expect(el(fixture).textContent).not.toContain('first-result');
  });

  it('shows an error toast and an error card offering to retry when the search fails', () => {
    const { fixture, searchServiceStub, queryParamMap } = setup(null);
    const showSpy = vi.spyOn(TestBed.inject(GbtToastService), 'show');
    searchServiceStub.search.mockReturnValue(throwError(() => new Error('Network error')));

    queryParamMap.next(convertToParamMap({ q: 'test-query' }));
    fixture.detectChanges();

    expect(showSpy).toHaveBeenCalledWith("Impossible d'effectuer la recherche. Réessayez plus tard.", 'error');
    const error = el(fixture).querySelector('.search-results__failed');
    expect(text(error?.querySelector('h2'))).toBe("La recherche n'a pas abouti");
    expect(error?.querySelector('h2')?.id).toBe(error?.getAttribute('aria-labelledby'));
    expect(error?.querySelector('.gbt-empty-state')?.getAttribute('data-tone')).toBe('error');
    expect(el(fixture).textContent).not.toContain('Aucun résultat');
    expect(el(fixture).querySelector('.search-results__tabs')).toBeNull();
    expect(text(el(fixture).querySelector('[role="status"]'))).toBe('La recherche a échoué.');

    searchServiceStub.search.mockReturnValue(of(response({ users: [{ id: 'u1', username: 'alice' }] })));
    (error!.querySelector('gbt-button button') as HTMLButtonElement).click();
    fixture.detectChanges();

    expect(searchServiceStub.search).toHaveBeenLastCalledWith('test-query');
    expect(searchServiceStub.search).toHaveBeenCalledTimes(2);
    expect(sectionTitles(fixture)).toEqual(['Utilisateurs']);
  });

  it('uses a secondary button to retry (no primary action on the page)', () => {
    const { fixture, searchServiceStub, queryParamMap } = setup(null);
    searchServiceStub.search.mockReturnValue(throwError(() => new Error('Network error')));
    queryParamMap.next(convertToParamMap({ q: 'test-query' }));
    fixture.detectChanges();
    expect(el(fixture).querySelector('.search-results__failed .gbt-button--secondary')).toBeTruthy();
    expect(el(fixture).querySelector('.gbt-button--primary')).toBeNull();
  });

  it('uses the segmented control for the tabs', () => {
    const { fixture } = searchFor('widget', FULL);
    expect(fixture.debugElement.query(By.directive(SegmentedControl))).toBeTruthy();
  });
});
