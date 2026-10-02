import { afterNextRender, Component, computed, DestroyRef, ElementRef, HostListener, inject, Injector, OnInit, signal, viewChild } from '@angular/core';
import { NgTemplateOutlet } from '@angular/common';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { Router, RouterLink, RouterLinkActive, RouterOutlet } from '@angular/router';
import { Subject, catchError, debounceTime, distinctUntilChanged, of, switchMap } from 'rxjs';
import { AuthService } from '../auth/auth.service';
import { currentUrl } from '../shared/current-url';
import { SearchResponse, SearchService } from '../search/search.service';
import { BreadcrumbSwitcherService } from './breadcrumb-switcher.service';
import { MeService } from './me.service';
import { NotificationBell } from './notification-bell/notification-bell';
import { PageTitleService } from './page-title.service';
import { RepositoryContext, RepositoryContextService } from '../repositories/repository-context.service';
import { SettingsService } from '../settings/settings.service';
import { SidebarCollapseService } from './sidebar-collapse.service';
import { AppShell as GbtAppShell, AppShellNavGroup, Breadcrumb, Icon, Menu, MenuItem, SearchBar, SearchResultCategory, Toaster } from '@masmarino/gabarit';

interface QuickSearchResult {
  kind: 'repository' | 'issue' | 'mergeRequest' | 'user';
  id: string;
  label: string;
  link: string[] | null;
}

interface NavItem {
  action: string;
  icon: 'home' | 'folder-git-2' | 'server' | 'settings';
  text: string;
  link: string;
  children?: { action: string; icon: string; text: string; link: string }[];
}

const EMPTY_SEARCH_RESPONSE: SearchResponse = { repositories: [], issues: [], mergeRequests: [], users: [] };

@Component({
  selector: 'fg-app-shell',
  standalone: true,
  imports: [RouterOutlet, RouterLink, RouterLinkActive, Icon, NotificationBell, Toaster, GbtAppShell, AppShellNavGroup, SearchBar, Breadcrumb, Menu, MenuItem, NgTemplateOutlet],
  templateUrl: './app-shell.html',
  styleUrl: './app-shell.scss',
})
export class AppShell implements OnInit {
  private auth = inject(AuthService);
  private router = inject(Router);
  private search = inject(SearchService);
  private destroyRef = inject(DestroyRef);
  protected me = inject(MeService);
  protected pageTitle = inject(PageTitleService);
  protected repoContext = inject(RepositoryContextService);
  protected settings = inject(SettingsService);
  protected sidebarCollapse = inject(SidebarCollapseService);
  protected switcher = inject(BreadcrumbSwitcherService);
  private injector = inject(Injector);
  private searchBar = viewChild('searchBar', { read: ElementRef });
  protected mobileSearchOpen = signal(false);
  searchQuery = signal('');
  private searchInput$ = new Subject<string>();
  private searchResponse = signal<SearchResponse>(EMPTY_SEARCH_RESPONSE);

  readonly searchResults = computed<SearchResultCategory<QuickSearchResult>[]>(() => {
    const r = this.searchResponse();
    const categories: SearchResultCategory<QuickSearchResult>[] = [];
    if (r.repositories.length > 0) {
      categories.push({
        label: 'Dépôts',
        icon: 'folder-git-2',
        items: r.repositories.map((repo) => ({
          kind: 'repository' as const,
          id: repo.id,
          label: repo.path.join('/'),
          link: ['/repositories', ...repo.path],
        })),
      });
    }
    if (r.issues.length > 0) {
      categories.push({
        label: 'Tickets',
        icon: 'circle-dot',
        items: r.issues.map((issue) => ({
          kind: 'issue' as const,
          id: issue.id,
          label: `${issue.repository.path.join('/')}#${issue.number} — ${issue.title}`,
          link: ['/repositories', ...issue.repository.path, '-', 'issues', String(issue.number)],
        })),
      });
    }
    if (r.mergeRequests.length > 0) {
      categories.push({
        label: 'Demandes de fusion',
        icon: 'git-pull-request',
        items: r.mergeRequests.map((mr) => ({
          kind: 'mergeRequest' as const,
          id: mr.id,
          label: `${mr.repository.path.join('/')} — ${mr.title}`,
          link: ['/repositories', ...mr.repository.path, '-', 'merge-requests', mr.id],
        })),
      });
    }
    if (r.users.length > 0) {
      categories.push({
        label: 'Utilisateurs',
        icon: 'user',
        items: r.users.map((user) => ({ kind: 'user' as const, id: user.id, label: user.username, link: null })),
      });
    }
    return categories;
  });

