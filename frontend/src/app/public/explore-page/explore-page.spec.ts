import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting, TestRequest } from '@angular/common/http/testing';
import { provideRouter, Router } from '@angular/router';
import { RouterTestingHarness } from '@angular/router/testing';
import { of } from 'rxjs';
import { ExplorePage, SEARCH_DEBOUNCE_MS } from './explore-page';
import { PublicConfigService } from '../public-config.service';
import { PublicCatalogPage, PublicRepositorySummary } from '../public-repositories.service';
import { PageTitleService } from '../../shell/page-title.service';

const CATALOG = '/api/public/repositories';
const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();

const repo = (overrides: Partial<PublicRepositorySummary> = {}): PublicRepositorySummary => ({
  id: 'repo-1',
  name: 'hello',
  path: ['alice', 'hello'],
  owner: 'alice',
  description: 'Un premier dépôt',
  stars: 12,
  createdAt: '2026-09-01T10:00:00Z',
  ...overrides,
});

const page = (items: PublicRepositorySummary[], total = items.length, pageNumber = 1): PublicCatalogPage => ({ items, total, page: pageNumber, perPage: 20 });

async function setup(url = '/explore', options: { publicPagesEnabled?: boolean | null } = {}) {
  const pageTitle = { title: () => '', set: vi.fn() };
  TestBed.configureTestingModule({
    providers: [
      provideHttpClient(),
      provideHttpClientTesting(),
      provideRouter([{ path: 'explore', component: ExplorePage }]),
      { provide: PublicConfigService, useValue: { publicPagesEnabled: () => of(options.publicPagesEnabled === undefined ? true : options.publicPagesEnabled) } },
      { provide: PageTitleService, useValue: pageTitle },
    ],
  });
  const harness = await RouterTestingHarness.create(url);
  const http = TestBed.inject(HttpTestingController);
  const el = () => harness.routeNativeElement as HTMLElement;
  const refresh = async () => {
    harness.fixture.detectChanges();
    await harness.fixture.whenStable();
    harness.fixture.detectChanges();
  };
  const search = () => http.expectOne((r) => r.url === CATALOG);
  const answer = async (request: TestRequest, body: PublicCatalogPage) => {
    request.flush(body);
    await refresh();
  };
  return { harness, http, el, refresh, search, answer, pageTitle, router: TestBed.inject(Router) };
}

