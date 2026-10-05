import { Component, computed, DestroyRef, effect, inject, output, signal, untracked } from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { Params, Router } from '@angular/router';
import { Subject, catchError, debounceTime, distinctUntilChanged, of, switchMap } from 'rxjs';
import { CommandGroup, CommandItem, CommandPalette } from '@masmarino/gabarit/command-palette';
import { RepositoryContext, RepositoryContextService } from '../../repositories/repository-context.service';
import { canMaintain, canWrite } from '../../repositories/repository-role';
import { SearchResponse, SearchService } from '../../search/search.service';
import { SettingsService } from '../../settings/settings.service';
import { NEW_PARAM } from '../../shared/open-when-asked';
import { MeService } from '../me.service';
import { RecentRepositoriesService } from './recent-repositories.service';

/** Where a choice leads: a page, or signing out. */
export type QuickTarget = { link: string[]; queryParams?: Params } | { logout: true };

type QuickItem = CommandItem<QuickTarget>;

const EMPTY_RESPONSE: SearchResponse = { repositories: [], issues: [], mergeRequests: [], users: [] };

/**
 * The quick search, opened with ⌘K (Ctrl K elsewhere), `/` or the header's `gbt-command-palette-trigger` (give it this
 * component): what this browser opened last, the current repository's pages, every page of the menu and the
 * "Nouveau …" actions, filtered as one types, then what the server finds and a way to the full results page. Placed
 * outside the header, whose dark theme it would otherwise take on.
 */
@Component({
  selector: 'fg-quick-search',
  imports: [CommandPalette],
  templateUrl: './quick-search.html',
})
export class QuickSearch {
  private router = inject(Router);
  private search = inject(SearchService);
  private me = inject(MeService);
  private repoContext = inject(RepositoryContextService);
  private settings = inject(SettingsService);
  private recent = inject(RecentRepositoriesService);

  /** Signing out belongs to the shell, which also offers it from the account menu. */
  readonly logout = output<void>();

  protected readonly shortcuts = ['mod+k', '/'];
  protected readonly open = signal(false);
  protected readonly query = signal('');
  protected readonly searching = signal(false);
  private readonly response = signal<SearchResponse>(EMPTY_RESPONSE);
  private readonly query$ = new Subject<string>();

  protected readonly resultsAnnouncement = (count: number): string =>
    count === 0 ? 'Aucun résultat' : `${count} résultat${count > 1 ? 's' : ''}`;

  protected readonly groups = computed<CommandGroup<QuickTarget>[]>(() => {
    const ctx = this.repoContext.current();
    const query = this.query().trim();
    const groups: CommandGroup<QuickTarget>[] = [];
    const recents = this.recentItems(ctx);
    if (recents.length > 0) {
      groups.push({ label: 'Récents', items: recents });
    }
    if (ctx) {
      groups.push({ label: `Dans ${ctx.path[ctx.path.length - 1]}`, items: this.repositoryItems(ctx) });
    }
    groups.push({ label: 'Aller à', items: this.pageItems() });
    groups.push({ label: 'Actions', items: this.actionItems() });
    // Found by the server, so already matching: the palette mustn't filter them again on its own terms. They come after
    // the pages and actions, which show at once, so arriving results never move the option under the arrow keys.
    if (query) {
      groups.push(...this.serverGroups());
      groups.push({
        label: 'Recherche',
        filter: false,
        items: [{ id: 'search-all', label: `Rechercher « ${query} » partout`, icon: 'search', data: { link: ['/search'], queryParams: { q: query } } }],
      });
    }
    return groups;
  });

  constructor() {
    this.query$
      .pipe(
        debounceTime(200),
        distinctUntilChanged(),
        switchMap((q) => (q ? this.search.search(q).pipe(catchError(() => of(EMPTY_RESPONSE))) : of(EMPTY_RESPONSE))),
        takeUntilDestroyed(inject(DestroyRef)),
      )
      .subscribe((response) => {
        this.response.set(response);
        this.searching.set(false);
      });

    // Every repository opened joins the recent ones, whichever way it was reached.
    effect(() => {
      const ctx = this.repoContext.current();
      const userId = this.me.id();
      if (ctx && userId) {
        untracked(() => this.recent.remember(userId, ctx.path));
      }
    });
  }

