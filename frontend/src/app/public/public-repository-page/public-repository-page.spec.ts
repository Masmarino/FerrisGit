import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting, TestRequest } from '@angular/common/http/testing';
import { provideRouter } from '@angular/router';
import { RouterTestingHarness } from '@angular/router/testing';
import { GbtToastService } from '@masmarino/gabarit';
import { PublicRepositoryPage } from './public-repository-page';
import { publicRepositoryMatcher } from '../public.guards';
import { providePublicRepositoryData } from '../public-providers';
import { PageTitleService } from '../../shell/page-title.service';

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();

const REPO = {
  id: 'repo-1',
  name: 'hello',
  description: 'Un dépôt public',
  owner: 'alice',
  visibility: 'public',
  createdAt: '2026-01-01T00:00:00Z',
  path: ['alice', 'hello'],
  starCount: 4,
};

/** A plausible answer for each read the reused pages make. */
function answerFor(request: TestRequest): unknown {
  const url = request.request.url;
  if (url.endsWith('/repositories/by-id/repo-1')) return REPO;
  if (url.includes('/readme/')) return { content: null };
  if (url.includes('/languages/')) return { languages: [] };
  if (url.includes('/blob/')) return { sha: 'abc', size: 5, isBinary: false, content: 'hello' };
  if (url.endsWith('/releases/v1.0')) return { id: 'r1', tagName: 'v1.0', title: 'Version 1', notes: '', draft: false, prerelease: false, authorId: null, author: null, createdAt: '2026-01-01T00:00:00Z', publishedAt: '2026-01-01T00:00:00Z', targetCommitSha: null, assets: [] };
  return [];
}

async function setup(url: string) {
  TestBed.configureTestingModule({
    providers: [
      provideHttpClient(),
      provideHttpClientTesting(),
      provideRouter([{ matcher: publicRepositoryMatcher, component: PublicRepositoryPage, providers: providePublicRepositoryData() }]),
      { provide: GbtToastService, useValue: { show: vi.fn() } },
    ],
  });
  const harness = await RouterTestingHarness.create(url);
  const http = TestBed.inject(HttpTestingController);
  const el = () => harness.routeNativeElement as HTMLElement;
  const requested: string[] = [];
  const refresh = async () => {
    harness.fixture.detectChanges();
    await harness.fixture.whenStable();
    harness.fixture.detectChanges();
  };
  /** Answers every pending read (and the ones they trigger), and records their URLs. */
  const flushReads = async () => {
    for (let round = 0; round < 5; round++) {
      const pending = http.match((r) => !r.url.includes('/resolve/'));
      if (pending.length === 0) break;
      for (const request of pending) {
        requested.push(request.request.url);
        request.flush(answerFor(request) as never);
      }
      await refresh();
    }
  };
  const resolve = async (body: unknown = { type: 'personalRepository', repositoryId: 'repo-1' }, path = 'alice/hello') => {
    http.expectOne(`/api/public/resolve/${path}`).flush(body as never);
    await refresh();
    await flushReads();
  };
  const tabs = () => Array.from(el().querySelectorAll<HTMLAnchorElement>('gbt-nav-tabs a'));
  return { harness, http, el, refresh, resolve, flushReads, requested, tabs };
}

