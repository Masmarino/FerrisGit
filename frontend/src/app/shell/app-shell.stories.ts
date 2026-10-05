import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { Component, inject, provideAppInitializer, signal } from '@angular/core';
import { provideRouter, Router, withDisabledInitialNavigation } from '@angular/router';
import { provideLocationMocks } from '@angular/common/testing';
import { of } from 'rxjs';
import { userEvent, within } from 'storybook/test';
import { AppShell } from './app-shell';
import { AuthService } from '../auth/auth.service';
import { NotificationsService } from '../notifications/notifications.service';
import { RepositoryContext, RepositoryContextService } from '../repositories/repository-context.service';
import { SearchService } from '../search/search.service';
import { PublicSettings, SettingsService } from '../settings/settings.service';
import { BreadcrumbSwitcherService } from './breadcrumb-switcher.service';
import { Me, MeService } from './me.service';
import { VersionService } from './version.service';
import { PageTitleService } from './page-title.service';
import { SidebarCollapseService } from './sidebar-collapse.service';
import { provideFerrisgitIcons } from '../shared/register-icons';
import { GbtToastService } from '@masmarino/gabarit/toaster';

// Environment providers only work in `applicationConfig`, not `moduleMetadata`.
// Routing is off (there are no pages here); the active-link story resolves one catch-all route.
@Component({ selector: 'fg-story-blank', template: '', standalone: true })
class BlankPage {}

// Mock locations keep navigation off the real URL, so reloading the story still works.
const withActiveRoute = applicationConfig({
  providers: [
    provideLocationMocks(),
    provideAppInitializer(() => {
      const router = inject(Router);
      router.resetConfig([{ path: '**', component: BlankPage }]);
      return router.navigateByUrl('/repositories');
    }),
  ],
});

const withApp = applicationConfig({ providers: [provideRouter([], withDisabledInitialNavigation()), provideFerrisgitIcons()] });

interface ShellOptions {
  me: Me;
  title?: string;
  repository?: RepositoryContext | null;
  executionEngine?: PublicSettings['executionEngine'];
  collapsed?: boolean;
  unreadCount?: number;
}

const ADMIN: Me = { id: 'u1', username: 'camille', email: 'camille@ferrisgit.example', isAdmin: true, createdAt: '2025-03-12T09:00:00Z' };
const USER: Me = { id: 'u2', username: 'julien', email: 'julien@ferrisgit.example', isAdmin: false, createdAt: '2025-03-12T09:00:00Z' };

const REPOSITORY: RepositoryContext = {
  repositoryId: 'repo-1',
  path: ['camille', 'ferrisgit-web'],
  role: 'owner',
  ancestors: [],
  groupId: null,
};

const GROUP_REPOSITORY: RepositoryContext = {
  repositoryId: 'repo-2',
  path: ['plateforme', 'infra', 'runners'],
  role: 'contributor',
  ancestors: [
    { label: 'plateforme', link: ['/groups', 'plateforme'] },
    { label: 'infra', link: ['/groups', 'plateforme', 'infra'] },
  ],
  groupId: 'group-infra',
};

function shellProviders(options: ShellOptions) {
  return [
    { provide: AuthService, useValue: { logout: () => {} } satisfies Pick<AuthService, 'logout'> },
    {
      provide: MeService,
      useValue: {
        id: signal(options.me.id),
        username: signal(options.me.username),
        email: signal(options.me.email),
        isAdmin: signal(options.me.isAdmin),
        createdAt: signal(options.me.createdAt),
        load: () => {},
      } satisfies Partial<MeService>,
    },
    { provide: PageTitleService, useValue: { title: signal(options.title ?? 'FerrisGit') } satisfies Pick<PageTitleService, 'title'> },
    { provide: RepositoryContextService, useValue: { current: signal(options.repository ?? null) } satisfies Pick<RepositoryContextService, 'current'> },
    {
      provide: SettingsService,
      useValue: {
        publicSettings: signal<PublicSettings | null>({ executionEngine: options.executionEngine ?? 'kubernetes' }),
        loadPublic: () => {},
      } satisfies Pick<SettingsService, 'publicSettings' | 'loadPublic'>,
    },
    { provide: VersionService, useValue: { version: signal('0.1.3'), load: () => {} } satisfies Pick<VersionService, 'version' | 'load'> },
    {
      provide: SidebarCollapseService,
      useValue: { collapsed: signal(options.collapsed ?? false), set: () => {} } satisfies Pick<SidebarCollapseService, 'collapsed' | 'set'>,
    },
    {
      provide: BreadcrumbSwitcherService,
      useValue: {
        // Like the real service: sub-groups only in a group's switcher, never in a personal owner's.
        groups: signal(options.repository?.ancestors.length ? [{ id: 'g1', name: 'observabilite' }] : []),
        repositories: signal([
          { id: 'repo-2', path: ['plateforme', 'infra', 'runners'] },
          { id: 'repo-3', path: ['plateforme', 'infra', 'terraform'] },
        ]),
        loading: signal(false),
        loadForGroup: () => {},
        loadForOwner: () => {},
      } satisfies Pick<BreadcrumbSwitcherService, 'groups' | 'repositories' | 'loading' | 'loadForGroup' | 'loadForOwner'>,
    },
    {
      provide: SearchService,
      useValue: { search: () => of({ repositories: [], issues: [], mergeRequests: [], users: [] }) } satisfies Pick<SearchService, 'search'>,
    },
    {
      provide: NotificationsService,
      useValue: {
        list: () => of([]),
        unreadCount: () => of({ count: options.unreadCount ?? 0 }),
        markRead: () => of(undefined),
        markAllRead: () => of(undefined),
      } satisfies Pick<NotificationsService, 'list' | 'unreadCount' | 'markRead' | 'markAllRead'>,
    },
  ];
}

