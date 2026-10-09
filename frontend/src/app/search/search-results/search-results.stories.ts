import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { expect, userEvent, waitFor, within } from 'storybook/test';
import { ActivatedRoute, convertToParamMap, provideRouter, withDisabledInitialNavigation } from '@angular/router';
import { NEVER, Observable, of, throwError } from 'rxjs';
import { SearchResults } from './search-results';
import { SearchIssueResult, SearchMergeRequestResult, SearchRepositoryRef, SearchRepositoryResult, SearchResponse, SearchService } from '../search.service';
import { provideFerrisgitIcons } from '../../shared/register-icons';
import { daysAgo, hoursAgo, inShellContentArea, minutesAgo } from '../../shared/layout/page-story-helpers';

const ref = (...path: string[]): SearchRepositoryRef => ({ id: path.join('/'), name: path.at(-1)!, path });
const FERRISGIT = ref('florian', 'ferrisgit');
const GABARIT = ref('florian', 'gabarit');
// A group repository, so three path segments.
const RUNNER = ref('plateforme', 'infra', 'runner');

let nextId = 0;
function repository(path: string[], fields: Partial<SearchRepositoryResult> = {}): SearchRepositoryResult {
  return { id: `repo-${++nextId}`, name: path.at(-1)!, description: '', path, visibility: 'private', ...fields };
}

function issue(fields: Partial<SearchIssueResult> & Pick<SearchIssueResult, 'number' | 'title'>): SearchIssueResult {
  return { id: `issue-${++nextId}`, status: 'todo', kind: 'task', createdAt: daysAgo(2), repository: FERRISGIT, ...fields };
}

function mergeRequest(fields: Partial<SearchMergeRequestResult> & Pick<SearchMergeRequestResult, 'title'>): SearchMergeRequestResult {
  return { id: `mr-${++nextId}`, status: 'open', sourceBranch: 'feat/runner', targetBranch: 'main', createdAt: daysAgo(1), repository: FERRISGIT, ...fields };
}

const POPULATED: SearchResponse = {
  repositories: [
    repository(['plateforme', 'infra', 'runner'], { description: 'Le runner de CI de FerrisGit : exécute les jobs des pipelines dans des pods Kubernetes.' }),
    repository(['florian', 'runner-images'], { description: 'Images de base des jobs.', visibility: 'public' }),
    repository(['bastien', 'runner-playground']),
  ],
  issues: [
    issue({ number: 42, title: 'Le runner perd la connexion pendant un job long', kind: 'bug', status: 'in_progress', repository: RUNNER, createdAt: hoursAgo(5) }),
    issue({ number: 38, title: 'Afficher la version du runner dans l’administration', kind: 'feature', createdAt: daysAgo(3) }),
    issue({ number: 12, title: 'Documenter l’enregistrement des runners', status: 'in_review', repository: RUNNER, createdAt: daysAgo(9) }),
    issue({ number: 7, title: 'Nettoyer les runners hors ligne', kind: 'epic', status: 'done', createdAt: daysAgo(40) }),
  ],
  mergeRequests: [
    mergeRequest({ title: 'Reconnecter le runner après une coupure réseau', sourceBranch: 'fix/runner-reconnect', repository: RUNNER, createdAt: minutesAgo(35) }),
    mergeRequest({ title: 'Page d’administration des runners', status: 'merged', sourceBranch: 'feat/runners-admin', createdAt: daysAgo(6) }),
    mergeRequest({ title: 'Runner : ancien protocole', status: 'closed', sourceBranch: 'old/runner-v1', targetBranch: 'develop', repository: GABARIT, createdAt: daysAgo(18) }),
  ],
  users: [
    { id: 'u1', username: 'runner-bot' },
    { id: 'u2', username: 'rune' },
  ],
};

const CAPPED: SearchResponse = {
  repositories: Array.from({ length: 8 }, (_, i) => repository(['florian', `service-${i + 1}`], { description: i % 2 === 0 ? `Service n°${i + 1} de la plateforme.` : '', visibility: i % 3 === 0 ? 'public' : 'private' })),
  issues: Array.from({ length: 8 }, (_, i) =>
    issue({ number: 100 + i, title: `Le service ${i + 1} ne démarre pas après une mise à jour`, kind: i % 2 === 0 ? 'bug' : 'task', status: i % 4 === 0 ? 'in_progress' : 'todo', createdAt: daysAgo(i + 1) }),
  ),
  mergeRequests: [mergeRequest({ title: 'Redémarrer les services proprement', sourceBranch: 'fix/service-restart' })],
  users: [],
};

const LONG: SearchResponse = {
  repositories: [
    repository(['organisation-avec-un-nom-tres-long', 'equipe-plateforme', 'infrastructure-des-runners-kubernetes'], {
      description:
        'Tout ce qu’il faut pour faire tourner les runners sur Kubernetes : charts Helm, images, scripts de migration, tableaux de bord de supervision et procédures d’astreinte détaillées pour chaque incident connu.',
    }),
  ],
  issues: [
    issue({
      number: 1234,
      title: 'Quand un runner Kubernetes redémarre pendant un job, la pipeline reste « en cours » indéfiniment et bloque les suivantes de la même branche',
      kind: 'bug',
      repository: ref('organisation-avec-un-nom-tres-long', 'equipe-plateforme', 'infrastructure-des-runners-kubernetes'),
    }),
  ],
  mergeRequests: [
    mergeRequest({
      title: 'Remplacer toutes les confirmations natives par la modale de confirmation partagée du design system Gabarit',
      sourceBranch: 'feature/remplacer-les-confirmations-natives-par-la-modale-partagee',
      targetBranch: 'release/2026-09-hotfixes-plateforme',
      status: 'merged',
    }),
  ],
  users: [{ id: 'u-long', username: 'utilisateur-avec-un-identifiant-particulierement-long-pour-tester' }],
};