  /** Opens the palette, for the header's trigger. */
  show(): void {
    this.open.set(true);
  }

  protected onQueryChange(query: string): void {
    this.query.set(query);
    const q = query.trim();
    if (!q) {
      // Nothing to wait for: the stale results go at once rather than after the debounce.
      this.response.set(EMPTY_RESPONSE);
    }
    this.searching.set(q !== '');
    this.query$.next(q);
  }

  protected onSelect(item: QuickItem): void {
    const target = item.data;
    if (!target) {
      return;
    }
    if ('logout' in target) {
      this.logout.emit();
      return;
    }
    this.router.navigate(target.link, { queryParams: target.queryParams });
  }

  private recentItems(ctx: RepositoryContext | null): QuickItem[] {
    // The repository one is in is already the group below.
    const here = ctx?.path.join('/');
    return this.recent
      .list(this.me.id())
      .filter((path) => path.join('/') !== here)
      .map((path) => ({
        id: `recent:${path.join('/')}`,
        label: path[path.length - 1],
        description: path.join('/'),
        icon: 'clock',
        data: { link: ['/repositories', ...path] },
      }));
  }

  private repositoryItems(ctx: RepositoryContext): QuickItem[] {
    const link = (...subPage: string[]): string[] => (subPage.length === 0 ? ['/repositories', ...ctx.path] : ['/repositories', ...ctx.path, '-', ...subPage]);
    const items: QuickItem[] = [
      { id: 'repo:overview', label: 'Aperçu', icon: 'folder-git-2', keywords: ['code', 'fichiers', 'readme'], data: { link: link() } },
      { id: 'repo:pipelines', label: 'Pipelines', icon: 'play', keywords: ['ci', 'jobs'], data: { link: link('pipelines') } },
      { id: 'repo:merge-requests', label: 'Demandes de fusion', icon: 'git-pull-request', keywords: ['merge', 'mr'], data: { link: link('merge-requests') } },
      { id: 'repo:issues', label: 'Tickets', icon: 'circle-dot', keywords: ['issues'], data: { link: link('issues') } },
      { id: 'repo:board', label: 'Tableau des tickets', icon: 'kanban', keywords: ['kanban', 'board'], data: { link: link('issues', 'board') } },
      { id: 'repo:releases', label: 'Releases', icon: 'tag', keywords: ['versions'], data: { link: link('releases') } },
      { id: 'repo:wiki', label: 'Wiki', icon: 'book-open', data: { link: link('wiki') } },
    ];
    if (canMaintain(ctx.role)) {
      items.push({ id: 'repo:settings', label: 'Réglages du dépôt', icon: 'settings', keywords: ['membres', 'webhooks', 'variables'], data: { link: link('settings') } });
    }
    if (canWrite(ctx.role)) {
      items.push(
        { id: 'repo:new-issue', label: 'Nouveau ticket', icon: 'plus', keywords: ['créer'], data: { link: link('issues'), queryParams: { [NEW_PARAM]: 'issue' } } },
        { id: 'repo:new-merge-request', label: 'Nouvelle demande de fusion', icon: 'plus', keywords: ['créer', 'merge'], data: { link: link('merge-requests'), queryParams: { [NEW_PARAM]: 'merge-request' } } },
        { id: 'repo:new-wiki-page', label: 'Nouvelle page de wiki', icon: 'plus', keywords: ['créer'], data: { link: link('wiki', 'new') } },
      );
    }
    return items;
  }

