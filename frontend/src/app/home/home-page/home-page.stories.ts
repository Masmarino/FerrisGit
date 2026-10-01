import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { expect, waitFor } from 'storybook/test';
import { signal } from '@angular/core';
import { provideRouter, withDisabledInitialNavigation } from '@angular/router';
import { NEVER, Observable, of, throwError } from 'rxjs';
import { HomePage } from './home-page';
import { DashboardResponse, DashboardService } from '../dashboard.service';
import { Notification } from '../../notifications/notifications.service';
import { SearchIssueResult, SearchMergeRequestResult, SearchRepositoryRef } from '../../search/search.service';
import { MeService } from '../../shell/me.service';
import { provideFerrisgitIcons } from '../../shared/register-icons';
import { daysAgo, hoursAgo, inShellContentArea, minutesAgo } from '../../shared/layout/page-story-helpers';

const repo = (...path: string[]): SearchRepositoryRef => ({ id: path.join('/'), name: path.at(-1)!, path });
const FERRISGIT = repo('florian', 'ferrisgit');
const GABARIT = repo('florian', 'gabarit');
const RUNNER = repo('plateforme', 'infra', 'runner');

let nextId = 0;
function issue(fields: Partial<SearchIssueResult> & Pick<SearchIssueResult, 'number' | 'title'>): SearchIssueResult {
  return { id: `issue-${++nextId}`, status: 'todo', kind: 'task', createdAt: daysAgo(2), repository: FERRISGIT, ...fields };
}

function mergeRequest(fields: Partial<SearchMergeRequestResult> & Pick<SearchMergeRequestResult, 'title'>): SearchMergeRequestResult {
  return { id: `mr-${++nextId}`, status: 'open', sourceBranch: 'feat/branch', targetBranch: 'main', createdAt: daysAgo(1), repository: FERRISGIT, ...fields };
}

function notification(fields: Partial<Notification> & Pick<Notification, 'kind'>): Notification {
  return {
    id: `n-${++nextId}`,
    repositoryOwner: 'florian',
    repositoryName: 'ferrisgit',
    actorUsername: 'alice',
    mergeRequestId: null,
    mergeRequestTitle: null,
    pipelineId: null,
    commitSha: null,
    role: null,
    issueId: null,
    issueNumber: null,
    issueTitle: null,
    read: true,
    createdAt: hoursAgo(3),
    ...fields,
  };
}

const ACTIVITY: Notification[] = [
  notification({ kind: 'merge_request_approved', mergeRequestId: 'mr-a', mergeRequestTitle: 'Refonte de la page d’accueil', read: false, createdAt: minutesAgo(12) }),
  notification({ kind: 'issue_assigned', actorUsername: 'bastien', issueId: 'i-a', issueNumber: 42, read: false, createdAt: hoursAgo(2) }),
  notification({ kind: 'pipeline_failed', actorUsername: null, pipelineId: 'p-1', commitSha: '9f3c2a17d0b4e5f6', createdAt: hoursAgo(5) }),
  notification({ kind: 'merge_request_commented', actorUsername: 'bastien', mergeRequestId: 'mr-b', mergeRequestTitle: 'Découper le lecteur gix par module', createdAt: daysAgo(1) }),
  notification({ kind: 'collaborator_added', repositoryOwner: 'plateforme', repositoryName: 'runner', role: 'developer', createdAt: daysAgo(3) }),
  notification({ kind: 'issue_closed', issueId: 'i-b', issueNumber: 7, createdAt: daysAgo(6) }),
];

const POPULATED: DashboardResponse = {
  assignedIssues: [
    issue({ number: 42, title: 'Le pipeline échoue quand le runner perd la connexion', kind: 'bug', status: 'in_progress', createdAt: hoursAgo(2) }),
    issue({ number: 38, title: 'Afficher les dates relatives dans la liste des pipelines', kind: 'feature', createdAt: daysAgo(3) }),
    issue({ number: 12, title: 'Documenter l’enregistrement des runners', kind: 'task', status: 'in_review', repository: RUNNER, createdAt: daysAgo(9) }),
  ],
  authoredIssues: [],
  authoredMergeRequests: [
    mergeRequest({ title: 'Refonte de la page d’accueil', createdAt: hoursAgo(20) }),
    mergeRequest({ title: 'Ajouter le composant de pagination', repository: GABARIT, createdAt: daysAgo(4) }),
  ],
  mergeRequestsToReview: [mergeRequest({ title: 'Découper le lecteur gix par module', createdAt: daysAgo(1) })],
  activity: ACTIVITY,
};