describe('PublicRepositoryPage', () => {
  afterEach(() => TestBed.inject(HttpTestingController).verify());

  it('resolves the path through the public API, without the "repositories" prefix', async () => {
    const { resolve } = await setup('/repositories/alice/hello');
    await resolve();
  });

  it('shows the overview with the reused tree view, reading only the public API', async () => {
    const { el, resolve, requested } = await setup('/repositories/alice/hello');
    await resolve();

    const tree = el().querySelector('fg-repository-tree-view');
    expect(tree).not.toBeNull();
    expect(requested.length).toBeGreaterThan(0);
    expect(requested.every((url) => url.startsWith('/api/public/'))).toBe(true);
    expect(requested).toEqual(expect.arrayContaining(['/api/public/repositories/by-id/repo-1/tree/HEAD', '/api/public/repositories/repo-1/branches', '/api/public/repositories/repo-1/tags']));
  });

  it('shows no write action: no star button', async () => {
    const { el, resolve } = await setup('/repositories/alice/hello');
    await resolve();

    expect(el().querySelector('.repository-header__star')).toBeNull();
    expect(el().querySelector('.repository-header__clone')).not.toBeNull();
  });

  it('heads the page with the repository path and a tab bar, Aperçu active', async () => {
    const { el, resolve, tabs } = await setup('/repositories/alice/hello');
    await resolve();

    expect(text(el().querySelector('.public-repository-page__breadcrumb'))).toContain('alice');
    expect(text(el().querySelector('.public-repository-page__breadcrumb'))).toContain('hello');
    expect(tabs().map((a) => [text(a), a.getAttribute('href')])).toEqual([
      ['Aperçu', '/repositories/alice/hello'],
      ['Commits', '/repositories/alice/hello/-/commits'],
      ['Releases', '/repositories/alice/hello/-/releases'],
    ]);
    expect(tabs().map((a) => a.getAttribute('aria-current'))).toEqual(['page', null, null]);
  });

  it('opens a folder and a file with the reused tree and blob views', async () => {
    const folder = await setup('/repositories/alice/hello/-/tree/main/src');
    await folder.resolve();
    expect(folder.el().querySelector('fg-repository-tree-view')).not.toBeNull();
    expect(folder.requested).toContain('/api/public/repositories/by-id/repo-1/tree/main/src');
    expect(folder.tabs()[0].getAttribute('aria-current')).toBe('page');
  });

  it('opens a file with the reused blob view', async () => {
    const { el, resolve, requested } = await setup('/repositories/alice/hello/-/blob/main/README.md');
    await resolve();

    expect(el().querySelector('fg-repository-blob-view')).not.toBeNull();
    expect(requested).toContain('/api/public/repositories/by-id/repo-1/blob/main/README.md');
  });

  it('lists the commits under the Commits tab', async () => {
    const { el, resolve, tabs, requested } = await setup('/repositories/alice/hello/-/commits');
    await resolve();

    expect(el().querySelector('fg-repository-commit-list')).not.toBeNull();
    expect(requested).toContain('/api/public/repositories/by-id/repo-1/commits');
    expect(tabs().map((a) => a.getAttribute('aria-current'))).toEqual([null, 'page', null]);
    expect(text(el().querySelector('.public-repository-page__breadcrumb'))).toContain('Commits');
  });

  it('lists the releases without the "Nouvelle release" action', async () => {
    const { el, resolve, tabs, requested } = await setup('/repositories/alice/hello/-/releases');
    await resolve();

    expect(el().querySelector('fg-release-list')).not.toBeNull();
    expect(requested).toContain('/api/public/repositories/repo-1/releases');
    expect(text(el())).not.toContain('Nouvelle release');
    expect(text(el())).not.toContain('Créer une release');
    expect(tabs().map((a) => a.getAttribute('aria-current'))).toEqual([null, null, 'page']);
  });

  it('shows a release without its edit actions', async () => {
    const { el, resolve, requested } = await setup('/repositories/alice/hello/-/releases/v1.0');
    await resolve();

    expect(el().querySelector('fg-release-detail')).not.toBeNull();
    expect(requested).toContain('/api/public/repositories/repo-1/releases/v1.0');
    expect(el().querySelector('.release-detail__edit-button')).toBeNull();
    expect(text(el())).not.toContain('Supprimer');
  });

  it.each(['settings', 'pipelines', 'merge-requests', 'issues', 'wiki'])('answers "not found" for the signed-in-only sub-page %s', async (subPage) => {
    const { el, resolve } = await setup(`/repositories/alice/hello/-/${subPage}`);
    await resolve();

    expect(el().querySelector('fg-public-not-found')).not.toBeNull();
    expect(el().querySelector('gbt-nav-tabs')).toBeNull();
  });

  it('answers "not found" for a group, which has no public page', async () => {
    const { el, resolve } = await setup('/repositories/acme');
    await resolve({ type: 'group', groupId: 'g1', chain: [{ id: 'g1', name: 'acme' }], role: null }, 'acme');

    expect(el().querySelector('fg-public-not-found')).not.toBeNull();
  });

  it('answers "not found" for an unknown or private repository (404)', async () => {
    const { el, http, refresh } = await setup('/repositories/acme/secret');
    http.expectOne('/api/public/resolve/acme/secret').flush('', { status: 404, statusText: 'Not Found' });
    await refresh();

    expect(el().querySelector('fg-public-not-found')).not.toBeNull();
    expect(el().querySelector('gbt-nav-tabs')).toBeNull();
  });

  it('asks to wait when throttled (429), and retries on demand', async () => {
    const { el, http, refresh, resolve } = await setup('/repositories/alice/hello');
    http.expectOne('/api/public/resolve/alice/hello').flush('', { status: 429, statusText: 'Too Many Requests' });
    await refresh();

    const alert = el().querySelector('gbt-alert')!;
    expect(text(alert)).toContain('Trop de requêtes en peu de temps');
    Array.from(alert.querySelectorAll('button')).find((b) => text(b) === 'Réessayer')!.click();
    await refresh();

    await resolve();
    expect(el().querySelector('fg-repository-tree-view')).not.toBeNull();
  });

  it('says the repository could not load on a server error', async () => {
    const { el, http, refresh } = await setup('/repositories/alice/hello');
    http.expectOne('/api/public/resolve/alice/hello').flush('', { status: 500, statusText: 'Server Error' });
    await refresh();

    expect(text(el().querySelector('gbt-alert'))).toContain("Ce dépôt n'a pas pu être chargé.");
  });

  it('resolves again when the visitor moves to another sub-page', async () => {
    const { el, resolve, harness, refresh } = await setup('/repositories/alice/hello');
    await resolve();

    await harness.navigateByUrl('/repositories/alice/hello/-/commits');
    await refresh();
    await resolve();

    expect(el().querySelector('fg-repository-commit-list')).not.toBeNull();
  });

  it('leaves no page title of a previous sub-page behind', async () => {
    const { resolve, harness, refresh } = await setup('/repositories/alice/hello/-/commits');
    await resolve();
    expect(TestBed.inject(PageTitleService).title()).toBe('Commits');

    await harness.navigateByUrl('/repositories/alice/hello');
    await refresh();
    await resolve();

    expect(TestBed.inject(PageTitleService).title()).toBe('');
  });
});