  private pageItems(): QuickItem[] {
    const items: QuickItem[] = [
      { id: 'page:home', label: 'Accueil', icon: 'home', data: { link: ['/home'] } },
      { id: 'page:repositories', label: 'Dépôts', icon: 'folder-git-2', data: { link: ['/repositories'] } },
      { id: 'page:groups', label: 'Groupes', icon: 'folders', data: { link: ['/repositories'], queryParams: { tab: 'groups' } } },
      { id: 'page:explore', label: 'Explorer', description: 'Les dépôts publics', icon: 'globe', keywords: ['catalogue', 'public'], data: { link: ['/explore'] } },
    ];
    if (this.settings.publicSettings()?.executionEngine === 'docker-runners') {
      items.push({ id: 'page:runners', label: 'Runners', icon: 'server', data: { link: ['/runners'] } });
    }
    items.push(
      { id: 'page:account', label: 'Mon compte', icon: 'user', keywords: ['profil', 'mot de passe', 'sécurité', 'mfa', 'email'], data: { link: ['/account'] } },
      { id: 'page:docs', label: 'Documentation', icon: 'book-open', keywords: ['aide'], data: { link: ['/docs'] } },
    );
    if (this.me.isAdmin()) {
      items.push(
        { id: 'admin:dashboard', label: 'Tableau de bord', description: 'Administration', icon: 'layout-dashboard', keywords: ['admin'], data: { link: ['/admin/dashboard'] } },
        { id: 'admin:users', label: 'Utilisateurs', description: 'Administration', icon: 'users', keywords: ['admin', 'comptes', 'inviter'], data: { link: ['/admin/users'] } },
        { id: 'admin:health', label: 'Santé', description: 'Administration', icon: 'activity', keywords: ['admin', 'état'], data: { link: ['/admin/health'] } },
        { id: 'admin:settings', label: "Réglages de l'instance", description: 'Administration', icon: 'settings', keywords: ['admin', 'smtp', 'kubernetes', 'docker'], data: { link: ['/admin/settings'] } },
      );
    }
    return items;
  }

  private actionItems(): QuickItem[] {
    return [
      { id: 'action:new-repository', label: 'Nouveau dépôt', icon: 'plus', keywords: ['créer'], data: { link: ['/repositories'], queryParams: { [NEW_PARAM]: 'repository' } } },
      { id: 'action:new-group', label: 'Nouveau groupe', icon: 'folder', keywords: ['créer'], data: { link: ['/repositories'], queryParams: { [NEW_PARAM]: 'group' } } },
      { id: 'action:logout', label: 'Déconnexion', icon: 'log-out', keywords: ['quitter', 'se déconnecter'], data: { logout: true } },
    ];
  }

  private serverGroups(): CommandGroup<QuickTarget>[] {
    const r = this.response();
    const groups: CommandGroup<QuickTarget>[] = [
      {
        label: 'Dépôts',
        filter: false,
        items: r.repositories.map((repo) => ({ id: `repository:${repo.id}`, label: repo.name, description: repo.path.join('/'), icon: 'folder-git-2', data: { link: ['/repositories', ...repo.path] } })),
      },
      {
        label: 'Tickets',
        filter: false,
        items: r.issues.map((issue) => ({
          id: `issue:${issue.id}`,
          label: `#${issue.number} ${issue.title}`,
          description: issue.repository.path.join('/'),
          icon: 'circle-dot',
          data: { link: ['/repositories', ...issue.repository.path, '-', 'issues', String(issue.number)] },
        })),
      },
      {
        label: 'Demandes de fusion',
        filter: false,
        items: r.mergeRequests.map((mr) => ({
          id: `merge-request:${mr.id}`,
          label: mr.title,
          description: mr.repository.path.join('/'),
          icon: 'git-pull-request',
          data: { link: ['/repositories', ...mr.repository.path, '-', 'merge-requests', mr.id] },
        })),
      },
    ];
    // Only administrators have a page per user to go to.
    if (this.me.isAdmin()) {
      groups.push({
        label: 'Utilisateurs',
        filter: false,
        items: r.users.map((user) => ({ id: `user:${user.id}`, label: user.username, icon: 'user', data: { link: ['/admin/users', user.id] } })),
      });
    }
    return groups;
  }
}