const EMPTY: SearchResponse = { repositories: [], issues: [], mergeRequests: [], users: [] };

function withSearch(query: string | null, response$: Observable<SearchResponse>) {
  return moduleMetadata({
    providers: [
      { provide: ActivatedRoute, useValue: { queryParamMap: of(convertToParamMap(query !== null ? { q: query } : {})) } satisfies Pick<ActivatedRoute, 'queryParamMap'> },
      { provide: SearchService, useValue: { search: () => response$ } satisfies Pick<SearchService, 'search'> },
    ],
  });
}

const rect = (el: Element) => el.getBoundingClientRect();
const centreY = (el: Element) => rect(el).top + rect(el).height / 2;

function assertPageLayout(canvas: HTMLElement): void {
  const main = canvas.querySelector('.gbt-page-layout__main');
  const cards = Array.from(canvas.querySelectorAll('.search-results__section'));
  if (!main || cards.length === 0) throw new Error('results not rendered yet');

  const doc = canvas.ownerDocument.documentElement;
  if (doc.scrollWidth > doc.clientWidth + 1) throw new Error(`horizontal overflow: ${doc.scrollWidth}px of content in ${doc.clientWidth}px`);

  const h1 = canvas.querySelector('gbt-page-header h1')!;
  if (Math.abs(rect(h1).left - rect(cards[0]).left) > 1) throw new Error('the h1 and the cards do not share a left edge');

  const tabs = canvas.querySelector('.search-results__tabs .gbt-segmented-control')!;
  if (rect(tabs).right > rect(main).right + 0.5) throw new Error('the tabs overflow the main column');
  for (const option of Array.from(tabs.querySelectorAll('[role="radio"]'))) {
    if (rect(option).right > rect(tabs).right + 0.5) throw new Error('a tab spills out of the track');
  }

  for (const row of Array.from(canvas.querySelectorAll('.search-results__rows gbt-list-row'))) {
    const title = row.querySelector('.gbt-list-row__title')!;
    const line = centreY(title.firstElementChild!);
    const leading = row.querySelector('.gbt-list-row__leading gbt-icon');
    if (leading && Math.abs(centreY(leading) - line) > 1) throw new Error('leading icon off the title line');
    const trailing = row.querySelector('.gbt-list-row__trailing > *');
    const meta = row.querySelector('.gbt-list-row__meta');
    const wrapped = trailing && meta && rect(trailing).top >= rect(meta).top;
    if (trailing && !wrapped && Math.abs(centreY(trailing) - line) > 1.5) throw new Error('trailing badge off the title line');
    for (const child of Array.from(title.children)) {
      if (rect(child).right > rect(title).right + 0.5) throw new Error('title content spills out of its column');
    }
    const column = row.querySelector('.gbt-list-row__main')!;
    for (const chip of Array.from(row.querySelectorAll('.search-results__repo, .search-results__branches'))) {
      if (rect(chip).right > rect(column).right + 0.5) throw new Error('a meta chip spills out of its column');
    }
    if (rect(row).right > rect(main).right + 0.5) throw new Error('row wider than the main column');
  }
}

async function expectPageLayout({ canvasElement }: { canvasElement: HTMLElement }) {
  await waitFor(() => assertPageLayout(canvasElement), { timeout: 3000 });
}

const meta: Meta<SearchResults> = {
  title: 'Search/SearchResults',
  component: SearchResults,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [applicationConfig({ providers: [provideRouter([], withDisabledInitialNavigation()), provideFerrisgitIcons()] }), inShellContentArea],
};

export default meta;
type Story = StoryObj<SearchResults>;

export const Populated: Story = {
  decorators: [withSearch('runner', of(POPULATED))],
  play: async (context) => {
    await expectPageLayout(context);
    const tabs = Array.from(context.canvasElement.querySelectorAll('.search-results__tabs [role="radio"]'), (tab) => tab.textContent?.trim());
    await expect(tabs).toEqual(['Tout (12)', 'Dépôts (3)', 'Tickets (4)', 'Demandes de fusion (3)', 'Utilisateurs (2)']);
  },
};

export const IssuesTab: Story = {
  decorators: [withSearch('runner', of(POPULATED))],
  play: async (context) => {
    const canvas = within(context.canvasElement);
    await userEvent.click(await canvas.findByRole('radio', { name: 'Tickets (4)' }));
    await expectPageLayout(context);
    await expect(context.canvasElement.querySelectorAll('.search-results__section')).toHaveLength(1);
  },
};

export const Capped: Story = {
  decorators: [withSearch('service', of(CAPPED))],
  play: async (context) => {
    await expectPageLayout(context);
    await expect(context.canvasElement.querySelectorAll('.search-results__capped')).toHaveLength(2);
  },
};

export const LongContent: Story = {
  decorators: [withSearch('une recherche avec beaucoup de mots pour voir comment le titre se comporte', of(LONG))],
  play: expectPageLayout,
};

export const OnlyUsers: Story = {
  decorators: [withSearch('rune', of({ ...EMPTY, users: POPULATED.users }))],
  play: expectPageLayout,
};

export const NoQuery: Story = {
  decorators: [withSearch(null, of(EMPTY))],
};

export const NoResults: Story = {
  decorators: [withSearch('introuvable', of(EMPTY))],
};

export const Loading: Story = {
  decorators: [withSearch('runner', NEVER)],
};

export const Failed: Story = {
  decorators: [withSearch('runner', throwError(() => new Error('boom')))],
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.querySelector('.search-results__failed')).not.toBeNull());
  },
};
