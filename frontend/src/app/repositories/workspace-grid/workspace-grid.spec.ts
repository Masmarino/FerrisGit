import { Component, LOCALE_ID, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { provideRouter } from '@angular/router';
import { WORKSPACE_PAGE_SIZE, WorkspaceGrid, WorkspaceGroupItem } from './workspace-grid';
import { WorkspaceGridFilters } from './workspace-grid-filters';
import { Repository } from '../repositories.service';
import { repositoryFixture } from '../repository-fixtures';
import { formatDateTime, formatRelativeTime, GbtToastService } from '@masmarino/gabarit';

const ABSOLUTE_OPTIONS = { day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' } as const;
const RELATIVE_OPTIONS = { style: 'short', maxUnit: 'day', absoluteAfterDays: 30 } as const;
const absoluteDateTime = (iso: string) => formatDateTime(iso, 'fr', ABSOLUTE_OPTIONS);
const relativeTime = (iso: string) => formatRelativeTime(iso, 'fr', undefined, RELATIVE_OPTIONS);

const text = (el: Element | null | undefined) => (el?.textContent ?? '').replace(/\s+/g, ' ').trim();
const rows = (el: HTMLElement) => Array.from(el.querySelectorAll<HTMLElement>('.workspace-grid__items > li'));
const rowTitles = (el: HTMLElement) => rows(el).map((row) => text(row.querySelector('.gbt-list-row__title > a')));

function manyRepositories(count: number): Repository[] {
  return Array.from({ length: count }, (_, index) => {
    const name = `repo-${String(index + 1).padStart(2, '0')}`;
    return repositoryFixture({ id: `r${index + 1}`, createdAt: new Date(Date.UTC(2026, 0, index + 1)).toISOString(), path: ['alice', name] });
  });
}

describe('WorkspaceGrid', () => {
  const group: WorkspaceGroupItem = { id: 'g1', name: 'acme', link: ['/repositories', 'acme'], path: 'acme', role: 'maintainer' };
  const repo = repositoryFixture({ id: 'r1', description: 'A widget', visibility: 'public', path: ['alice', 'widget'] });

  function setup() {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([]), { provide: LOCALE_ID, useValue: 'fr' }] });
    const fixture = TestBed.createComponent(WorkspaceGrid);
    fixture.componentRef.setInput('searchLabel', 'Rechercher');
    fixture.componentRef.setInput('emptyHeading', 'Rien ici');
    const http = TestBed.inject(HttpTestingController);
    const el = fixture.nativeElement as HTMLElement;
    return { fixture, http, el };
  }

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  // jsdom has no clipboard, so the copy tests stub it on navigator and restore it exactly.
  let savedClipboard: PropertyDescriptor | undefined;

  beforeEach(() => {
    savedClipboard = Object.getOwnPropertyDescriptor(navigator, 'clipboard');
  });

  afterEach(() => {
    if (savedClipboard) {
      Object.defineProperty(navigator, 'clipboard', savedClipboard);
    } else {
      delete (navigator as { clipboard?: Clipboard }).clipboard;
    }
  });

  function stubClipboard() {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true, writable: true });
    return writeText;
  }

  it('shows the empty state when there is nothing to show and it is not loading', () => {
    const { fixture } = setup();
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).toContain('Rien ici');
  });

  it('shows skeleton placeholders instead of the empty state while loading', () => {
    const { fixture } = setup();
    fixture.componentRef.setInput('loading', true);
    fixture.detectChanges();
    expect(fixture.nativeElement.textContent).not.toContain('Rien ici');
    expect(fixture.nativeElement.querySelectorAll('gbt-skeleton-list .gbt-skeleton-list__row')).toHaveLength(5);
    // One polite status, outside any aria-busy region (those can hold back the announcement).
    expect(fixture.nativeElement.querySelector('gbt-skeleton-list [role="status"]')?.textContent?.trim()).toBe('Chargement…');
    expect(fixture.nativeElement.querySelector('[aria-busy="true"]')).toBeNull();
  });

  it('shows a failed state with a retry button instead of the empty state when the list could not be loaded', () => {
    const { fixture, el } = setup();
    fixture.componentRef.setInput('failed', true);
    fixture.detectChanges();
    let retries = 0;
    fixture.componentInstance.retry.subscribe(() => retries++);

    const failed = el.querySelector('gbt-list-card gbt-alert .gbt-alert')!;
    expect(text(failed)).toContain("n'a pas pu être chargée");
    expect(failed.getAttribute('data-variant')).toBe('error');
    // The parent's toast announces it, so the alert stays silent.
    expect(failed.getAttribute('role')).toBeNull();
    expect(failed.getAttribute('aria-live')).toBeNull();
    expect(el.textContent).not.toContain('Rien ici');
    Array.from(failed.querySelectorAll<HTMLButtonElement>('button'))
      .find((b) => text(b) === 'Réessayer')!
      .click();
    expect(retries).toBe(1);
  });

  it('renders a list row for each group and each repository, groups first', () => {
    const { fixture, el } = setup();
    fixture.componentRef.setInput('groups', [group]);
    fixture.componentRef.setInput('repositories', [repo]);
    fixture.detectChanges();

    expect(rowTitles(el)).toEqual(['acme', 'alice/widget']);
    const [groupRow, repoRow] = rows(el);
    expect(groupRow.textContent).toContain('Groupe');
    expect(repoRow.textContent).toContain('Dépôt');
  });

  it("keeps each row's title link a direct child of the row's title line, with the full path as its tooltip", () => {
    const { fixture, el } = setup();
    fixture.componentRef.setInput('repositories', [repo]);
    fixture.detectChanges();

    const link = el.querySelector<HTMLAnchorElement>('gbt-list-row .gbt-list-row__title > a')!;
    expect(link.getAttribute('href')).toBe('/repositories/alice/widget');
    expect(link.getAttribute('title')).toBe('alice/widget');
  });

  it('shows a visibility badge for a repository row but not for a group row, and says the visibility with the leading icon too', () => {
    const { fixture, el } = setup();
    fixture.componentRef.setInput('groups', [group]);
    fixture.componentRef.setInput('repositories', [repo, { ...repo, id: 'r2', name: 'secret', visibility: 'private' as const, path: ['alice', 'secret'], createdAt: '2025-12-01T00:00:00Z' }]);
    fixture.detectChanges();

    const [groupRow, publicRow, privateRow] = rows(el);
    // A group row has a role badge too, so pick the visibility one by its own class.
    expect(groupRow.querySelector('.workspace-grid__visibility')).toBeNull();
    expect(text(publicRow.querySelector('.workspace-grid__visibility'))).toBe('Public');
    expect(text(privateRow.querySelector('.workspace-grid__visibility'))).toBe('Privé');
    expect(text(groupRow.querySelector('gbt-badge'))).toBe('Mainteneur');
    expect(text(groupRow.querySelector('.gbt-list-row__leading'))).toBe('Groupe');
    expect(text(publicRow.querySelector('.gbt-list-row__leading'))).toBe('Dépôt public');
    expect(text(privateRow.querySelector('.gbt-list-row__leading'))).toBe('Dépôt privé');
  });

  it('shows the owner/role label for a repository row and the group role for a group row', () => {
    const { fixture, el } = setup();
    fixture.componentRef.setInput('groups', [group]);
    fixture.componentRef.setInput('repositories', [repo, { ...repo, id: 'r2', role: 'contributor' as const, path: ['bob', 'shared'], createdAt: '2025-12-01T00:00:00Z' }]);
    fixture.detectChanges();

    const [groupRow, ownedRow, sharedRow] = rows(el);
    expect(text(groupRow.querySelector('.workspace-grid__role'))).toBe('Mainteneur');
    expect(text(ownedRow.querySelector('.workspace-grid__role'))).toBe('Propriétaire');
    expect(text(sharedRow.querySelector('.workspace-grid__role'))).toBe('Contributeur');
  });

  describe('row meta', () => {
    it('shows the description, the relative creation date with the exact date on hover', () => {
      const { fixture, el } = setup();
      fixture.componentRef.setInput('repositories', [repo]);
      fixture.detectChanges();

      const [row] = rows(el);
      expect(text(row.querySelector('.workspace-grid__description'))).toBe('A widget');
      const time = row.querySelector('.gbt-list-row__meta time')!;
      expect(time.getAttribute('datetime')).toBe(repo.createdAt);
      expect(time.getAttribute('title')).toBe(absoluteDateTime(repo.createdAt));
      expect(text(row.querySelector('.gbt-list-row__meta'))).toContain(`créé ${relativeTime(repo.createdAt)}`);
    });

    it('leaves the size and the stars out when the listing does not know them', () => {
      const { fixture, el } = setup();
      fixture.componentRef.setInput('repositories', [repo]);
      fixture.detectChanges();

      const [row] = rows(el);
      expect(row.querySelector('.workspace-grid__size')).toBeNull();
      expect(row.querySelector('.workspace-grid__stars')).toBeNull();
      expect(row.querySelector('.workspace-grid__description')).not.toBeNull();
    });

    it('shows the size and the stars when known, including zero stars', () => {
      const { fixture, el } = setup();
      fixture.componentRef.setInput('repositories', [
        { ...repo, sizeBytes: 3 * 1024 * 1024, starCount: 12 },
        { ...repo, id: 'r2', path: ['alice', 'other'], starCount: 0, createdAt: '2025-12-01T00:00:00Z' },
      ]);
      fixture.detectChanges();

      const [known, noStars] = rows(el);
      expect(text(known.querySelector('.workspace-grid__size'))).toBe('3 Mo');
      expect(text(known.querySelector('.workspace-grid__stars'))).toBe('12 étoiles');
      expect(text(noStars.querySelector('.workspace-grid__stars'))).toBe('0 étoile');
      expect(noStars.querySelector('.workspace-grid__size')).toBeNull();
    });

    it('omits the description line when there is none', () => {
      const { fixture, el } = setup();
      fixture.componentRef.setInput('repositories', [{ ...repo, description: '' }]);
      fixture.detectChanges();
      expect(rows(el)[0].querySelector('.workspace-grid__description')).toBeNull();
    });
  });

  it('keeps the trailing column in rows without a menu, so every row ends on the same edge', () => {
    const { fixture, el } = setup();
    fixture.componentRef.setInput('groups', [{ ...group, role: 'reader' as const }]);
    fixture.componentRef.setInput('repositories', [repo]);
    fixture.detectChanges();

    const [readerGroup, repoRow] = rows(el);
    expect(readerGroup.querySelector('.gbt-menu__trigger')).toBeNull();
    expect(readerGroup.querySelector('.gbt-list-row__trailing .workspace-grid__menu-slot')).not.toBeNull();
    expect(repoRow.querySelector('.gbt-list-row__trailing .gbt-menu__trigger')).not.toBeNull();
  });

  it('names each row menu after its item', () => {
    const { fixture, el } = setup();
    fixture.componentRef.setInput('groups', [group]);
    fixture.componentRef.setInput('repositories', [repo]);
    fixture.detectChanges();

    const labels = Array.from(el.querySelectorAll('.gbt-menu__trigger'), (b) => b.getAttribute('aria-label'));
    expect(labels).toEqual(['Actions du groupe acme', 'Actions du dépôt alice/widget']);
  });

  describe('sorting and pagination', () => {
    it('lists the newest repositories first by default', () => {
      const { fixture, el } = setup();
      fixture.componentRef.setInput('repositories', manyRepositories(3));
      fixture.detectChanges();
      expect(rowTitles(el)).toEqual(['alice/repo-03', 'alice/repo-02', 'alice/repo-01']);
    });

    it(`shows ${WORKSPACE_PAGE_SIZE} rows per page with a pager, and the rest on the next page`, () => {
      const { fixture, el } = setup();
      fixture.componentRef.setInput('repositories', manyRepositories(30));
      fixture.detectChanges();

      expect(WORKSPACE_PAGE_SIZE).toBe(25);
      expect(rows(el)).toHaveLength(25);
      expect(rowTitles(el)[0]).toBe('alice/repo-30');
      el.querySelector<HTMLButtonElement>('gbt-pagination [aria-label="Page suivante"]')!.click();
      fixture.detectChanges();
      expect(rowTitles(el)).toEqual(['alice/repo-05', 'alice/repo-04', 'alice/repo-03', 'alice/repo-02', 'alice/repo-01']);
    });

    it('shows no pager when everything fits on one page', () => {
      const { fixture, el } = setup();
      fixture.componentRef.setInput('repositories', manyRepositories(25));
      fixture.detectChanges();
      expect(el.querySelector('gbt-pagination')).toBeNull();
    });

    it('clamps the page when a refresh shrinks the list under it (the pager gone, the last page shown)', () => {
      const { fixture, el } = setup();
      fixture.componentRef.setInput('repositories', manyRepositories(30));
      fixture.detectChanges();
      el.querySelector<HTMLButtonElement>('gbt-pagination [aria-label="Page suivante"]')!.click();
      fixture.detectChanges();

      fixture.componentRef.setInput('repositories', manyRepositories(20));
      fixture.detectChanges();

      expect(el.querySelector('gbt-pagination')).toBeNull();
      expect(rows(el)).toHaveLength(20);
    });

    it('stays on the current page when a refresh keeps it (same scope)', () => {
      const { fixture, el } = setup();
      fixture.componentRef.setInput('scope', 'all');
      fixture.componentRef.setInput('repositories', manyRepositories(30));
      fixture.detectChanges();
      el.querySelector<HTMLButtonElement>('gbt-pagination [aria-label="Page suivante"]')!.click();
      fixture.detectChanges();

      fixture.componentRef.setInput('repositories', manyRepositories(29));
      fixture.detectChanges();
      expect(el.querySelector('gbt-pagination [aria-current="page"]')?.getAttribute('aria-label')).toBe('Page 2');
      expect(rows(el)).toHaveLength(4);
    });

    it('goes back to the first page when the scope changes (another tab, another group)', () => {
      const { fixture, el } = setup();
      fixture.componentRef.setInput('scope', 'all');
      fixture.componentRef.setInput('repositories', manyRepositories(30));
      fixture.detectChanges();
      el.querySelector<HTMLButtonElement>('gbt-pagination [aria-label="Page suivante"]')!.click();
      fixture.detectChanges();

      fixture.componentRef.setInput('scope', 'mine');
      fixture.detectChanges();
      expect(el.querySelector('gbt-pagination [aria-current="page"]')?.getAttribute('aria-label')).toBe('Page 1');
    });
  });

  describe('with its aside filters', () => {
    @Component({
      standalone: true,
      imports: [WorkspaceGrid, WorkspaceGridFilters],
      template: `
        <fg-workspace-grid #grid [groups]="groups()" [repositories]="repositories()" searchLabel="Rechercher un dépôt ou un groupe" emptyHeading="Rien ici" />
        <fg-workspace-grid-filters [grid]="grid" />
      `,
    })
    class Host {
      groups = signal<WorkspaceGroupItem[]>([group]);
      repositories = signal<Repository[]>([repo]);
    }

    function hosted() {
      TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([]), { provide: LOCALE_ID, useValue: 'fr' }] });
      const fixture = TestBed.createComponent(Host);
      fixture.detectChanges();
      return { fixture, el: fixture.nativeElement as HTMLElement };
    }

    function type(fixture: { detectChanges(): void }, el: HTMLElement, value: string) {
      const input = el.querySelector<HTMLInputElement>('.workspace-grid-filters__search input')!;
      input.value = value;
      input.dispatchEvent(new Event('input'));
      fixture.detectChanges();
    }

    it("labels the search field with the grid's search label", () => {
      const { el } = hosted();
      expect(text(el.querySelector('.workspace-grid-filters__search label'))).toBe('Rechercher un dépôt ou un groupe');
    });

    it('filters by name across both groups and repositories', () => {
      const { fixture, el } = hosted();
      type(fixture, el, 'widget');

      const titles = rowTitles(el);
      expect(titles).toEqual(['alice/widget']);
      expect(titles.join()).not.toContain('acme');
    });

    it('matches the whole path, not only the last segment', () => {
      const { fixture, el } = hosted();
      type(fixture, el, 'alice/');
      expect(rowTitles(el)).toEqual(['alice/widget']);
    });

    it('shows a plain no-results message (not the illustrated empty state) when a search matches nothing', () => {
      const { fixture, el } = hosted();
      type(fixture, el, 'does-not-exist');

      expect(text(el.querySelector('gbt-list-card [list-card-message]'))).toBe('Aucun résultat pour cette recherche');
      expect(el.textContent).not.toContain('Rien ici');
    });

    it('sorts by name from the aside, in either direction', () => {
      const { fixture, el } = hosted();
      fixture.componentInstance.repositories.set(manyRepositories(3).reverse().map((r, i) => ({ ...r, path: ['alice', ['beta', 'alpha', 'gamma'][i]] })));
      fixture.detectChanges();

      el.querySelector<HTMLButtonElement>('.workspace-grid-filters__sort .gbt-select__trigger')!.click();
      fixture.detectChanges();
      Array.from(el.querySelectorAll<HTMLButtonElement>('[role="option"]'))
        .find((option) => text(option) === 'Nom')!
        .click();
      fixture.detectChanges();
      expect(rowTitles(el).slice(1)).toEqual(['alice/gamma', 'alice/beta', 'alice/alpha']);

      Array.from(el.querySelectorAll<HTMLButtonElement>('.workspace-grid-filters__direction [role="radio"]'))
        .find((b) => text(b) === 'Croissant')!
        .click();
      fixture.detectChanges();
      expect(rowTitles(el).slice(1)).toEqual(['alice/alpha', 'alice/beta', 'alice/gamma']);
    });
  });

  it('hides the delete action on a group the caller cannot manage', () => {
    const { fixture } = setup();
    fixture.componentRef.setInput('groups', [{ ...group, role: 'reader' as const }]);
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('.gbt-menu__trigger')).toBeNull();
  });

  it('offers no delete action on a repository the caller only reads', () => {
    const { fixture, el } = setup();
    fixture.componentRef.setInput('repositories', [{ ...repo, role: 'reader' as const }]);
    fixture.detectChanges();

    el.querySelector<HTMLButtonElement>('.gbt-menu__trigger')!.click();
    fixture.detectChanges();
    const items = Array.from(el.querySelectorAll('[role="menuitem"]'), (item) => text(item));
    expect(items).toEqual(['Copier le chemin', "Copier l'URL de clonage"]);
  });

  it('deletes a repository after confirmation and emits changed', () => {
    const { fixture, http } = setup();
    fixture.componentRef.setInput('repositories', [repo]);
    fixture.detectChanges();
    let changedCount = 0;
    fixture.componentInstance.changed.subscribe(() => changedCount++);

    fixture.nativeElement.querySelector('.gbt-menu__trigger').click();
    fixture.detectChanges();
    const items: HTMLButtonElement[] = Array.from(fixture.nativeElement.querySelectorAll('[role="menuitem"]'));
    items.find((el) => el.textContent?.includes('Supprimer'))!.click();
    fixture.detectChanges();

    // The typed confirmation has its own tests, so skip it here.
    fixture.componentInstance['deleteRepoConfirmed']();
    http.expectOne('/api/repositories/by-id/r1').flush(null);
    expect(changedCount).toBe(1);
  });

  it('asks to type the repository name in the danger confirmation', () => {
    const { fixture, el } = setup();
    fixture.componentRef.setInput('repositories', [repo]);
    fixture.detectChanges();

    el.querySelector<HTMLButtonElement>('.gbt-menu__trigger')!.click();
    fixture.detectChanges();
    Array.from(el.querySelectorAll<HTMLButtonElement>('[role="menuitem"]'))
      .find((item) => item.textContent?.includes('Supprimer'))!
      .click();
    fixture.detectChanges();

    const dialog = el.querySelector('gbt-confirm-danger-modal')!;
    expect(dialog).not.toBeNull();
    expect(text(dialog)).toContain('Tapez le nom du dépôt pour confirmer');
  });

  it('words the repository deletion warning with "tickets", not "issues"', () => {
    const { fixture, el } = setup();
    fixture.componentRef.setInput('repositories', [repo]);
    fixture.detectChanges();

    el.querySelector<HTMLButtonElement>('.gbt-menu__trigger')!.click();
    fixture.detectChanges();
    Array.from(el.querySelectorAll<HTMLButtonElement>('[role="menuitem"]'))
      .find((item) => item.textContent?.includes('Supprimer'))!
      .click();
    fixture.detectChanges();

    const dialog = text(el.querySelector('gbt-confirm-danger-modal'));
    expect(dialog).toContain('(tickets, demandes de fusion, releases, wiki)');
    expect(dialog).not.toContain('issues');
  });

  it('deletes a group after confirmation and emits changed', () => {
    const { fixture, http } = setup();
    fixture.componentRef.setInput('groups', [group]);
    fixture.detectChanges();
    let changedCount = 0;
    fixture.componentInstance.changed.subscribe(() => changedCount++);

    fixture.nativeElement.querySelector('.gbt-menu__trigger').click();
    fixture.detectChanges();
    const items: HTMLButtonElement[] = Array.from(fixture.nativeElement.querySelectorAll('[role="menuitem"]'));
    items.find((el) => el.textContent?.includes('Supprimer'))!.click();
    fixture.detectChanges();

    // The typed confirmation has its own tests, so skip it here.
    fixture.componentInstance['deleteGroupConfirmed']();
    http.expectOne('/api/groups/g1').flush(null);
    expect(changedCount).toBe(1);
  });

  it('shows a group-specific toast when deleting a non-empty group fails with 409', () => {
    const { fixture, http } = setup();
    fixture.componentRef.setInput('groups', [group]);
    fixture.detectChanges();

    fixture.nativeElement.querySelector('.gbt-menu__trigger').click();
    fixture.detectChanges();
    const items: HTMLButtonElement[] = Array.from(fixture.nativeElement.querySelectorAll('[role="menuitem"]'));
    items.find((el) => el.textContent?.includes('Supprimer'))!.click();
    fixture.detectChanges();

    fixture.componentInstance['deleteGroupConfirmed']();
    http.expectOne('/api/groups/g1').flush({ error: 'this group still has at least one repository — remove it first' }, { status: 409, statusText: 'Conflict' });

    // Only AppShell renders gbt-toaster, so check GbtToastService directly.
    const [toast] = TestBed.inject(GbtToastService).toasts();
    expect(toast.message).toContain('Videz-le avant de le supprimer');
    expect(toast.message).not.toContain('remove it first');
  });

  it('copies the repository path to the clipboard', async () => {
    const { fixture } = setup();
    fixture.componentRef.setInput('repositories', [repo]);
    fixture.detectChanges();
    const writeText = stubClipboard();

    fixture.nativeElement.querySelector('.gbt-menu__trigger').click();
    fixture.detectChanges();
    const items: HTMLButtonElement[] = Array.from(fixture.nativeElement.querySelectorAll('[role="menuitem"]'));
    items.find((el) => el.textContent?.includes('Copier le chemin'))!.click();
    await fixture.whenStable();

    expect(writeText).toHaveBeenCalledWith('alice/widget');
    const [toast] = TestBed.inject(GbtToastService).toasts();
    expect(toast.message).toBe('Chemin copié');
  });

  it('copies the group path to the clipboard', async () => {
    const { fixture } = setup();
    fixture.componentRef.setInput('groups', [group]);
    fixture.detectChanges();
    const writeText = stubClipboard();

    fixture.nativeElement.querySelector('.gbt-menu__trigger').click();
    fixture.detectChanges();
    const items: HTMLButtonElement[] = Array.from(fixture.nativeElement.querySelectorAll('[role="menuitem"]'));
    items.find((el) => el.textContent?.includes('Copier le chemin'))!.click();
    await fixture.whenStable();

    expect(writeText).toHaveBeenCalledWith('acme');
    const [toast] = TestBed.inject(GbtToastService).toasts();
    expect(toast.message).toBe('Chemin copié');
  });

  it('copies the clone URL to the clipboard', async () => {
    const { fixture } = setup();
    fixture.componentRef.setInput('repositories', [repo]);
    fixture.detectChanges();
    const writeText = stubClipboard();

    fixture.nativeElement.querySelector('.gbt-menu__trigger').click();
    fixture.detectChanges();
    const items: HTMLButtonElement[] = Array.from(fixture.nativeElement.querySelectorAll('[role="menuitem"]'));
    items.find((el) => el.textContent?.includes("URL de clonage"))!.click();
    await fixture.whenStable();

    expect(writeText).toHaveBeenCalledWith(`${location.origin}/alice/widget.git`);
    const [toast] = TestBed.inject(GbtToastService).toasts();
    expect(toast.message).toBe('URL de clonage copiée');
  });

  it('says the copy failed, with an error toast, when the clipboard is unavailable (plain HTTP)', async () => {
    const { fixture } = setup();
    fixture.componentRef.setInput('repositories', [repo]);
    fixture.detectChanges();
    Object.defineProperty(navigator, 'clipboard', { value: undefined, configurable: true, writable: true });

    fixture.nativeElement.querySelector('.gbt-menu__trigger').click();
    fixture.detectChanges();
    const items: HTMLButtonElement[] = Array.from(fixture.nativeElement.querySelectorAll('[role="menuitem"]'));
    items.find((el) => el.textContent?.includes('Copier le chemin'))!.click();
    await fixture.whenStable();

    const [toast] = TestBed.inject(GbtToastService).toasts();
    expect(toast.message).toBe('Copie impossible : le presse-papiers est indisponible.');
    expect(toast.variant).toBe('error');
  });

  it('says the copy failed, with an error toast, when the clipboard refuses the write', async () => {
    const { fixture } = setup();
    fixture.componentRef.setInput('groups', [group]);
    fixture.detectChanges();
    const writeText = stubClipboard();
    writeText.mockRejectedValue(new Error('denied'));

    fixture.nativeElement.querySelector('.gbt-menu__trigger').click();
    fixture.detectChanges();
    const items: HTMLButtonElement[] = Array.from(fixture.nativeElement.querySelectorAll('[role="menuitem"]'));
    items.find((el) => el.textContent?.includes('Copier le chemin'))!.click();
    await fixture.whenStable();

    expect(writeText).toHaveBeenCalledWith('acme');
    const [toast] = TestBed.inject(GbtToastService).toasts();
    expect(toast.message).toBe('Copie impossible : le presse-papiers est indisponible.');
    expect(toast.variant).toBe('error');
  });

  it("returns focus to the row's own kebab trigger when the repo delete confirmation is cancelled", async () => {
    const { fixture } = setup();
    fixture.componentRef.setInput('repositories', [repo]);
    fixture.detectChanges();

    const trigger: HTMLButtonElement = fixture.nativeElement.querySelector('.gbt-menu__trigger');
    trigger.click();
    fixture.detectChanges();
    const items: HTMLButtonElement[] = Array.from(fixture.nativeElement.querySelectorAll('[role="menuitem"]'));
    items.find((el) => el.textContent?.includes('Supprimer'))!.click();
    fixture.detectChanges();

    // cancelDelete() must run however the modal was closed (Cancel, Escape, backdrop): (closed) doesn't say which.
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    // cancelDelete() tears down the modal, whose ngOnDestroy restores focus synchronously, then restores
    // it again in a microtask so that one wins. Flush it before asserting.
    fixture.detectChanges();
    await Promise.resolve();

    expect(document.activeElement).toBe(trigger);
  });

  it("returns focus to the row's own kebab trigger when the group delete confirmation is cancelled", async () => {
    const { fixture } = setup();
    fixture.componentRef.setInput('groups', [group]);
    fixture.detectChanges();

    const trigger: HTMLButtonElement = fixture.nativeElement.querySelector('.gbt-menu__trigger');
    trigger.click();
    fixture.detectChanges();
    const items: HTMLButtonElement[] = Array.from(fixture.nativeElement.querySelectorAll('[role="menuitem"]'));
    items.find((el) => el.textContent?.includes('Supprimer'))!.click();
    fixture.detectChanges();

    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
    fixture.detectChanges();
    await Promise.resolve();

    expect(document.activeElement).toBe(trigger);
  });

  it("returns focus to the row's own kebab trigger after a failed repository delete", async () => {
    const { fixture, http } = setup();
    fixture.componentRef.setInput('repositories', [repo]);
    fixture.detectChanges();

    const trigger: HTMLButtonElement = fixture.nativeElement.querySelector('.gbt-menu__trigger');
    trigger.click();
    fixture.detectChanges();
    const items: HTMLButtonElement[] = Array.from(fixture.nativeElement.querySelectorAll('[role="menuitem"]'));
    items.find((el) => el.textContent?.includes('Supprimer'))!.click();
    fixture.detectChanges();

    fixture.componentInstance['deleteRepoConfirmed']();
    http.expectOne('/api/repositories/by-id/r1').flush({ error: 'boom' }, { status: 500, statusText: 'Internal Server Error' });
    fixture.detectChanges();
    await Promise.resolve();

    expect(document.activeElement).toBe(trigger);
  });
});