const CAPPED: DashboardResponse = {
  ...POPULATED,
  assignedIssues: Array.from({ length: 20 }, (_, i) =>
    issue({ number: 100 + i, title: `Ticket de triage n°${i + 1}`, kind: i % 3 === 0 ? 'bug' : 'task', status: i % 4 === 0 ? 'in_progress' : 'todo', createdAt: daysAgo(i + 1) }),
  ),
};

const LONG: DashboardResponse = {
  assignedIssues: [
    issue({
      number: 1234,
      title: 'Quand un runner Kubernetes redémarre pendant un job, le pipeline reste « en cours » indéfiniment et bloque les suivants de la même branche',
      kind: 'bug',
      repository: repo('organisation-avec-un-nom-tres-long', 'equipe-plateforme', 'infrastructure-des-runners-kubernetes'),
    }),
  ],
  authoredIssues: [issue({ number: 7, title: 'Court', kind: 'epic', status: 'done' })],
  authoredMergeRequests: [mergeRequest({ title: 'Remplacer toutes les confirmations natives par la modale de confirmation partagée du design system Gabarit', status: 'merged' })],
  mergeRequestsToReview: [mergeRequest({ title: 'Fermer les anciennes branches', status: 'closed' })],
  activity: [
    notification({
      kind: 'merge_request_changes_requested',
      actorUsername: 'utilisateur-avec-un-identifiant-particulierement-long',
      mergeRequestId: 'mr-long',
      mergeRequestTitle: 'Remplacer toutes les confirmations natives par la modale de confirmation partagée',
      read: false,
    }),
    ...ACTIVITY.slice(0, 2),
  ],
};

const EMPTY: DashboardResponse = { assignedIssues: [], authoredIssues: [], authoredMergeRequests: [], mergeRequestsToReview: [], activity: [] };

function withDashboard(response$: Observable<DashboardResponse>, username = 'florian') {
  return moduleMetadata({
    providers: [
      { provide: DashboardService, useValue: { get: () => response$ } satisfies Pick<DashboardService, 'get'> },
      { provide: MeService, useValue: { username: signal(username) } },
    ],
  });
}

const rect = (el: Element) => el.getBoundingClientRect();
const centreY = (el: Element) => rect(el).top + rect(el).height / 2;

