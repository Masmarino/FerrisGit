import { LOCALE_ID } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController, TestRequest } from '@angular/common/http/testing';
import { provideRouter, Router } from '@angular/router';
import { RepositoryTreeView } from './repository-tree-view';
import { formatDateTime, GbtToastService } from '@masmarino/gabarit';
import { Repository } from '../repositories.service';

const ABSOLUTE_OPTIONS = { day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' } as const;
const absoluteDateTime = (iso: string) => formatDateTime(iso, 'fr', ABSOLUTE_OPTIONS);

const REPO: Repository = {
  id: 'repo-1',
  name: 'hello',
  description: '',
  owner: 'alice',
  role: 'owner',
  visibility: 'public',
  createdAt: '2026-01-01T12:00:00Z', // midday: the same calendar day in any test machine's time zone
  path: ['alice', 'hello'],
  starCount: 3,
  isStarred: false,
  sizeBytes: 2048,
};

const NOW = new Date('2026-03-01T12:00:00Z');

const commit = (sha: string, message: string, authorName: string, committedAt: string) => ({ sha, message, authorName, authorEmail: `${authorName.toLowerCase()}@example.com`, committedAt });

describe('RepositoryTreeView', () => {
  function setup(treePath: string[] = [], ref = 'main') {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([]), { provide: LOCALE_ID, useValue: 'fr' }] });
    const fixture = TestBed.createComponent(RepositoryTreeView);
    fixture.componentRef.setInput('repositoryId', 'repo-1');
    fixture.componentRef.setInput('ref', ref);
    fixture.componentRef.setInput('treePath', treePath);
    const http = TestBed.inject(HttpTestingController);
    return { fixture, http };
  }

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
    vi.useRealTimers();
  });

  // Catch-all for requests a test ignores: `getLanguages` resolves to `{ languages: [...] }`, every other swept endpoint to a bare array.
  function sweep(http: HttpTestingController) {
    http.match(() => true).forEach((req) => req.flush(req.request.url.includes('/languages/') ? { languages: [] } : []));
  }

  // Flushes every request still open. Run it after the `detectChanges()` that follows the `getById` flush,
  // because only then has `RepositoryHeader` rendered its `BranchSwitcher`, whose requests it must catch.
  function flushRepo(http: HttpTestingController, fixture: ComponentFixture<RepositoryTreeView>, repo: Repository = REPO) {
    http.expectOne('/api/repositories/by-id/repo-1').flush(repo);
    fixture.detectChanges();
    sweep(http);
  }

  function commitsRequest(http: HttpTestingController): TestRequest {
    return http.expectOne((req) => req.url === '/api/repositories/by-id/repo-1/commits');
  }

  function renderRoot(entries: unknown[], commits: unknown[] | 'error' = [], repo: Repository = REPO) {
    const ctx = setup();
    ctx.fixture.detectChanges();
    ctx.http.expectOne('/api/repositories/by-id/repo-1/tree/main').flush(entries);
    const commitsReq = commitsRequest(ctx.http);
    if (commits === 'error') {
      commitsReq.flush('boom', { status: 500, statusText: 'Internal Server Error' });
    } else {
      commitsReq.flush(commits);
    }
    flushRepo(ctx.http, ctx.fixture, repo);
    ctx.fixture.detectChanges();
    return ctx;
  }

  const text = (fixture: ComponentFixture<RepositoryTreeView>) => fixture.nativeElement.textContent as string;
  const factValue = (root: Element, term: string) =>
    Array.from(root.querySelectorAll('dt'))
      .find((dt) => dt.textContent?.trim() === term)
      ?.nextElementSibling?.textContent?.trim();

  const query = <T extends Element = HTMLElement>(fixture: ComponentFixture<RepositoryTreeView>, selector: string) => fixture.nativeElement.querySelector(selector) as T | null;

  it('renders directories before files, both alphabetical', () => {
    const { fixture, http } = setup();
    fixture.detectChanges();
    http.expectOne('/api/repositories/by-id/repo-1/tree/main').flush([
      { name: 'zeta.txt', isDir: false, lastCommit: null },
      { name: 'beta', isDir: true, lastCommit: null },
      { name: 'alpha', isDir: true, lastCommit: null },
    ]);
    flushRepo(http, fixture);
    fixture.detectChanges();

    const entries: Element[] = Array.from(fixture.nativeElement.querySelectorAll('.repository-tree-view__entry-name'));
    const names = entries.map((el) => el.textContent?.trim());
    expect(names).toEqual(['alpha', 'beta', 'zeta.txt']);
  });

  it('links each entry to its tree or blob view at the current ref', () => {
    const { fixture } = renderRoot([
      { name: 'src', isDir: true, lastCommit: null },
      { name: 'Cargo.toml', isDir: false, lastCommit: null },
    ]);

    const hrefs = Array.from(fixture.nativeElement.querySelectorAll('.repository-tree-view__entry-name')).map((a) => (a as Element).getAttribute('href'));
    expect(hrefs).toEqual(['/repositories/alice/hello/-/tree/main/src', '/repositories/alice/hello/-/blob/main/Cargo.toml']);
  });

  it('fetches and renders the README only when at the repository root, in a README.md card', () => {
    const { fixture, http } = setup();
    fixture.detectChanges();
    // Flush `getById` directly, without the catch-all that would empty-flush the `readme` request asserted below.
    http.expectOne('/api/repositories/by-id/repo-1').flush(REPO);
    http.expectOne('/api/repositories/by-id/repo-1/tree/main').flush([]);
    http.expectOne('/api/repositories/by-id/repo-1/readme/main').flush({ content: '# Hello' });
    fixture.detectChanges();
    sweep(http);
    fixture.detectChanges();

    const card = query(fixture, '.repository-tree-view__readme');
    expect(card).toBeTruthy();
    expect(card!.querySelector('.gbt-card__header')?.textContent).toContain('README.md');
    expect(card!.querySelector('fg-markdown-view')).toBeTruthy();
  });

  it('shows no README card when the repository has no README', () => {
    const { fixture, http } = setup();
    fixture.detectChanges();
    http.expectOne('/api/repositories/by-id/repo-1').flush(REPO);
    http.expectOne('/api/repositories/by-id/repo-1/tree/main').flush([]);
    http.expectOne('/api/repositories/by-id/repo-1/readme/main').flush({ content: null });
    fixture.detectChanges();
    sweep(http);
    fixture.detectChanges();

    expect(query(fixture, '.repository-tree-view__readme')).toBeNull();
    expect(query(fixture, 'fg-markdown-view')).toBeNull();
    expect(text(fixture)).not.toContain('README.md');
  });

  it('does not fetch the README when browsing a subdirectory', () => {
    const { fixture, http } = setup(['src']);
    fixture.detectChanges();
    http.expectOne('/api/repositories/by-id/repo-1/tree/main/src').flush([]);
    flushRepo(http, fixture);

    http.expectNone('/api/repositories/by-id/repo-1/readme/main');
  });

  it('asks for the commits of the current ref', () => {
    const { fixture, http } = setup([], 'develop');
    fixture.detectChanges();

    const req = commitsRequest(http);
    expect(req.request.params.get('ref')).toBe('develop');
    req.flush([]);
    http.expectOne('/api/repositories/by-id/repo-1/tree/develop').flush([]);
    flushRepo(http, fixture);
  });

  it('shows the newest commit of the ref in a banner: author, message, short sha and relative time', () => {
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(NOW);
    const { fixture } = renderRoot(
      [{ name: 'README.md', isDir: false, lastCommit: null }],
      [commit('a1b2c3d4e5f6a7b8', 'Refonte de la page du dépôt', 'Alice Martin', '2026-03-01T10:00:00Z'), commit('ffff000011112222', 'Premier commit', 'Bob', '2026-02-01T10:00:00Z')],
    );

    const banner = query(fixture, '.repository-tree-view__latest');
    expect(banner).toBeTruthy();
    expect(banner!.querySelector('gbt-user-chip')?.textContent).toContain('Alice Martin');
    expect(banner!.querySelector('.repository-tree-view__latest-message')?.textContent?.trim()).toBe('Refonte de la page du dépôt');
    expect(banner!.querySelector('.repository-tree-view__sha')?.textContent?.trim()).toBe('a1b2c3d');
    const time = banner!.querySelector('time')!;
    expect(time.textContent?.trim()).toBe('il y a 2 h');
    expect(time.getAttribute('datetime')).toBe('2026-03-01T10:00:00Z');
    expect(time.getAttribute('title')).toBe(absoluteDateTime('2026-03-01T10:00:00Z'));
    expect(text(fixture)).not.toContain('Premier commit');
  });

  it('shows only the first line of a multi-line commit message in the banner', () => {
    const { fixture } = renderRoot([], [commit('a1b2c3d4', 'Titre du commit\n\nCorps détaillé du message', 'Alice', '2026-03-01T10:00:00Z')]);

    expect(query(fixture, '.repository-tree-view__latest-message')?.textContent?.trim()).toBe('Titre du commit');
    expect(text(fixture)).not.toContain('Corps détaillé');
  });

  it('shows no banner when the ref has no commits', () => {
    const { fixture } = renderRoot([], []);

    expect(query(fixture, '.repository-tree-view__latest')).toBeNull();
  });

  it('shows no banner when the commits request fails', () => {
    const { fixture } = renderRoot([], 'error');

    expect(query(fixture, '.repository-tree-view__latest')).toBeNull();
  });

  it('no longer renders the old Commits card', () => {
    const { fixture } = renderRoot([], [commit('a1b2c3d4', 'Un commit', 'Alice', '2026-03-01T10:00:00Z')]);

    const headings = Array.from(fixture.nativeElement.querySelectorAll('h2, h3, .gbt-card__heading')).map((el) => (el as Element).textContent?.trim());
    expect(headings).not.toContain('Commits');
    expect(query(fixture, 'gbt-card:not(.repository-tree-view__files):not(.repository-tree-view__readme)')).toBeNull();
    expect(text(fixture)).not.toContain("Aucun commit pour l'instant");
  });

  it('in a subdirectory, shows the newest commit among the listed entries without asking for the ref history', () => {
    const { fixture, http } = setup(['src']);
    fixture.detectChanges();
    http.expectOne('/api/repositories/by-id/repo-1/tree/main/src').flush([
      { name: 'old.ts', isDir: false, lastCommit: commit('0ld0ld0ld', 'Ancien changement', 'Bob', '2026-01-10T10:00:00Z') },
      { name: 'new.ts', isDir: false, lastCommit: commit('9e99e99e9', 'Changement récent', 'Alice', '2026-02-20T10:00:00Z') },
      { name: 'none.ts', isDir: false, lastCommit: null },
    ]);
    http.expectNone((req) => req.url === '/api/repositories/by-id/repo-1/commits');
    flushRepo(http, fixture);
    fixture.detectChanges();

    expect(query(fixture, '.repository-tree-view__latest-message')?.textContent?.trim()).toBe('Changement récent');
    expect(query(fixture, '.repository-tree-view__sha')?.textContent?.trim()).toBe('9e99e99');
  });

  it('shows each entry’s last commit message and its relative date, with the exact date on hover', () => {
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(NOW);
    const { fixture } = renderRoot([
      { name: 'src', isDir: true, lastCommit: commit('a1', 'Refactor tree view', 'Alice', '2026-02-28T09:00:00Z') },
      { name: 'Cargo.toml', isDir: false, lastCommit: commit('b2', 'Bump dependencies', 'Bob', '2026-03-01T11:30:00Z') },
      { name: 'LICENSE', isDir: false, lastCommit: null },
    ]);

    const rows: HTMLTableRowElement[] = Array.from(fixture.nativeElement.querySelectorAll('.repository-tree-view__table tbody tr'));
    expect(rows.length).toBe(3);

    expect(rows[0].querySelector('.repository-tree-view__row-message')?.textContent?.trim()).toBe('Refactor tree view');
    const srcTime = rows[0].querySelector('.repository-tree-view__row-date time')!;
    expect(srcTime.textContent?.trim()).toBe('hier');
    expect(srcTime.getAttribute('datetime')).toBe('2026-02-28T09:00:00Z');
    expect(srcTime.getAttribute('title')).toBe(absoluteDateTime('2026-02-28T09:00:00Z'));

    expect(rows[1].querySelector('.repository-tree-view__row-message')?.textContent?.trim()).toBe('Bump dependencies');
    expect(rows[1].querySelector('.repository-tree-view__row-date time')?.textContent?.trim()).toBe('il y a 30 min');

    expect(rows[2].querySelector('time')).toBeNull();
  });

  it('shows the file listing inside a files card', () => {
    const { fixture } = renderRoot([]);

    const card = query(fixture, '.repository-tree-view__files');
    expect(card).toBeTruthy();
    expect(card!.querySelector('.repository-tree-view__table')).toBeTruthy();
    expect(card!.textContent).toContain('Fichiers');
    const header = card!.querySelector(':scope > .gbt-card__header')!;
    const inner = card!.querySelector(':scope > .gbt-card')!;
    expect(Array.from(card!.children)).toEqual([header, inner]);
    expect(inner.getAttribute('data-variant')).toBe('outlined');
    expect(inner.hasAttribute('data-flush')).toBe(true);
    expect(header.querySelector('.repository-tree-view__files-header')).toBeTruthy();
  });

  it('says so when the folder is empty', () => {
    const { fixture } = renderRoot([]);

    expect(query(fixture, '.repository-tree-view__files')?.textContent).toContain('Ce dossier est vide');
  });

  it('expands a directory in place to show its children, without navigating away', () => {
    const { fixture, http } = setup();
    fixture.detectChanges();
    http.expectOne('/api/repositories/by-id/repo-1/tree/main').flush([{ name: 'src', isDir: true, lastCommit: null }]);
    flushRepo(http, fixture);
    fixture.detectChanges();

    const toggle: HTMLButtonElement = fixture.nativeElement.querySelector('.repository-tree-view__toggle');
    expect(toggle.getAttribute('aria-expanded')).toBe('false');
    expect(toggle.getAttribute('aria-label')).toBe('Déplier src');
    toggle.click();
    fixture.detectChanges();

    http.expectOne('/api/repositories/by-id/repo-1/tree/main/src').flush([{ name: 'index.ts', isDir: false, lastCommit: null }]);
    fixture.detectChanges();

    const names: string[] = Array.from(fixture.nativeElement.querySelectorAll('.repository-tree-view__entry-name')).map((el) => (el as Element).textContent?.trim());
    expect(names).toEqual(['src', 'index.ts']);
    expect(toggle.getAttribute('aria-expanded')).toBe('true');
    expect(toggle.getAttribute('aria-label')).toBe('Réduire src');
  });

  it('shows a spinner (announced as "Chargement…") on the directory row while its children load, and removes it once they are in', () => {
    const { fixture, http } = setup();
    fixture.detectChanges();
    http.expectOne('/api/repositories/by-id/repo-1/tree/main').flush([{ name: 'src', isDir: true, lastCommit: null }]);
    flushRepo(http, fixture);
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('gbt-spinner')).toBeNull();

    (fixture.nativeElement.querySelector('.repository-tree-view__toggle') as HTMLButtonElement).click();
    fixture.detectChanges();

    const spinner: HTMLElement = fixture.nativeElement.querySelector('gbt-spinner');
    expect(spinner.getAttribute('role')).toBe('status');
    expect(spinner.textContent?.trim()).toBe('Chargement…');

    http.expectOne('/api/repositories/by-id/repo-1/tree/main/src').flush([{ name: 'index.ts', isDir: false, lastCommit: null }]);
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('gbt-spinner')).toBeNull();
  });

  it('indents the children of an expanded directory', () => {
    const { fixture, http } = setup();
    fixture.detectChanges();
    http.expectOne('/api/repositories/by-id/repo-1/tree/main').flush([{ name: 'src', isDir: true, lastCommit: null }]);
    flushRepo(http, fixture);
    fixture.detectChanges();

    (fixture.nativeElement.querySelector('.repository-tree-view__toggle') as HTMLButtonElement).click();
    fixture.detectChanges();
    http.expectOne('/api/repositories/by-id/repo-1/tree/main/src').flush([{ name: 'index.ts', isDir: false, lastCommit: null }]);
    fixture.detectChanges();

    const cells: HTMLElement[] = Array.from(fixture.nativeElement.querySelectorAll('.repository-tree-view__name-cell'));
    expect(cells[0].style.getPropertyValue('--depth')).toBe('0');
    expect(cells[1].style.getPropertyValue('--depth')).toBe('1');
  });

  it('collapses an expanded directory and re-expanding it reuses the cached children instead of refetching', () => {
    const { fixture, http } = setup();
    fixture.detectChanges();
    http.expectOne('/api/repositories/by-id/repo-1/tree/main').flush([{ name: 'src', isDir: true, lastCommit: null }]);
    flushRepo(http, fixture);
    fixture.detectChanges();

    const toggle: HTMLButtonElement = fixture.nativeElement.querySelector('.repository-tree-view__toggle');
    toggle.click();
    fixture.detectChanges();
    http.expectOne('/api/repositories/by-id/repo-1/tree/main/src').flush([{ name: 'index.ts', isDir: false, lastCommit: null }]);
    fixture.detectChanges();

    toggle.click();
    fixture.detectChanges();
    let names: string[] = Array.from(fixture.nativeElement.querySelectorAll('.repository-tree-view__entry-name')).map((el) => (el as Element).textContent?.trim());
    expect(names).toEqual(['src']);

    toggle.click();
    fixture.detectChanges();
    http.expectNone('/api/repositories/by-id/repo-1/tree/main/src');
    names = Array.from(fixture.nativeElement.querySelectorAll('.repository-tree-view__entry-name')).map((el) => (el as Element).textContent?.trim());
    expect(names).toEqual(['src', 'index.ts']);
  });

  it('links a subdirectory breadcrumb back to the repository root and each ancestor folder', () => {
    const { fixture, http } = setup(['src', 'app']);
    fixture.detectChanges();
    http.expectOne('/api/repositories/by-id/repo-1/tree/main/src/app').flush([]);
    flushRepo(http, fixture);
    fixture.detectChanges();

    const links: HTMLAnchorElement[] = Array.from(fixture.nativeElement.querySelectorAll('gbt-breadcrumb a'));
    expect(links.map((a) => a.textContent?.trim())).toEqual(['hello', 'src']);
    expect(links.map((a) => a.getAttribute('href'))).toEqual(['/repositories/alice/hello/-/tree/main', '/repositories/alice/hello/-/tree/main/src']);
    expect(query(fixture, 'gbt-breadcrumb .gbt-breadcrumb__current')?.textContent?.trim()).toBe('app');
  });

  it('shows the contributors/languages sidebar at the repository root', () => {
    const { fixture } = renderRoot([]);

    expect(query(fixture, '.repository-tree-view__sidebar fg-contributor-avatars')).toBeTruthy();
    expect(query(fixture, '.repository-tree-view__sidebar fg-language-bar')).toBeTruthy();
  });

  it('keeps showing the contributors/languages sidebar when browsing a subdirectory', () => {
    const { fixture, http } = setup(['src']);
    fixture.detectChanges();
    http.expectOne('/api/repositories/by-id/repo-1/tree/main/src').flush([]);
    flushRepo(http, fixture);
    fixture.detectChanges();

    expect(query(fixture, '.repository-tree-view__sidebar fg-contributor-avatars')).toBeTruthy();
    expect(query(fixture, '.repository-tree-view__sidebar fg-language-bar')).toBeTruthy();
  });

  it('titles the aside panels À propos, Cloner, Langages and Contributeurs', () => {
    const { fixture } = renderRoot([]);

    const headings = Array.from(fixture.nativeElement.querySelectorAll('.repository-tree-view__sidebar .gbt-panel__heading')).map((el) => (el as Element).textContent?.trim());
    expect(headings).toEqual(['À propos', 'Cloner', 'Langages', 'Contributeurs']);
  });

  it('shows the description, owner, visibility, creation date, size and star count in the À propos panel', () => {
    const { fixture } = renderRoot([], [], { ...REPO, description: 'A test repo' });

    const about = query(fixture, '.repository-tree-view__about')!;
    expect(about.textContent).toContain('A test repo');
    expect(about.textContent).toContain('alice');
    expect(about.textContent).toContain('Public');
    expect(about.textContent).toContain('1 janvier 2026');
    expect(about.textContent).toContain('2 Ko');
    expect(factValue(about, 'Favoris')).toBe('3');
    expect(factValue(about, 'Visibilité')).toBe('Public');
    expect(factValue(about, 'Propriétaire')).toBe('alice');
    expect(factValue(about, 'Taille')).toBe('2 Ko');
    const created = Array.from(about.querySelectorAll('dt')).find((dt) => dt.textContent?.trim() === 'Créé le')!.nextElementSibling!.querySelector('time')!;
    expect(created.textContent?.trim()).toBe('1 janvier 2026');
    expect(created.getAttribute('datetime')).toBe('2026-01-01T12:00:00Z');
    expect(created.getAttribute('title')).toBeTruthy();
    expect(about.querySelector('dl')!.getAttribute('data-value-align')).toBe('end');
  });

  it('falls back to "Aucune description" and leaves the size out when unknown', () => {
    const { fixture } = renderRoot([], [], { ...REPO, description: '', sizeBytes: undefined });

    const about = query(fixture, '.repository-tree-view__about')!;
    expect(about.textContent).toContain('Aucune description');
    expect(about.textContent).not.toContain('Taille');
  });

  it('mirrors a star toggle from the header in the À propos star count', () => {
    const { fixture, http } = renderRoot([]);

    (fixture.nativeElement.querySelector('.repository-header__star button') as HTMLButtonElement).click();
    fixture.detectChanges();
    expect(factValue(query(fixture, '.repository-tree-view__about')!, 'Favoris')).toBe('4');

    http.expectOne({ url: '/api/repositories/by-id/repo-1/star', method: 'POST' }).flush({ starCount: 9, isStarred: true });
    fixture.detectChanges();
    expect(factValue(query(fixture, '.repository-tree-view__about')!, 'Favoris')).toBe('9');
  });

  it('renders the HTTPS clone URL in a copy field in the Cloner panel', () => {
    const { fixture } = renderRoot([]);

    const clonePanel = fixture.nativeElement.querySelector('#cloner');
    expect(clonePanel).toBeTruthy();
    expect(clonePanel.querySelector('gbt-copy-field code')?.textContent).toBe(`${location.origin}/alice/hello.git`);
  });

  describe('opened on its #cloner fragment', () => {
    // jsdom has no `scrollIntoView`, so this spies on the prototype and attaches the fixture to the document (focus needs it). Both are undone even if the test fails.
    const originalScrollIntoView = Object.getOwnPropertyDescriptor(Element.prototype, 'scrollIntoView');
    let attached: HTMLElement | null = null;

    afterEach(() => {
      attached?.remove();
      attached = null;
      if (originalScrollIntoView) {
        Object.defineProperty(Element.prototype, 'scrollIntoView', originalScrollIntoView);
      } else {
        delete (Element.prototype as Partial<Element>).scrollIntoView;
      }
    });

    it('brings the Cloner panel into view', async () => {
      const scrollIntoView = vi.fn();
      Element.prototype.scrollIntoView = scrollIntoView;
      TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([]), { provide: LOCALE_ID, useValue: 'fr' }] });
      await TestBed.inject(Router).navigateByUrl('/#cloner');
      const fixture = TestBed.createComponent(RepositoryTreeView);
      fixture.componentRef.setInput('repositoryId', 'repo-1');
      fixture.componentRef.setInput('ref', 'main');
      fixture.componentRef.setInput('treePath', []);
      attached = fixture.nativeElement;
      document.body.appendChild(fixture.nativeElement);
      const http = TestBed.inject(HttpTestingController);
      fixture.detectChanges();
      http.expectOne('/api/repositories/by-id/repo-1/tree/main').flush([]);
      flushRepo(http, fixture);
      await fixture.whenStable();
      fixture.detectChanges();

      expect(scrollIntoView).toHaveBeenCalled();
      expect(document.activeElement?.closest('#cloner')).toBeTruthy();
    });
  });

  it('shows skeleton placeholders for the files card and the repository panels while loading', () => {
    const { fixture, http } = setup();
    fixture.detectChanges();

    expect(query(fixture, '.repository-tree-view__files--loading gbt-skeleton')).toBeTruthy();
    expect(query(fixture, '.repository-tree-view__sidebar gbt-skeleton')).toBeTruthy();
    expect(query(fixture, '.repository-tree-view__table')).toBeNull();

    http.expectOne('/api/repositories/by-id/repo-1/tree/main').flush([]);
    flushRepo(http, fixture);
    fixture.detectChanges();
    expect(query(fixture, '.repository-tree-view__files--loading')).toBeNull();
  });

  it('shows the empty-repository card with the clone box and both push commands when HEAD at the root does not resolve', () => {
    const { fixture, http } = setup([], 'HEAD');
    fixture.detectChanges();
    http.expectOne('/api/repositories/by-id/repo-1/tree/HEAD').flush('not found', { status: 404, statusText: 'Not Found' });
    flushRepo(http, fixture);
    fixture.detectChanges();

    const card = query(fixture, '.repository-tree-view__empty-repo')!;
    expect(card).toBeTruthy();
    expect(card.textContent).toContain('Ce dépôt est vide');
    expect(card.querySelector('h2')?.textContent?.trim()).toBe('Ce dépôt est vide');
    expect(card.getAttribute('aria-labelledby')).toBe(card.querySelector('h2')?.id);
    const commands = Array.from(card.querySelectorAll('pre')).map((pre) => pre.textContent ?? '');
    expect(commands.some((c) => c.includes(`git remote add origin ${location.origin}/alice/hello.git`))).toBe(true);
    expect(commands.some((c) => c.includes('git push -u origin main'))).toBe(true);
    expect(card.querySelector('gbt-copy-field code')?.textContent).toBe(`${location.origin}/alice/hello.git`);
  });

  it('keeps only the À propos panel beside an empty repository', () => {
    const { fixture, http } = setup([], 'HEAD');
    fixture.detectChanges();
    http.expectOne('/api/repositories/by-id/repo-1/tree/HEAD').flush('not found', { status: 404, statusText: 'Not Found' });
    flushRepo(http, fixture);
    fixture.detectChanges();

    const headings = Array.from(fixture.nativeElement.querySelectorAll('.repository-tree-view__sidebar .gbt-panel__heading')).map((el) => (el as Element).textContent?.trim());
    expect(headings).toEqual(['À propos']);
    expect(query(fixture, '.repository-tree-view__files')).toBeNull();
  });

  it('shows a not-found empty state with a link back to the repository root for an unknown ref elsewhere', () => {
    const { fixture, http } = setup([], 'does-not-exist');
    fixture.detectChanges();
    http.expectOne('/api/repositories/by-id/repo-1/tree/does-not-exist').flush('not found', { status: 404, statusText: 'Not Found' });
    flushRepo(http, fixture);
    fixture.detectChanges();

    const notFound = query(fixture, '.repository-tree-view__not-found')!;
    expect(notFound.querySelector('gbt-empty-state')).toBeTruthy();
    expect(notFound.textContent).toContain("n'existe pas");
    const link = notFound.querySelector('a') as HTMLAnchorElement;
    expect(link.getAttribute('href')).toBe('/repositories/alice/hello');
    expect(text(fixture)).not.toContain('Ce dépôt est vide');
    expect(query(fixture, '.repository-tree-view__files')).toBeNull();
  });

  it('shows a load-error toast (not the empty/not-found copy) when the tree request fails with a non-404 status', () => {
    const { fixture, http } = setup();
    const showSpy = vi.spyOn(TestBed.inject(GbtToastService), 'show');
    fixture.detectChanges();
    http.expectOne('/api/repositories/by-id/repo-1/tree/main').flush('boom', { status: 500, statusText: 'Internal Server Error' });
    flushRepo(http, fixture);
    fixture.detectChanges();

    expect(showSpy).toHaveBeenCalledWith('Impossible de charger ce dépôt. Réessayez plus tard.', 'error');
    // The toast announces it, so the message in the page stays silent (one live region, not two).
    const failed = (fixture.nativeElement as HTMLElement).querySelector('gbt-alert .gbt-alert');
    expect(failed?.textContent?.trim()).toBe("Les fichiers n'ont pas pu être chargés.");
    expect(failed?.getAttribute('data-variant')).toBe('error');
    expect(failed?.getAttribute('role')).toBeNull();
    expect(failed?.getAttribute('aria-live')).toBeNull();
    expect(fixture.nativeElement.textContent).not.toContain('Ce dépôt est vide');
    expect(fixture.nativeElement.textContent).not.toContain("n'existe pas");
  });
});
