import { Component, computed, DestroyRef, inject, OnInit, signal } from '@angular/core';
import { NgTemplateOutlet } from '@angular/common';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { NavigationEnd, Router, RouterLink, RouterLinkActive, RouterOutlet } from '@angular/router';
import { filter } from 'rxjs';
import { AuthService } from '../auth/auth.service';
import { currentUrl } from '../shared/current-url';
import { BreadcrumbSwitcherService } from './breadcrumb-switcher.service';
import { MeService } from './me.service';
import { NotificationBell } from './notification-bell/notification-bell';
import { PageTitleService } from './page-title.service';
import { QuickSearch } from './quick-search/quick-search';
import { Crumb, pageTrail } from './page-trail';
import { RepositoryContext, RepositoryContextService } from '../repositories/repository-context.service';
import { SettingsService } from '../settings/settings.service';
import { SidebarCollapseService } from './sidebar-collapse.service';
import { VersionService } from './version.service';
import { AppShell as GbtAppShell, AppShellNavGroup } from '@masmarino/gabarit/app-shell';
import { Breadcrumb } from '@masmarino/gabarit/breadcrumb';
import { CommandPaletteTrigger } from '@masmarino/gabarit/command-palette';
import { Icon } from '@masmarino/gabarit/icon';
import { Menu, MenuItem } from '@masmarino/gabarit/menu';
import { Toaster } from '@masmarino/gabarit/toaster';
import { TranslocoPipe } from '@jsverse/transloco';
import { t } from '../shared/i18n/translator';

interface NavItem {
  action: string;
  icon: 'home' | 'folder-git-2' | 'server' | 'settings';
  text: string;
  link: string;
  children?: { action: string; icon: string; text: string; link: string }[];
}

@Component({
  selector: 'fg-app-shell',
  standalone: true,
  imports: [TranslocoPipe, RouterOutlet, RouterLink, RouterLinkActive, Icon, NotificationBell, Toaster, GbtAppShell, AppShellNavGroup, QuickSearch, CommandPaletteTrigger, Breadcrumb, Menu, MenuItem, NgTemplateOutlet],
  templateUrl: './app-shell.html',
  styleUrl: './app-shell.scss',
})
export class AppShell implements OnInit {
  private auth = inject(AuthService);
  private router = inject(Router);
  private destroyRef = inject(DestroyRef);
  protected me = inject(MeService);
  protected pageTitle = inject(PageTitleService);
  protected repoContext = inject(RepositoryContextService);
  protected settings = inject(SettingsService);
  protected sidebarCollapse = inject(SidebarCollapseService);
  protected switcher = inject(BreadcrumbSwitcherService);
  protected version = inject(VersionService);
  /** What sits above the current page outside a repository, from its route; the page's title names the page itself. */
  protected readonly trail = signal<Crumb[]>([]);

  constructor() {
    this.trail.set(pageTrail(this.router.routerState.snapshot.root));
    this.router.events
      .pipe(
        filter((event) => event instanceof NavigationEnd),
        takeUntilDestroyed(this.destroyRef),
      )
      .subscribe(() => this.trail.set(pageTrail(this.router.routerState.snapshot.root)));
  }

  readonly navItems = computed<NavItem[]>(() => {
    const items: NavItem[] = [
      { action: 'home', icon: 'home', link: '/home', text: t('nav.home') },
      { action: 'repositories', icon: 'folder-git-2', link: '/repositories', text: t('nav.repositories') },
    ];
    if (this.settings.publicSettings()?.executionEngine === 'docker-runners') {
      items.push({ action: 'runners', icon: 'server', link: '/runners', text: t('nav.runners') });
    }
    if (this.me.isAdmin()) {
      items.push({
        action: 'admin',
        icon: 'settings',
        link: '/admin',
        text: t('nav.administration'),
        children: [
          { action: 'dashboard', icon: 'layout-dashboard', link: '/admin/dashboard', text: t('nav.dashboard') },
          { action: 'users', icon: 'users', link: '/admin/users', text: t('nav.users') },
          { action: 'health', icon: 'activity', link: '/admin/health', text: t('nav.health') },
          { action: 'settings', icon: 'settings', link: '/admin/settings', text: t('nav.settings') },
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
    this.version.load();
  }

  logout(): void {
    this.auth.logout();
    this.router.navigateByUrl('/login');
  }
}