const meta: Meta<AppShell> = {
  title: 'Shell/AppShell',
  component: AppShell,
  tags: ['autodocs'],
  decorators: [withApp],
  parameters: { layout: 'fullscreen' },
};

export default meta;
type Story = StoryObj<AppShell>;

export const PlainUser: Story = {
  decorators: [moduleMetadata({ providers: shellProviders({ me: USER, title: 'Accueil', unreadCount: 2 }) })],
};

export const AdminGroupCollapsed: Story = {
  decorators: [moduleMetadata({ providers: shellProviders({ me: ADMIN, title: 'Accueil' }) })],
};

// The Admin group only opens by itself under /admin and routing is off, so the story toggles it.
export const AdminGroupExpanded: Story = {
  decorators: [moduleMetadata({ providers: shellProviders({ me: ADMIN, title: 'Réglages' }) })],
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: 'Administration' }));
  },
};

export const AdminGroupExpandedCollapsedRail: Story = {
  decorators: [moduleMetadata({ providers: shellProviders({ me: ADMIN, title: 'Réglages', collapsed: true }) })],
  play: AdminGroupExpanded.play,
};

export const WithRunners: Story = {
  decorators: [moduleMetadata({ providers: shellProviders({ me: ADMIN, title: 'Runners', executionEngine: 'docker-runners' }) })],
};

export const SidebarCollapsed: Story = {
  decorators: [moduleMetadata({ providers: shellProviders({ me: USER, title: 'Dépôts', collapsed: true }) })],
};

export const RepositorySelected: Story = {
  decorators: [moduleMetadata({ providers: shellProviders({ me: ADMIN, title: 'Pipelines', repository: REPOSITORY, unreadCount: 5 }) })],
};

export const RepositoryOverviewReader: Story = {
  decorators: [moduleMetadata({ providers: shellProviders({ me: USER, title: '', repository: { ...REPOSITORY, role: 'reader' } }) })],
};

export const GroupRepositorySelected: Story = {
  decorators: [moduleMetadata({ providers: shellProviders({ me: USER, title: 'Tickets', repository: GROUP_REPOSITORY }) })],
};

export const UserMenuOpen: Story = {
  decorators: [moduleMetadata({ providers: shellProviders({ me: USER, title: 'Accueil' }) })],
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(await canvas.findByRole('button', { name: 'julien' }));
  },
};

// routerLinkActive needs a finished navigation, hence the navigate before render.
export const ActiveNavItem: Story = {
  decorators: [
    withActiveRoute,
    moduleMetadata({ providers: shellProviders({ me: USER, title: 'Dépôts' }) }),
  ],
};

export const ActiveNavItemCollapsed: Story = {
  decorators: [
    withActiveRoute,
    moduleMetadata({ providers: shellProviders({ me: USER, title: 'Dépôts', collapsed: true }) }),
  ],
};

const withToasts = applicationConfig({
  providers: [
    provideAppInitializer(() => {
      const toasts = inject(GbtToastService);
      toasts.show('Dépôt créé.', 'success');
      toasts.show('Le jeton expire dans 3 jours.', 'warning');
      toasts.show('Impossible de fusionner : un conflit a été détecté.', 'error');
      toasts.show('La pipeline a démarré.', 'info');
    }),
  ],
});

export const WithToasts: Story = {
  decorators: [withToasts, moduleMetadata({ providers: shellProviders({ me: USER, title: 'Dépôts' }) })],
};

export const QuickSearchOpen: Story = {
  decorators: [moduleMetadata({ providers: shellProviders({ me: ADMIN, title: 'Pipelines', repository: REPOSITORY }) })],
  play: async ({ canvasElement }) => {
    await userEvent.click(await within(canvasElement).findByRole('button', { name: 'Rechercher ou aller à…' }));
  },
};
