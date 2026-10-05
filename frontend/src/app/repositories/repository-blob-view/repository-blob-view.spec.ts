import { Component } from '@angular/core';
import { ComponentFixture, TestBed } from '@angular/core/testing';
import { HttpRequest, provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { provideRouter, Router } from '@angular/router';
import { RepositoryBlobView } from './repository-blob-view';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { TreeEntry } from '../repositories.service';
import { repositoryFixture } from '../repository-fixtures';

@Component({ template: '' })
class DummyRoutedComponent {}

const API = '/api/repositories/by-id/repo-1';
const REPO = repositoryFixture({ visibility: 'public', createdAt: '2026-01-01' });

const dir = (name: string): TreeEntry => ({ name, isDir: true, lastCommit: null });
const file = (name: string): TreeEntry => ({ name, isDir: false, lastCommit: null });

describe('RepositoryBlobView', () => {
  function setup(blobPath: string[] = ['README.md'], ref = 'main') {
    // Wildcard route so router.navigate() resolves, as in branch-switcher.spec.ts.
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([{ path: '**', component: DummyRoutedComponent }])] });
    const fixture = TestBed.createComponent(RepositoryBlobView);
    fixture.componentRef.setInput('repositoryId', 'repo-1');
    fixture.componentRef.setInput('ref', ref);
    fixture.componentRef.setInput('blobPath', blobPath);
    return { fixture, http: TestBed.inject(HttpTestingController), router: TestBed.inject(Router) };
  }

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  // Flushes every open request. Call it after the detectChanges() that follows the getById flush: only
  // then has RepositoryHeader rendered its BranchSwitcher. Missing its requests fails verify() in afterEach.
  function flushRepo(http: HttpTestingController, fixture: ComponentFixture<RepositoryBlobView>) {
    http.expectOne(API).flush(REPO);
    fixture.detectChanges();
    sweep(http);
  }

  // getLanguages answers `{ languages: [...] }`, not a bare array like the other endpoints.
  function sweep(http: HttpTestingController) {
    http.match(() => true).forEach((req) => req.flush(req.request.url.includes('/languages/') ? { languages: [] } : []));
  }

  function flushBlob(http: HttpTestingController, path: string, body: object, ref = 'main') {
    http.expectOne(`${API}/blob/${ref}/${path}`).flush(body);
  }

  // The page only needs names and kinds from a tree listing, not each entry's last commit. Every tree
  // request must carry lastCommit=false, or the server walks up to 200 commits per entry for nothing.
  const treeRequest = (path: string, ref = 'main') => (req: HttpRequest<unknown>) =>
    req.url === `${API}/tree/${ref}${path ? `/${path}` : ''}` && req.params.get('lastCommit') === 'false';

  function flushTree(http: HttpTestingController, path: string, entries: TreeEntry[], ref = 'main') {
    http.expectOne(treeRequest(path, ref)).flush(entries);
  }

  const el = (fixture: ComponentFixture<RepositoryBlobView>) => fixture.nativeElement as HTMLElement;
  const copyButton = (root: HTMLElement) => root.querySelector<HTMLButtonElement>('gbt-copy-button button');
  const treeItems = (root: HTMLElement) => Array.from(root.querySelectorAll<HTMLElement>('[role="treeitem"]'));
  const treeItem = (root: HTMLElement, label: string) => treeItems(root).find((li) => li.textContent?.trim() === label) ?? null;

  describe('content', () => {
    it('renders a markdown file through fg-markdown-view', () => {
      const { fixture, http } = setup(['README.md']);
      fixture.detectChanges();
      flushBlob(http, 'README.md', { sha: 'a', size: 5, isBinary: false, content: '# Hi' });
      flushRepo(http, fixture);
      fixture.detectChanges();

      expect(el(fixture).querySelector('fg-markdown-view')).toBeTruthy();
      expect(el(fixture).querySelector('fg-code-view')).toBeNull();
    });

    it('switches a markdown file to its source, with line numbers, through the Code source toggle', () => {
      const { fixture, http } = setup(['README.md']);
      fixture.detectChanges();
      flushBlob(http, 'README.md', { sha: 'a', size: 5, isBinary: false, content: '# Hi\n\nTexte\n' });
      flushRepo(http, fixture);
      fixture.detectChanges();

      const source = Array.from(el(fixture).querySelectorAll<HTMLButtonElement>('[role="radio"]')).find((b) => b.textContent?.trim() === 'Code source');
      expect(source).toBeTruthy();
      source!.click();
      fixture.detectChanges();

      expect(el(fixture).querySelector('fg-markdown-view')).toBeNull();
      expect(el(fixture).querySelector('fg-code-view .code-view__line-numbers')?.textContent).toBe('1\n2\n3');
    });

    it('renders a non-markdown file through fg-code-view, with line numbers', () => {
      const { fixture, http } = setup(['main.rs']);
      fixture.detectChanges();
      flushBlob(http, 'main.rs', { sha: 'a', size: 5, isBinary: false, content: 'fn main() {\n}\n' });
      flushRepo(http, fixture);
      fixture.detectChanges();

      expect(el(fixture).querySelector('fg-code-view')).toBeTruthy();
      expect(el(fixture).querySelector('fg-code-view .code-view__line-numbers')?.textContent).toBe('1\n2');
    });

    it('shows a binary-file notice in the file card and no viewer when isBinary is true', () => {
      const { fixture, http } = setup(['image.bin']);
      fixture.detectChanges();
      flushBlob(http, 'image.bin', { sha: 'a', size: 5, isBinary: true, content: null });
      flushRepo(http, fixture);
      fixture.detectChanges();

      const state = el(fixture).querySelector('.repository-blob-view__file gbt-empty-state');
      expect(state?.textContent).toContain('binaire');
      expect(el(fixture).querySelector('fg-code-view')).toBeNull();
    });

    it('shows a too-large notice in the file card when content is null but the file is not binary', () => {
      const { fixture, http } = setup(['big.txt']);
      fixture.detectChanges();
      flushBlob(http, 'big.txt', { sha: 'a', size: 2000000, isBinary: false, content: null });
      flushRepo(http, fixture);
      fixture.detectChanges();

      const state = el(fixture).querySelector('.repository-blob-view__file gbt-empty-state');
      expect(state?.textContent).toContain('volumineux');
      expect(state?.textContent).toContain('1,9 Mo');
    });

    it('redirects to the equivalent tree route when the blob path is actually a directory', async () => {
      const { fixture, http, router } = setup(['src']);
      fixture.detectChanges();
      // Flush getById directly: the catch-all would empty-flush the tree request asserted below.
      http.expectOne(API).flush(REPO);
      http.expectOne(`${API}/blob/main/src`).flush('not found', { status: 404, statusText: 'Not Found' });
      http.expectOne(treeRequest('src')).flush([{ name: 'main.rs', isDir: false, lastCommit: null }]);
      await fixture.whenStable();
      // Change detection after the navigation mounts BranchSwitcher, so flush its listBranches/listTags requests too.
      sweep(http);

      expect(router.url).toBe('/repositories/alice/hello/-/tree/main/src');
    });

    it('shows the not-found card, and no file navigator, when the path is neither a file nor a folder', () => {
      const { fixture, http } = setup(['docs', 'absent.md']);
      fixture.detectChanges();
      http.expectOne(API).flush(REPO);
      http.expectOne(`${API}/blob/main/docs/absent.md`).flush('not found', { status: 404, statusText: 'Not Found' });
      http.expectOne(treeRequest('docs/absent.md')).flush('not found', { status: 404, statusText: 'Not Found' });
      fixture.detectChanges();
      sweep(http);
      fixture.detectChanges();

      expect(el(fixture).querySelector('gbt-empty-state')?.textContent).toContain("Cette branche, ce tag ou ce chemin n'existe pas");
      expect(el(fixture).querySelector('gbt-tree')).toBeNull();
    });

    it('shows a load-error toast (not a redirect attempt) when the blob request fails with a non-404 status', () => {
      const { fixture, http } = setup(['main.rs']);
      const showSpy = vi.spyOn(TestBed.inject(GbtToastService), 'show');
      fixture.detectChanges();
      http.expectOne(`${API}/blob/main/main.rs`).flush('boom', { status: 500, statusText: 'Internal Server Error' });
      flushRepo(http, fixture);
      fixture.detectChanges();

      expect(showSpy).toHaveBeenCalledWith('Impossible de charger ce dépôt. Réessayez plus tard.', 'error');
      http.expectNone((req) => req.url === `${API}/tree/main/main.rs`);
      const failed = el(fixture).querySelector('gbt-alert .gbt-alert');
      expect(failed?.textContent).toContain("Le fichier n'a pas pu être chargé");
      expect(failed?.textContent).toContain('Réessayez dans un instant.');
      // The toast announces it, so the message in the card stays silent.
      expect(failed?.getAttribute('data-variant')).toBe('error');
      expect(failed?.getAttribute('role')).toBeNull();
      expect(failed?.getAttribute('aria-live')).toBeNull();
    });
  });

  describe('file card header', () => {
    it('shows the path as a breadcrumb: the repository and each folder link to their tree, the file is current', () => {
      const { fixture, http } = setup(['src', 'lib', 'util.rs']);
      fixture.detectChanges();
      flushBlob(http, 'src/lib/util.rs', { sha: 'a', size: 1234, isBinary: false, content: 'pub fn f() {}\n' });
      flushRepo(http, fixture);
      fixture.detectChanges();

      const crumb = el(fixture).querySelector('.repository-blob-view__file-header gbt-breadcrumb') as HTMLElement;
      expect(crumb).toBeTruthy();
      const host = el(fixture).querySelector('.repository-blob-view__file gbt-card')!;
      const header = host.querySelector(':scope > .gbt-card__header')!;
      const card = host.querySelector(':scope > .gbt-card')!;
      expect(Array.from(host.children)).toEqual([header, card]);
      expect(card.getAttribute('data-variant')).toBe('outlined');
      expect(header.querySelector('.repository-blob-view__file-header')).toBeTruthy();
      const links = Array.from(crumb.querySelectorAll('a')).map((a) => [a.textContent?.trim(), a.getAttribute('href')]);
      expect(links).toEqual([
        ['hello', '/repositories/alice/hello/-/tree/main'],
        ['src', '/repositories/alice/hello/-/tree/main/src'],
        ['lib', '/repositories/alice/hello/-/tree/main/src/lib'],
      ]);
      expect(crumb.querySelector('[aria-current="page"]')?.textContent?.trim()).toBe('util.rs');
    });

    it('shows the size and the line count of a text file', () => {
      const { fixture, http } = setup(['main.rs']);
      fixture.detectChanges();
      flushBlob(http, 'main.rs', { sha: 'a', size: 1234, isBinary: false, content: 'a\nb\nc\n' });
      flushRepo(http, fixture);
      fixture.detectChanges();

      const meta = el(fixture).querySelector('.repository-blob-view__file-meta')?.textContent ?? '';
      expect(meta).toContain('1,2 Ko');
      expect(meta).toContain('3 lignes');
    });

    it('shows the size of a binary file', () => {
      const { fixture, http } = setup(['logo.png']);
      fixture.detectChanges();
      flushBlob(http, 'logo.png', { sha: 'a', size: 204800, isBinary: true, content: null });
      flushRepo(http, fixture);
      fixture.detectChanges();

      expect(el(fixture).querySelector('.repository-blob-view__file-meta')?.textContent).toContain('200 Ko');
    });

    describe('Copier', () => {
      let writeText: ReturnType<typeof vi.fn>;
      // jsdom has no clipboard. The stub goes on navigator and is restored exactly, since spec files share
      // one global scope.
      let savedClipboard: PropertyDescriptor | undefined;

      beforeEach(() => {
        savedClipboard = Object.getOwnPropertyDescriptor(navigator, 'clipboard');
        writeText = vi.fn().mockResolvedValue(undefined);
        Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true, writable: true });
      });

      afterEach(() => {
        vi.useRealTimers();
        window.getSelection()?.removeAllRanges();
        if (savedClipboard) {
          Object.defineProperty(navigator, 'clipboard', savedClipboard);
        } else {
          delete (navigator as { clipboard?: Clipboard }).clipboard;
        }
      });

      it('copies the file content and says so for 2 s in a polite status', async () => {
        vi.useFakeTimers();
        const { fixture, http } = setup(['main.rs']);
        fixture.detectChanges();
        flushBlob(http, 'main.rs', { sha: 'a', size: 13, isBinary: false, content: 'fn main() {}\n' });
        flushRepo(http, fixture);
        fixture.detectChanges();

        const host = el(fixture).querySelector('gbt-copy-button.repository-blob-view__copy') as HTMLElement;
        const status = host.querySelector('[role="status"]') as HTMLElement;
        expect(status.getAttribute('aria-live')).toBe('polite');
        expect(status.textContent?.trim()).toBe('');
        expect(host.getAttribute('data-status')).toBe('idle');
        expect(copyButton(el(fixture))?.textContent?.trim()).toBe('Copier');
        expect(status.getAttribute('data-feedback')).toBe('hidden');
        expect(host.querySelector('button svg rect')).not.toBeNull();

        copyButton(el(fixture))!.click();
        await vi.advanceTimersByTimeAsync(0);
        fixture.detectChanges();

        expect(writeText).toHaveBeenCalledWith('fn main() {}\n');
        expect(status.textContent?.trim()).toBe('Contenu copié');
        expect(host.getAttribute('data-status')).toBe('copied');
        expect(host.querySelector('button svg rect')).toBeNull();
        expect(copyButton(el(fixture))?.textContent?.trim()).toBe('Copier');
        expect(el(fixture).querySelectorAll('.repository-blob-view__file-actions [role="status"]')).toHaveLength(1);

        await vi.advanceTimersByTimeAsync(1999);
        fixture.detectChanges();
        expect(status.textContent?.trim()).toBe('Contenu copié');

        await vi.advanceTimersByTimeAsync(1);
        fixture.detectChanges();
        expect(status.textContent?.trim()).toBe('');
        expect(host.getAttribute('data-status')).toBe('idle');
        expect(copyButton(el(fixture))?.textContent?.trim()).toBe('Copier');
      });

      it('selects the code and asks to copy by hand when the clipboard is unavailable (plain HTTP)', async () => {
        vi.useFakeTimers();
        Object.defineProperty(navigator, 'clipboard', { value: undefined, configurable: true, writable: true });
        const { fixture, http } = setup(['main.rs']);
        fixture.detectChanges();
        flushBlob(http, 'main.rs', { sha: 'a', size: 13, isBinary: false, content: 'fn main() {}\n' });
        flushRepo(http, fixture);
        fixture.detectChanges();

        copyButton(el(fixture))!.click();
        await vi.advanceTimersByTimeAsync(0);
        fixture.detectChanges();

        const status = el(fixture).querySelector('gbt-copy-button [role="status"]') as HTMLElement;
        expect(status.textContent?.trim()).toBe('Copie impossible, contenu sélectionné');
        expect(window.getSelection()?.toString().replace(/\s+/g, ' ').trim()).toBe('fn main() {}');
        // A refusal needs the user to act, so it stays twice as long as a success.
        await vi.advanceTimersByTimeAsync(3999);
        fixture.detectChanges();
        expect(status.textContent?.trim()).toBe('Copie impossible, contenu sélectionné');
        await vi.advanceTimersByTimeAsync(1);
        fixture.detectChanges();
        expect(status.textContent?.trim()).toBe('');
      });

      it('is not offered for a binary file, a too-large file or a rendered markdown file', () => {
        const cases: [string, object][] = [
          ['logo.png', { sha: 'a', size: 5, isBinary: true, content: null }],
          ['dump.sql', { sha: 'a', size: 5_000_000, isBinary: false, content: null }],
          ['README.md', { sha: 'a', size: 4, isBinary: false, content: '# Hi' }],
        ];
        for (const [name, body] of cases) {
          TestBed.resetTestingModule();
          const { fixture, http } = setup([name]);
          fixture.detectChanges();
          flushBlob(http, name, body);
          flushRepo(http, fixture);
          fixture.detectChanges();

          expect(copyButton(el(fixture)), name).toBeNull();
          http.verify();
        }
      });
    });
  });

  describe('file navigator', () => {
    function openDeepFile(ref = 'main', failing: string | null = null) {
      const { fixture, http, router } = setup(['src', 'lib', 'util.rs'], ref);
      const toast = vi.spyOn(TestBed.inject(GbtToastService), 'show');
      fixture.detectChanges();
      flushBlob(http, 'src/lib/util.rs', { sha: 'a', size: 14, isBinary: false, content: 'pub fn f() {}\n' }, ref);
      const levels: [string, TreeEntry[]][] = [
        ['', [file('Cargo.toml'), dir('src'), dir('docs')]],
        ['src', [dir('lib'), file('main.rs')]],
        ['src/lib', [file('util.rs'), file('mod.rs')]],
      ];
      for (const [path, entries] of levels) {
        if (path === failing) {
          http.expectOne(treeRequest(path, ref)).flush('boom', { status: 500, statusText: 'Internal Server Error' });
        } else {
          flushTree(http, path, entries, ref);
        }
      }
      flushRepo(http, fixture);
      fixture.detectChanges();
      return { fixture, http, router, toast };
    }

    it('loads the root and every folder above the file, on the current ref', () => {
      const { fixture } = openDeepFile('develop');

      expect(treeItems(el(fixture)).map((li) => li.textContent?.trim())).toEqual(['docs', 'src', 'lib', 'mod.rs', 'util.rs', 'main.rs', 'Cargo.toml']);
    });

    it('asks every navigator listing for names only (lastCommit=false): root, folders above the file, in-place expansion', () => {
      const { fixture, http } = openDeepFile('develop');
      treeItem(el(fixture), 'docs')!.querySelector('button')!.click();
      fixture.detectChanges();

      const expansion = http.expectOne((req) => req.url === `${API}/tree/develop/docs`);
      expect(expansion.request.params.get('lastCommit')).toBe('false');
      expansion.flush([file('guide.md')]);
    });

    it('selects the current file and expands the folders above it', () => {
      const { fixture } = openDeepFile();

      expect(treeItem(el(fixture), 'util.rs')?.getAttribute('aria-selected')).toBe('true');
      expect(treeItem(el(fixture), 'src')?.getAttribute('aria-expanded')).toBe('true');
      expect(treeItem(el(fixture), 'lib')?.getAttribute('aria-expanded')).toBe('true');
      expect(treeItems(el(fixture)).filter((li) => li.getAttribute('aria-selected') === 'true')).toHaveLength(1);
    });

    it('shows a folder whose content is not loaded yet as collapsed', () => {
      const { fixture } = openDeepFile();

      expect(treeItem(el(fixture), 'docs')?.getAttribute('aria-expanded')).toBe('false');
    });

    it('tolerates a folder listing that fails: the path to the file stays, the other levels still show', () => {
      const { fixture, toast } = openDeepFile('main', 'src');

      expect(treeItems(el(fixture)).map((li) => li.textContent?.trim())).toEqual(['docs', 'src', 'lib', 'mod.rs', 'util.rs', 'Cargo.toml']);
      expect(treeItem(el(fixture), 'util.rs')?.getAttribute('aria-selected')).toBe('true');
      expect(toast).not.toHaveBeenCalled();
    });

    it('opens another file of the navigator on its blob page', async () => {
      const { fixture, http, router } = openDeepFile();

      treeItem(el(fixture), 'Cargo.toml')!.click();
      await fixture.whenStable();
      sweep(http);

      expect(router.url).toBe('/repositories/alice/hello/-/blob/main/Cargo.toml');
    });

    it('opens a folder of the navigator on its tree page', async () => {
      const { fixture, http, router } = openDeepFile();

      treeItem(el(fixture), 'docs')!.click();
      await fixture.whenStable();
      sweep(http);

      expect(router.url).toBe('/repositories/alice/hello/-/tree/main/docs');
    });

    it('expands a folder in place with its chevron, loading its content on the current ref', () => {
      const { fixture, http } = openDeepFile('develop');

      treeItem(el(fixture), 'docs')!.querySelector('button')!.click();
      fixture.detectChanges();
      flushTree(http, 'docs', [file('guide.md')], 'develop');
      fixture.detectChanges();

      expect(treeItem(el(fixture), 'docs')?.getAttribute('aria-expanded')).toBe('true');
      expect(treeItem(el(fixture), 'guide.md')).toBeTruthy();
    });

    it('folds into a "Fichiers" disclosure, closed at first, for narrow layouts', () => {
      const { fixture } = openDeepFile();

      const toggle = el(fixture).querySelector('.repository-blob-view__nav-toggle') as HTMLButtonElement;
      expect(toggle.textContent).toContain('Fichiers');
      expect(toggle.getAttribute('aria-expanded')).toBe('false');
      const panel = el(fixture).querySelector(`#${toggle.getAttribute('aria-controls')}`) as HTMLElement;
      expect(panel.querySelector('gbt-tree')).toBeTruthy();
      expect(panel.classList).not.toContain('repository-blob-view__nav-panel--open');

      toggle.click();
      fixture.detectChanges();

      expect(toggle.getAttribute('aria-expanded')).toBe('true');
      expect(panel.classList).toContain('repository-blob-view__nav-panel--open');
    });
  });
});