describe('ExplorePage', () => {
  afterEach(() => {
    vi.useRealTimers();
    TestBed.inject(HttpTestingController).verify();
  });

  it('is titled "Explorer" with its own visible heading', async () => {
    const { el, pageTitle, search, answer } = await setup();
    await answer(search(), page([]));

    expect(pageTitle.set).toHaveBeenCalledWith('Explorer');
    expect(text(el().querySelector('h1'))).toBe('Explorer les dépôts publics');
  });

  it('asks for the most starred repositories, page 1, by default', async () => {
    const { search, answer } = await setup();

    const request = search();
    expect(request.request.params.has('q')).toBe(false);
    expect(request.request.params.get('sort')).toBe('stars');
    expect(request.request.params.get('page')).toBe('1');
    await answer(request, page([]));
  });

  it('takes the query, sort and page from the URL', async () => {
    const { search, answer, el } = await setup('/explore?q=rust&sort=name&page=2');

    const request = search();
    expect(request.request.params.get('q')).toBe('rust');
    expect(request.request.params.get('sort')).toBe('name');
    expect(request.request.params.get('page')).toBe('2');
    await answer(request, page([repo()], 21, 2));
    expect(el().querySelector<HTMLInputElement>('form[role="search"] input')!.value).toBe('rust');
    expect(text(el().querySelector('.explore-page__heading'))).toBe('Résultats pour « rust »');
  });

  it('falls back to the defaults for a sort or page it does not know', async () => {
    const { search, answer } = await setup('/explore?sort=downloads&page=-3');

    const request = search();
    expect(request.request.params.get('sort')).toBe('stars');
    expect(request.request.params.get('page')).toBe('1');
    await answer(request, page([]));
  });

  it('shows each repository as a card: its path linking to its page, description, stars and creation date', async () => {
    const { el, search, answer } = await setup();
    await answer(search(), page([repo(), repo({ id: 'repo-2', name: 'cli', path: ['acme', 'tools', 'cli'], description: '', stars: 1 })]));

    const cards = Array.from(el().querySelectorAll('.explore-page__results > li'));
    expect(cards).toHaveLength(2);
    const link = cards[0].querySelector<HTMLAnchorElement>('h3 a')!;
    expect(text(link)).toBe('alice / hello');
    expect(link.getAttribute('href')).toBe('/repositories/alice/hello');
    expect(text(cards[0].querySelector('.repository-card__description'))).toBe('Un premier dépôt');
    expect(text(cards[0].querySelector('.repository-card__stars'))).toBe('12 12 étoiles');
    expect(cards[0].querySelector('time')?.getAttribute('datetime')).toBe('2026-09-01T10:00:00Z');
    expect(cards[1].querySelector('h3 a')?.getAttribute('href')).toBe('/repositories/acme/tools/cli');
    expect(cards[1].querySelector('.repository-card__description')).toBeNull();
    expect(text(el().querySelector('.explore-page__count'))).toBe('2 dépôts');
  });

  describe('search box', () => {
    function type(el: HTMLElement, value: string) {
      const input = el.querySelector<HTMLInputElement>('form[role="search"] input')!;
      input.value = value;
      input.dispatchEvent(new Event('input'));
    }

    it(`searches ${SEARCH_DEBOUNCE_MS} ms after the last keystroke, not before`, async () => {
      const { el, search, answer, router } = await setup();
      await answer(search(), page([]));
      vi.useFakeTimers();
      const navigate = vi.spyOn(router, 'navigate').mockResolvedValue(true);

      type(el(), 'fer');
      vi.advanceTimersByTime(SEARCH_DEBOUNCE_MS - 1);
      type(el(), 'ferris ');
      vi.advanceTimersByTime(SEARCH_DEBOUNCE_MS - 1);
      expect(navigate).not.toHaveBeenCalled();

      vi.advanceTimersByTime(1);
      expect(navigate).toHaveBeenCalledExactlyOnceWith([], expect.objectContaining({ queryParams: { q: 'ferris', page: null }, queryParamsHandling: 'merge', replaceUrl: true }));
    });

    it('searches at once on Enter, and starts again from page 1', async () => {
      const { el, search, answer, refresh, router } = await setup('/explore?page=3');
      await answer(search(), page([repo()], 60, 3));

      type(el(), 'ferris');
      el().querySelector('form[role="search"]')!.dispatchEvent(new Event('submit', { cancelable: true }));
      await refresh();

      expect(router.url).toBe('/explore?q=ferris');
      const request = search();
      expect(request.request.params.get('q')).toBe('ferris');
      expect(request.request.params.get('page')).toBe('1');
      await answer(request, page([]));
    });
  });

  it('sorts by the chosen order, starting again from page 1', async () => {
    const { el, search, answer, refresh, router } = await setup('/explore?page=2');
    await answer(search(), page([repo()], 30, 2));

    const options = Array.from(el().querySelectorAll<HTMLButtonElement>('.explore-page__sort [role="radio"]'));
    expect(options.map(text)).toEqual(['Populaires', 'Nom', 'Récents']);
    options[2].click();
    await refresh();

    expect(router.url).toBe('/explore?sort=created');
    await answer(search(), page([]));
  });

  describe('pagination', () => {
    it('links to the next page and greys out the previous one on page 1', async () => {
      const { el, search, answer } = await setup();
      await answer(search(), page([repo()], 45));

      const nav = el().querySelector('nav.explore-page__pagination')!;
      expect(nav.getAttribute('aria-label')).toBe('Pages de résultats');
      expect(text(nav.querySelector('.explore-page__page'))).toBe('Page 1 sur 3');
      expect(nav.querySelector('.explore-page__previous button')?.hasAttribute('disabled')).toBe(true);
      expect(nav.querySelector<HTMLAnchorElement>('a.explore-page__next')?.getAttribute('href')).toBe('/explore?page=2');
    });

    it('links back to page 1 without a page parameter, and greys out the next one on the last page', async () => {
      const { el, search, answer } = await setup('/explore?q=x&page=2');
      await answer(search(), page([repo()], 21, 2));

      const nav = el().querySelector('nav.explore-page__pagination')!;
      expect(nav.querySelector<HTMLAnchorElement>('a.explore-page__previous')?.getAttribute('href')).toBe('/explore?q=x');
      expect(nav.querySelector('.explore-page__next button')?.hasAttribute('disabled')).toBe(true);
    });

    it('is not there for a single page', async () => {
      const { el, search, answer } = await setup();
      await answer(search(), page([repo()]));
      expect(el().querySelector('nav.explore-page__pagination')).toBeNull();
    });
  });

  describe('states', () => {
    it('says when the instance has no public repository yet', async () => {
      const { el, search, answer } = await setup();
      await answer(search(), page([]));

      expect(text(el().querySelector('gbt-empty-state'))).toContain("Aucun dépôt public pour l'instant");
      expect(text(el().querySelector('.explore-page__count'))).toBe('Aucun dépôt');
    });

    it('says when nothing matches the search, and clears it', async () => {
      const { el, search, answer, refresh, router } = await setup('/explore?q=zzz');
      await answer(search(), page([]));

      expect(text(el().querySelector('gbt-empty-state'))).toContain('Aucun dépôt ne correspond à cette recherche');
      Array.from(el().querySelectorAll('gbt-empty-state button')).find((b) => text(b) === 'Effacer la recherche')!.dispatchEvent(new Event('click'));
      await refresh();

      expect(router.url).toBe('/explore');
      await answer(search(), page([]));
    });

    it('shows skeleton cards while loading', async () => {
      const { el, search, answer } = await setup();

      expect(el().querySelector('.explore-page__loading[aria-busy="true"]')).not.toBeNull();
      expect(text(el().querySelector('.explore-page__loading [role="status"]'))).toBe('Chargement des dépôts…');
      await answer(search(), page([]));
      expect(el().querySelector('.explore-page__loading')).toBeNull();
    });

    it('asks to wait a minute when throttled (429), and retries on demand', async () => {
      const { el, search, answer, refresh } = await setup();
      search().flush({ message: 'slow down' }, { status: 429, statusText: 'Too Many Requests', headers: { 'Retry-After': '60' } });
      await refresh();

      const alert = el().querySelector('gbt-alert')!;
      expect(text(alert)).toContain('Trop de recherches en peu de temps');
      Array.from(alert.querySelectorAll('button')).find((b) => text(b) === 'Réessayer')!.click();
      await refresh();

      await answer(search(), page([repo()]));
      expect(el().querySelector('gbt-alert')).toBeNull();
      expect(el().querySelectorAll('.explore-page__results > li')).toHaveLength(1);
    });

    it('says the catalog could not load on a server error', async () => {
      const { el, search, refresh } = await setup();
      search().flush('boom', { status: 500, statusText: 'Server Error' });
      await refresh();

      expect(text(el().querySelector('gbt-alert'))).toContain("Le catalogue n'a pas pu être chargé.");
    });

    it('says the public pages are closed when the API answers 404', async () => {
      const { el, search, refresh } = await setup();
      search().flush('', { status: 404, statusText: 'Not Found' });
      await refresh();

      expect(text(el().querySelector('gbt-empty-state'))).toContain('Les pages publiques sont désactivées');
      expect(el().querySelector('form[role="search"]')).toBeNull();
    });

    it('says the public pages are closed without asking the catalog when the instance turned them off', async () => {
      const { el, http, refresh } = await setup('/explore', { publicPagesEnabled: false });
      await refresh();

      expect(text(el().querySelector('gbt-empty-state'))).toContain('Les pages publiques sont désactivées');
      const login = el().querySelector<HTMLAnchorElement>('gbt-empty-state a')!;
      expect(text(login)).toBe('Se connecter');
      expect(login.getAttribute('href')).toBe('/login');
      http.expectNone((r) => r.url === CATALOG);
    });

    it('still tries the catalog when the instance settings could not be read', async () => {
      const { search, answer } = await setup('/explore', { publicPagesEnabled: null });
      await answer(search(), page([]));
    });
  });
});
