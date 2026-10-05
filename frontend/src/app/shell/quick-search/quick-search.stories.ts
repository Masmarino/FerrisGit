import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { inject, provideAppInitializer, signal } from '@angular/core';
import { provideRouter, withDisabledInitialNavigation } from '@angular/router';
import { of } from 'rxjs';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { RepositoryContext, RepositoryContextService } from '../../repositories/repository-context.service';
import { SearchResponse, SearchService } from '../../search/search.service';
import { PublicSettings, SettingsService } from '../../settings/settings.service';
import { provideFerrisgitIcons } from '../../shared/register-icons';
import { CommandPaletteTrigger } from '@masmarino/gabarit/command-palette';
import { MeService } from '../me.service';
import { QuickSearch } from './quick-search';
import { RecentRepositoriesService } from './recent-repositories.service';

const USER_ID = 'story-user';

const RESULTS: SearchResponse = {
  repositories: [
    { id: 'r1', name: 'ferrisgit-web', description: '', path: ['camille', 'ferrisgit-web'], visibility: 'private' },
    { id: 'r2', name: 'ferrisgit-runner', description: '', path: ['plateforme', 'infra', 'ferrisgit-runner'], visibility: 'public' },
  ],
  issues: [
    { id: 'i1', number: 42, title: 'La pipeline échoue sur les branches protégées', status: 'todo', kind: 'bug', createdAt: '', repository: { id: 'r1', name: 'ferrisgit-web', path: ['camille', 'ferrisgit-web'] } },
  ],
  mergeRequests: [
    { id: 'm1', title: 'Recherche rapide au clavier', status: 'open', sourceBranch: 'quick-search', targetBranch: 'main', createdAt: '', repository: { id: 'r1', name: 'ferrisgit-web', path: ['camille', 'ferrisgit-web'] } },
  ],
  users: [{ id: 'u3', username: 'ferris' }],
};

/** Like the server: only what contains the query. */
function matching(query: string): SearchResponse {
  const has = (text: string): boolean => text.toLowerCase().includes(query.toLowerCase());
  return {
    repositories: RESULTS.repositories.filter((repo) => has(repo.path.join('/'))),
    issues: RESULTS.issues.filter((issue) => has(issue.title)),
    mergeRequests: RESULTS.mergeRequests.filter((mr) => has(mr.title)),
    users: RESULTS.users.filter((user) => has(user.username)),
  };
}

const REPOSITORY: RepositoryContext = { repositoryId: 'repo-1', path: ['camille', 'ferrisgit-web'], role: 'maintainer', ancestors: [], groupId: null };

interface Options {
  admin?: boolean;
  repository?: RepositoryContext | null;
  engine?: PublicSettings['executionEngine'];
}

function providers(options: Options) {
  return [
    { provide: MeService, useValue: { id: signal(USER_ID), isAdmin: signal(options.admin ?? false) } satisfies Partial<MeService> },
    { provide: RepositoryContextService, useValue: { current: signal(options.repository ?? null) } satisfies Pick<RepositoryContextService, 'current'> },
    { provide: SettingsService, useValue: { publicSettings: signal<PublicSettings | null>({ executionEngine: options.engine ?? 'kubernetes' }) } satisfies Pick<SettingsService, 'publicSettings'> },
    { provide: SearchService, useValue: { search: (q: string) => of(matching(q)) } satisfies Pick<SearchService, 'search'> },
  ];
}

// What this browser opened before, so "Récents" has something to show.
const withRecents = applicationConfig({
  providers: [
    provideAppInitializer(() => {
      const recent = inject(RecentRepositoriesService);
      recent.remember(USER_ID, ['plateforme', 'infra', 'terraform']);
      recent.remember(USER_ID, ['plateforme', 'infra', 'runners']);
    }),
  ],
});

const openPalette = async (canvasElement: HTMLElement): Promise<HTMLInputElement> => {
  await userEvent.click(await within(canvasElement).findByRole('button', { name: 'Rechercher ou aller à…' }));
  return within(canvasElement).findByRole<HTMLInputElement>('combobox');
};

const meta: Meta<QuickSearch> = {
  title: 'Shell/QuickSearch',
  component: QuickSearch,
  decorators: [applicationConfig({ providers: [provideRouter([], withDisabledInitialNavigation()), provideFerrisgitIcons()] }), withRecents],
  parameters: { layout: 'padded' },
  // As in the shell: the header's trigger opens it.
  render: () => ({
    template: `<gbt-command-palette-trigger style="max-width: 26rem" label="Rechercher ou aller à…" [palette]="quickSearch" /><fg-quick-search #quickSearch />`,
    moduleMetadata: { imports: [CommandPaletteTrigger, QuickSearch] },
  }),
};

export default meta;
type Story = StoryObj<QuickSearch>;

/** In the header, closed: a field-like button that names the shortcut for this system. */
export const Closed: Story = {
  decorators: [moduleMetadata({ providers: providers({}) })],
};

export const Open: Story = {
  decorators: [moduleMetadata({ providers: providers({}) })],
  play: async ({ canvasElement }) => {
    await openPalette(canvasElement);
  },
};

/** In a repository, as its maintainer: its pages and creation actions come first. */
export const InRepository: Story = {
  decorators: [moduleMetadata({ providers: providers({ admin: true, repository: REPOSITORY, engine: 'docker-runners' }) })],
  play: async ({ canvasElement }) => {
    await openPalette(canvasElement);
    await expect(await within(canvasElement).findByText('Dans ferrisgit-web')).toBeInTheDocument();
  },
};

/** Typing filters the pages at once; what the server finds follows. */
export const Results: Story = {
  decorators: [moduleMetadata({ providers: providers({ admin: true }) })],
  play: async ({ canvasElement }) => {
    const field = await openPalette(canvasElement);
    await userEvent.type(field, 'ferris');
    await waitFor(() => expect(within(canvasElement).getByText('ferrisgit-runner')).toBeInTheDocument());
    await expect(within(canvasElement).getByText('Rechercher « ferris » partout')).toBeInTheDocument();
  },
};

/** Nothing local matches: the server's answer and the full search remain. */
export const OnlyServerResults: Story = {
  decorators: [moduleMetadata({ providers: providers({}) })],
  play: async ({ canvasElement }) => {
    const field = await openPalette(canvasElement);
    await userEvent.type(field, 'protégées');
    await waitFor(() => expect(within(canvasElement).getByText('#42 La pipeline échoue sur les branches protégées')).toBeInTheDocument());
  },
};
