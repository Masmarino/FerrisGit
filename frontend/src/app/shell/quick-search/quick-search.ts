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
import { TranslocoPipe } from '@jsverse/transloco';
import { t, tn } from '../../shared/i18n/translator';

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
/** The words, in the active language, that also find an item: `shell.quickSearch.keywords.<name>`, comma-separated. */
const keywords = (name: string): string[] => t(`shell.quickSearch.keywords.${name}`).split(',');

@Component({
  selector: 'fg-quick-search',
  imports: [TranslocoPipe, CommandPalette],
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
    count === 0 ? t('common.noResults') : tn('common.results', count);

  protected readonly groups = computed<CommandGroup<QuickTarget>[]>(() => {
    const ctx = this.repoContext.current();
    const query = this.query().trim();
    const groups: CommandGroup<QuickTarget>[] = [];
    const recents = this.recentItems(ctx);
    if (recents.length > 0) {
      groups.push({ label: t('shell.quickSearch.recent'), items: recents });
    }
    if (ctx) {
      groups.push({ label: t('shell.quickSearch.inRepository', { name: ctx.path[ctx.path.length - 1] }), items: this.repositoryItems(ctx) });
    }
    groups.push({ label: t('shell.quickSearch.goTo'), items: this.pageItems() });
    groups.push({ label: t('shell.quickSearch.actions'), items: this.actionItems() });
    // Found by the server, so already matching: the palette mustn't filter them again on its own terms. They come after
    // the pages and actions, which show at once, so arriving results never move the option under the arrow keys.
    if (query) {
      groups.push(...this.serverGroups());
      groups.push({
        label: t('shell.quickSearch.search'),
        filter: false,
        items: [{ id: 'search-all', label: t('shell.quickSearch.searchEverywhere', { query }), icon: 'search', data: { link: ['/search'], queryParams: { q: query } } }],
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
      { id: 'repo:overview', label: t('nav.overview'), icon: 'folder-git-2', keywords: keywords('overview'), data: { link: link() } },
      { id: 'repo:pipelines', label: t('nav.pipelines'), icon: 'play', keywords: keywords('pipelines'), data: { link: link('pipelines') } },
      { id: 'repo:merge-requests', label: t('nav.mergeRequests'), icon: 'git-pull-request', keywords: keywords('mergeRequests'), data: { link: link('merge-requests') } },
      { id: 'repo:issues', label: t('nav.issues'), icon: 'circle-dot', keywords: keywords('issues'), data: { link: link('issues') } },
      { id: 'repo:board', label: t('shell.quickSearch.issueBoard'), icon: 'kanban', keywords: keywords('board'), data: { link: link('issues', 'board') } },
      { id: 'repo:releases', label: t('nav.releases'), icon: 'tag', keywords: keywords('releases'), data: { link: link('releases') } },
      { id: 'repo:wiki', label: t('nav.wiki'), icon: 'book-open', data: { link: link('wiki') } },
    ];
    if (canMaintain(ctx.role)) {
      items.push({ id: 'repo:settings', label: t('shell.quickSearch.repositorySettings'), icon: 'settings', keywords: keywords('settings'), data: { link: link('settings') } });
    }
    if (canWrite(ctx.role)) {
      items.push(
        { id: 'repo:new-issue', label: t('shell.quickSearch.newIssue'), icon: 'plus', keywords: keywords('create'), data: { link: link('issues'), queryParams: { [NEW_PARAM]: 'issue' } } },
        { id: 'repo:new-merge-request', label: t('shell.quickSearch.newMergeRequest'), icon: 'plus', keywords: keywords('createMerge'), data: { link: link('merge-requests'), queryParams: { [NEW_PARAM]: 'merge-request' } } },
        { id: 'repo:new-wiki-page', label: t('shell.quickSearch.newWikiPage'), icon: 'plus', keywords: keywords('create'), data: { link: link('wiki', 'new') } },
        { id: 'repo:pipeline-editor', label: t('shell.quickSearch.editPipeline'), icon: 'pencil', keywords: keywords('pipelineEditor'), data: { link: link('pipelines', 'editor') } },
      );
    }
    return items;
  }

  private pageItems(): QuickItem[] {
    const items: QuickItem[] = [
      { id: 'page:home', label: t('nav.home'), icon: 'home', data: { link: ['/home'] } },
      { id: 'page:repositories', label: t('nav.repositories'), icon: 'folder-git-2', data: { link: ['/repositories'] } },
      { id: 'page:groups', label: t('nav.groups'), icon: 'folders', data: { link: ['/repositories'], queryParams: { tab: 'groups' } } },
      { id: 'page:explore', label: t('nav.explore'), description: t('shell.quickSearch.publicRepositories'), icon: 'globe', keywords: keywords('explore'), data: { link: ['/explore'] } },
    ];
    if (this.settings.publicSettings()?.executionEngine === 'docker-runners') {
      items.push({ id: 'page:runners', label: t('nav.runners'), icon: 'server', data: { link: ['/runners'] } });
    }
    items.push(
      { id: 'page:account', label: t('nav.account'), icon: 'user', keywords: keywords('account'), data: { link: ['/account'] } },
      { id: 'page:docs', label: t('nav.docs'), icon: 'book-open', keywords: keywords('docs'), data: { link: ['/docs'] } },
    );
    if (this.me.isAdmin()) {
      items.push(
        { id: 'admin:dashboard', label: t('nav.dashboard'), description: t('nav.administration'), icon: 'layout-dashboard', keywords: keywords('admin'), data: { link: ['/admin/dashboard'] } },
        { id: 'admin:users', label: t('nav.users'), description: t('nav.administration'), icon: 'users', keywords: keywords('users'), data: { link: ['/admin/users'] } },
        { id: 'admin:health', label: t('nav.health'), description: t('nav.administration'), icon: 'activity', keywords: keywords('health'), data: { link: ['/admin/health'] } },
        { id: 'admin:settings', label: t('shell.quickSearch.instanceSettings'), description: t('nav.administration'), icon: 'settings', keywords: keywords('instance'), data: { link: ['/admin/settings'] } },
      );
    }
    return items;
  }

  private actionItems(): QuickItem[] {
    return [
      { id: 'action:new-repository', label: t('shell.quickSearch.newRepository'), icon: 'plus', keywords: keywords('create'), data: { link: ['/repositories'], queryParams: { [NEW_PARAM]: 'repository' } } },
      { id: 'action:new-group', label: t('shell.quickSearch.newGroup'), icon: 'folder', keywords: keywords('create'), data: { link: ['/repositories'], queryParams: { [NEW_PARAM]: 'group' } } },
      { id: 'action:logout', label: t('nav.logout'), icon: 'log-out', keywords: keywords('logout'), data: { logout: true } },
    ];
  }

  private serverGroups(): CommandGroup<QuickTarget>[] {
    const r = this.response();
    const groups: CommandGroup<QuickTarget>[] = [
      {
        label: t('nav.repositories'),
        filter: false,
        items: r.repositories.map((repo) => ({ id: `repository:${repo.id}`, label: repo.name, description: repo.path.join('/'), icon: 'folder-git-2', data: { link: ['/repositories', ...repo.path] } })),
      },
      {
        label: t('nav.issues'),
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
        label: t('nav.mergeRequests'),
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
        label: t('nav.users'),
        filter: false,
        items: r.users.map((user) => ({ id: `user:${user.id}`, label: user.username, icon: 'user', data: { link: ['/admin/users', user.id] } })),
      });
    }
    return groups;
  }
}
