import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { Router, provideRouter } from '@angular/router';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { RepositoryRole } from '../../repositories/repositories.service';
import { SearchResponse } from '../../search/search.service';
import { SettingsService } from '../../settings/settings.service';
import { MeService } from '../me.service';
import { QuickSearch } from './quick-search';

@Component({ selector: 'fg-test-empty', template: '' })
class EmptyTestComponent {}

const NOTHING: SearchResponse = { repositories: [], issues: [], mergeRequests: [], users: [] };

describe('QuickSearch', () => {
  function setup(options: { admin?: boolean; engine?: 'docker-runners' | 'kubernetes'; userId?: string } = {}) {
    TestBed.configureTestingModule({
      providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([{ path: '**', component: EmptyTestComponent }])],
    });
    const me = TestBed.inject(MeService);
    me.id.set(options.userId ?? 'u-1');
    me.isAdmin.set(options.admin ?? false);
    TestBed.inject(SettingsService).publicSettings.set({ executionEngine: options.engine ?? 'kubernetes' });
    const fixture = TestBed.createComponent(QuickSearch);
    fixture.detectChanges();
    return { fixture, http: TestBed.inject(HttpTestingController), router: TestBed.inject(Router) };
  }

  type Fixture = ReturnType<typeof setup>['fixture'];

  function openPalette(fixture: Fixture): void {
    fixture.componentInstance.show();
    fixture.detectChanges();
  }

  function field(fixture: Fixture): HTMLInputElement {
    return fixture.nativeElement.querySelector('[role="dialog"] input[role="combobox"]');
  }

  function type(fixture: Fixture, text: string): void {
    field(fixture).value = text;
    field(fixture).dispatchEvent(new Event('input'));
    fixture.detectChanges();
  }

  function press(fixture: Fixture, key: string): void {
    field(fixture).dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true }));
    fixture.detectChanges();
  }

  /** Each visible group by its label, with its options' labels. */
  function groups(fixture: Fixture): Record<string, string[]> {
    const result: Record<string, string[]> = {};
    for (const group of fixture.nativeElement.querySelectorAll('[role="group"]') as NodeListOf<HTMLElement>) {
      const label = group.querySelector('.gbt-cp__group-label')!.textContent!.trim();
      result[label] = Array.from(group.querySelectorAll('.gbt-cp__item-label'), (item) => item.textContent!.trim());
    }
    return result;
  }

  function enterRepository(path: string[], role: RepositoryRole | null): void {
    TestBed.inject(RepositoryContextService).current.set({ repositoryId: `id-${path.join('-')}`, path, role, ancestors: [], groupId: null });
  }

  afterEach(() => {
    localStorage.clear();
    vi.useRealTimers();
  });

  it('opens on Ctrl K, closes on it again, and opens on "/" outside a field', () => {
    const { fixture } = setup();
    const dialog = (): HTMLElement | null => fixture.nativeElement.querySelector('[role="dialog"]');

    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, bubbles: true, cancelable: true }));
    fixture.detectChanges();
    expect(dialog()?.getAttribute('aria-label')).toBe('Recherche rapide');

    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, bubbles: true, cancelable: true }));
    fixture.detectChanges();
    expect(dialog()).toBeNull();

    document.dispatchEvent(new KeyboardEvent('keydown', { key: '/', bubbles: true, cancelable: true }));
    fixture.detectChanges();
    expect(dialog()).not.toBeNull();
  });

  it('offers the menu and the actions before anything is typed', () => {
    const { fixture } = setup();
    openPalette(fixture);

    expect(groups(fixture)).toEqual({
      'Aller à': ['Accueil', 'Dépôts', 'Groupes', 'Explorer', 'Mon compte', 'Documentation'],
      Actions: ['Nouveau dépôt', 'Nouveau groupe', 'Déconnexion'],
    });
  });

  it('offers a standard user no administration entry, whatever is typed', () => {
    vi.useFakeTimers();
    const { fixture, http } = setup({ admin: false });
    openPalette(fixture);
    const administration = ['Tableau de bord', 'Utilisateurs', 'Santé', "Réglages de l'instance"];

    for (const query of ['admin', 'tableau', 'utilisateur', 'santé', 'réglages', 'smtp', 'inviter', 'widgetfan']) {
      type(fixture, query);
      vi.advanceTimersByTime(200);
      // The server finds a user too: only an administrator has a page to open for them.
      http.match((req) => req.url === '/api/search').forEach((req) => req.flush({ ...NOTHING, users: [{ id: 'u9', username: 'widgetfan' }] }));
      fixture.detectChanges();
      const shown = groups(fixture);
      expect(Object.values(shown).flat().filter((label) => administration.includes(label))).toEqual([]);
      expect(Object.keys(shown)).not.toContain('Utilisateurs');
    }
  });

  it('adds the runners with Docker and the administration pages for an administrator', () => {
    const { fixture } = setup({ admin: true, engine: 'docker-runners' });
    openPalette(fixture);

    expect(groups(fixture)['Aller à']).toEqual([
      'Accueil',
      'Dépôts',
      'Groupes',
      'Explorer',
      'Runners',
      'Mon compte',
      'Documentation',
      'Tableau de bord',
      'Utilisateurs',
      'Santé',
      "Réglages de l'instance",
    ]);
  });

  it("lists the current repository's pages, and its creation actions only to those who can write", () => {
    const { fixture } = setup();
    enterRepository(['acme', 'widget'], 'reader');
    openPalette(fixture);

    expect(groups(fixture)['Dans widget']).toEqual(['Aperçu', 'Pipelines', 'Demandes de fusion', 'Tickets', 'Tableau des tickets', 'Releases', 'Wiki']);

    enterRepository(['acme', 'widget'], 'maintainer');
    fixture.detectChanges();
    expect(groups(fixture)['Dans widget']).toEqual([
      'Aperçu',
      'Pipelines',
      'Demandes de fusion',
      'Tickets',
      'Tableau des tickets',
      'Releases',
      'Wiki',
      'Réglages du dépôt',
      'Nouveau ticket',
      'Nouvelle demande de fusion',
      'Nouvelle page de wiki',
      'Éditer la pipeline',
    ]);
  });

  it('filters everything as one types, accents and descriptions included, best matches first', () => {
    const { fixture, http } = setup();
    openPalette(fixture);

    type(fixture, 'depot');

    const shown = groups(fixture);
    expect(shown['Aller à']).toEqual(['Dépôts', 'Explorer']);
    expect(shown['Actions']).toEqual(['Nouveau dépôt']);
    expect(shown['Recherche']).toEqual(['Rechercher « depot » partout']);
    http.match(() => true);
  });

  it('remembers the repositories opened, per account, newest first, without the one shown', () => {
    const { fixture } = setup({ userId: 'u-1' });
    enterRepository(['acme', 'widget'], 'reader');
    fixture.detectChanges();
    enterRepository(['acme', 'gadget'], 'reader');
    fixture.detectChanges();
    enterRepository(['acme', 'sprocket'], 'reader');
    fixture.detectChanges();

    openPalette(fixture);
    const recents = Array.from(
      fixture.nativeElement.querySelectorAll('[role="group"]:first-child .gbt-cp__item-description') as NodeListOf<HTMLElement>,
      (item) => item.textContent!.trim(),
    );
    expect(groups(fixture)['Récents']).toEqual(['gadget', 'widget']);
    expect(recents).toEqual(['acme/gadget', 'acme/widget']);

    TestBed.inject(MeService).id.set('u-2');
    fixture.detectChanges();
    // u-2 has only just been here: the repository on screen is theirs now, and not offered as a recent one.
    expect(groups(fixture)['Récents']).toBeUndefined();
  });

  it('still works when the browser refuses storage', () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('denied');
    });
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('denied');
    });
    const { fixture } = setup();
    enterRepository(['acme', 'widget'], 'reader');
    fixture.detectChanges();
    enterRepository(['acme', 'gadget'], 'reader');
    fixture.detectChanges();

    openPalette(fixture);

    expect(groups(fixture)['Récents']).toEqual(['widget']);
    vi.restoreAllMocks();
  });

  it('asks the server once the typing pauses and lists what it finds after the pages', () => {
    vi.useFakeTimers();
    const { fixture, http } = setup();
    openPalette(fixture);

    type(fixture, 'w');
    type(fixture, 'wid');
    vi.advanceTimersByTime(199);
    http.expectNone((req) => req.url === '/api/search');
    vi.advanceTimersByTime(1);
    http.expectOne((req) => req.url === '/api/search' && req.params.get('q') === 'wid').flush({
      repositories: [{ id: 'r1', name: 'widget', description: '', path: ['acme', 'widget'], visibility: 'private' }],
      issues: [{ id: 'i1', number: 7, title: 'Widget breaks', status: 'todo', kind: 'bug', createdAt: '', repository: { id: 'r1', name: 'widget', path: ['acme', 'widget'] } }],
      mergeRequests: [{ id: 'm1', title: 'Fix widget', status: 'open', sourceBranch: 'fix', targetBranch: 'main', createdAt: '', repository: { id: 'r1', name: 'widget', path: ['acme', 'widget'] } }],
      users: [{ id: 'u9', username: 'widgetfan' }],
    });
    fixture.detectChanges();

    const shown = groups(fixture);
    // A user has no page to open but for administrators.
    expect(Object.keys(shown)).toEqual(['Dépôts', 'Tickets', 'Demandes de fusion', 'Recherche']);
    expect(shown['Dépôts']).toEqual(['widget']);
    expect(shown['Tickets']).toEqual(['#7 Widget breaks']);
    expect(shown['Demandes de fusion']).toEqual(['Fix widget']);
    http.verify({ ignoreCancelled: true });
  });

  it('shows the users the server finds to an administrator, leading to their page', () => {
    vi.useFakeTimers();
    const { fixture, http, router } = setup({ admin: true });
    const navigate = vi.spyOn(router, 'navigate').mockResolvedValue(true);
    openPalette(fixture);

    type(fixture, 'widgetfan');
    vi.advanceTimersByTime(200);
    http.expectOne((req) => req.url === '/api/search').flush({ ...NOTHING, users: [{ id: 'u9', username: 'widgetfan' }] });
    fixture.detectChanges();

    expect(groups(fixture)['Utilisateurs']).toEqual(['widgetfan']);
    press(fixture, 'Enter');
    expect(navigate).toHaveBeenCalledWith(['/admin/users', 'u9'], { queryParams: undefined });
  });

  it('keeps the pages and the full search when the server fails', () => {
    vi.useFakeTimers();
    const { fixture, http } = setup();
    openPalette(fixture);

    type(fixture, 'compte');
    vi.advanceTimersByTime(200);
    http.expectOne((req) => req.url === '/api/search').flush('boom', { status: 500, statusText: 'Server Error' });
    fixture.detectChanges();

    expect(groups(fixture)).toEqual({ 'Aller à': ['Mon compte'], Recherche: ['Rechercher « compte » partout'] });
    expect(fixture.nativeElement.querySelector('[role="dialog"] gbt-spinner')).toBeNull();
  });

  it('opens the full results page from "Rechercher partout"', () => {
    vi.useFakeTimers();
    const { fixture, http, router } = setup();
    const navigate = vi.spyOn(router, 'navigate').mockResolvedValue(true);
    openPalette(fixture);

    type(fixture, 'zzz');
    vi.advanceTimersByTime(200);
    http.expectOne((req) => req.url === '/api/search').flush(NOTHING);
    fixture.detectChanges();
    press(fixture, 'Enter');

    expect(navigate).toHaveBeenCalledWith(['/search'], { queryParams: { q: 'zzz' } });
    expect(fixture.nativeElement.querySelector('[role="dialog"]')).toBeNull();
  });

  it('opens a "Nouveau …" dialog through the page that holds it', () => {
    const { fixture, http, router } = setup();
    const navigate = vi.spyOn(router, 'navigate').mockResolvedValue(true);
    enterRepository(['acme', 'widget'], 'contributor');
    openPalette(fixture);

    type(fixture, 'nouveau ticket');
    press(fixture, 'Enter');

    expect(navigate).toHaveBeenCalledWith(['/repositories', 'acme', 'widget', '-', 'issues'], { queryParams: { new: 'issue' } });
    http.match(() => true);
  });

  it('signs out through the shell', () => {
    const { fixture, http } = setup();
    const logout = vi.fn();
    fixture.componentInstance.logout.subscribe(logout);
    openPalette(fixture);

    type(fixture, 'déconnexion');
    press(fixture, 'Enter');

    expect(logout).toHaveBeenCalledTimes(1);
    http.match(() => true);
  });
});