  readonly searchResultLabel = (item: QuickSearchResult): string => item.label;

  /** Announced when the results open (Gabarit's default is English). */
  readonly resultsAnnouncement = (count: number): string => `${count} résultat${count > 1 ? 's' : ''}`;

  constructor() {
    this.searchInput$
      .pipe(
        debounceTime(250),
        distinctUntilChanged(),
        switchMap((q) => (q ? this.search.search(q).pipe(catchError(() => of(EMPTY_SEARCH_RESPONSE))) : of(EMPTY_SEARCH_RESPONSE))),
        takeUntilDestroyed(this.destroyRef),
      )
      .subscribe((r) => this.searchResponse.set(r));
  }

  readonly navItems = computed<NavItem[]>(() => {
    const items: NavItem[] = [
      { action: 'home', icon: 'home', link: '/home', text: 'Accueil' },
      { action: 'repositories', icon: 'folder-git-2', link: '/repositories', text: 'Dépôts' },
    ];
    if (this.settings.publicSettings()?.executionEngine === 'docker-runners') {
      items.push({ action: 'runners', icon: 'server', link: '/runners', text: 'Runners' });
    }
    if (this.me.isAdmin()) {
      items.push({
        action: 'admin',
        icon: 'settings',
        link: '/admin',
        text: 'Admin',
        children: [
          { action: 'settings', icon: 'settings', link: '/admin/settings', text: 'Réglages' },
          { action: 'users', icon: 'users', link: '/admin/users', text: 'Utilisateurs' },
          { action: 'dashboard', icon: 'layout-dashboard', link: '/admin/dashboard', text: 'Dashboard' },
          { action: 'health', icon: 'activity', link: '/admin/health', text: 'Santé' },
        ],
      });
    }
    return items;
  });

  // Follows every completed navigation, so groups auto-expand with the route.
  private url = currentUrl();

  // A manual toggle wins for good; a group without an entry follows the route.
  private readonly menuManualOverrides = signal<Record<string, boolean>>({});

  protected isMenuOpen(item: NavItem): boolean {
    return this.menuManualOverrides()[item.action] ?? this.url().startsWith(item.link);
  }

  protected setMenuOpen(item: NavItem, open: boolean): void {
    this.menuManualOverrides.update((overrides) => ({ ...overrides, [item.action]: open }));
  }

  protected repoLink(ctx: RepositoryContext, subPage: string[]): string[] {
    return subPage.length === 0 ? ['/repositories', ...ctx.path] : ['/repositories', ...ctx.path, '-', ...subPage];
  }

  protected onSwitcherOpen(ctx: RepositoryContext): void {
    if (ctx.groupId) {
      this.switcher.loadForGroup(ctx.groupId);
    } else {
      this.switcher.loadForOwner(ctx.path[0]);
    }
  }

  protected groupLink(ctx: RepositoryContext, group: { name: string }): string[] {
    // Only used by a group's own switcher, so ancestors is never empty.
    return [...ctx.ancestors[ctx.ancestors.length - 1].link, group.name];
  }

  ngOnInit(): void {
    this.me.load();
    this.settings.loadPublic();
  }

  onSearchInput(query: string): void {
    this.searchQuery.set(query);
    this.searchInput$.next(query.trim());
  }

  onSelectResult(item: QuickSearchResult): void {
    this.searchQuery.set('');
    this.searchInput$.next('');
    if (item.link) {
      this.router.navigate(item.link);
    }
  }

  onSearchSubmitEnter(): void {
    const q = this.searchQuery().trim();
    if (!q || this.searchResults().length > 0) {
      return;
    }
    this.mobileSearchOpen.set(false);
    this.router.navigate(['/search'], { queryParams: { q } });
  }

  @HostListener('document:keydown.escape')
  onEscape(): void {
    if (!this.mobileSearchOpen()) {
      return;
    }
    this.mobileSearchOpen.set(false);
    // Gabarit gap: `gbt-search-bar` only refocuses its toggle from its own close button, not when `expanded` is set from
    // outside. `.gbt-sb-toggle` is its internal class. Remove once `blurSearch` keeps focus in the bar.
    afterNextRender(
      () => (this.searchBar()?.nativeElement as HTMLElement | undefined)?.querySelector<HTMLElement>('.gbt-sb-toggle')?.focus(),
      { injector: this.injector },
    );
  }

  logout(): void {
    this.auth.logout();
    this.router.navigateByUrl('/login');
  }
}