function assertPageLayout(canvas: HTMLElement): void {
  const layout = canvas.querySelector('gbt-page-layout');
  const main = canvas.querySelector('.gbt-page-layout__main');
  const aside = canvas.querySelector('.gbt-page-layout__aside');
  if (!layout || !main || !aside) throw new Error('page layout not rendered yet');

  const doc = canvas.ownerDocument.documentElement;
  if (doc.scrollWidth > doc.clientWidth + 1) throw new Error(`horizontal overflow: ${doc.scrollWidth}px of content in ${doc.clientWidth}px`);

  if (rect(layout).width >= 769) {
    if (rect(aside).left < rect(main).right) throw new Error('the aside should sit beside the main column');
    if (Math.abs(rect(aside).top - rect(main).top) > 1) throw new Error('the aside should start level with the main column');
  } else if (rect(aside).top < rect(main).bottom) {
    throw new Error('the aside should stack under the main column');
  }

  const tiles = Array.from(canvas.querySelectorAll('gbt-stat-tile'));
  if (tiles.length !== 4) throw new Error('tiles not rendered yet');
  const rowsOfTiles = new Set(tiles.map((tile) => Math.round(rect(tile).top))).size;
  const expectedRows = rect(canvas.querySelector('.gbt-stat-grid')!).width >= 600 ? 1 : 2;
  if (rowsOfTiles !== expectedRows) throw new Error(`tiles on ${rowsOfTiles} rows, expected ${expectedRows}`);
  for (const tile of tiles) {
    const label = tile.querySelector('.gbt-stat-tile__label')!;
    if (rect(label).right > rect(tile).right) throw new Error('tile label spills out of its tile');
  }

  for (const row of Array.from(canvas.querySelectorAll('.home-page__rows gbt-list-row'))) {
    const link = row.querySelector('.gbt-list-row__title > a')!;
    const line = centreY(link);
    const icon = row.querySelector('.gbt-list-row__leading gbt-icon');
    if (!icon || Math.abs(centreY(icon) - line) > 1) throw new Error('kind icon off the title line');
    const badge = row.querySelector('.gbt-list-row__trailing fg-status-badge')!;
    if (rect(badge).top < rect(row.querySelector('.gbt-list-row__meta')!).top && Math.abs(centreY(badge) - line) > 1) throw new Error('status badge off the title line');
    if (rect(link).right > rect(row.querySelector('.gbt-list-row__title')!).right + 0.5) throw new Error('title link spills out of its column');
    const chip = row.querySelector('.home-page__repo')!;
    if (rect(chip).right > rect(row.querySelector('.gbt-list-row__main')!).right + 0.5) throw new Error('repository chip spills out of its column');
    if (rect(row).right > rect(main).right + 0.5) throw new Error('row wider than the main column');
  }

  for (const event of Array.from(canvas.querySelectorAll('.home-page__event'))) {
    const marker = event.querySelector('.home-page__event-marker')!;
    const link = event.querySelector('.home-page__event-link')!;
    const firstLine = rect(link).top + 10; // half of the 1.25rem line
    if (Math.abs(centreY(marker) - firstLine) > 1) throw new Error('activity marker off its first line');
  }
}

async function expectPageLayout({ canvasElement }: { canvasElement: HTMLElement }) {
  await waitFor(() => assertPageLayout(canvasElement), { timeout: 3000 });
}

const meta: Meta<HomePage> = {
  title: 'Home/HomePage',
  component: HomePage,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [applicationConfig({ providers: [provideRouter([], withDisabledInitialNavigation()), provideFerrisgitIcons()] }), inShellContentArea],
};

export default meta;
type Story = StoryObj<HomePage>;

export const Populated: Story = {
  decorators: [withDashboard(of(POPULATED))],
  play: async (context) => {
    await expectPageLayout(context);
    const counts = Array.from(context.canvasElement.querySelectorAll('.gbt-stat-tile__value'), (el) => el.textContent?.trim());
    await expect(counts).toEqual(['3', '0', '2', '1']);
    await expect(context.canvasElement.querySelector('.home-page__unread-count')?.textContent?.trim()).toBe('2 non lues');
  },
};

export const Capped: Story = {
  decorators: [withDashboard(of(CAPPED))],
  play: async (context) => {
    await expectPageLayout(context);
    await expect(context.canvasElement.querySelector('.gbt-stat-tile__value')?.textContent?.trim()).toBe('20+');
  },
};

export const LongContent: Story = {
  decorators: [withDashboard(of(LONG))],
  play: expectPageLayout,
};

export const NothingToDo: Story = {
  decorators: [withDashboard(of(EMPTY))],
  play: async (context) => {
    await expectPageLayout(context);
    await expect(context.canvasElement.querySelector('gbt-empty-state')).not.toBeNull();
  },
};

export const NothingToDoWithActivity: Story = {
  decorators: [withDashboard(of({ ...EMPTY, activity: ACTIVITY }))],
  play: expectPageLayout,
};

export const Loading: Story = {
  decorators: [withDashboard(NEVER)],
};

export const LoadFailed: Story = {
  decorators: [withDashboard(throwError(() => new Error('boom')))],
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.querySelector('gbt-alert')).not.toBeNull());
    await expect(canvasElement.querySelector('.gbt-page-layout__aside:not(:empty)')).toBeNull();
  },
};

export const WithoutUser: Story = {
  decorators: [withDashboard(of(POPULATED), '')],
};
