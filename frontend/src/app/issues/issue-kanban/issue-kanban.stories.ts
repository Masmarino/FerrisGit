import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { expect, waitFor } from 'storybook/test';
import { NEVER, of, throwError } from 'rxjs';
import { provideRouter, withDisabledInitialNavigation } from '@angular/router';
import { IssueKanban } from './issue-kanban';
import { Issue, IssuesService } from '../issues.service';
import { Label, LabelsService } from '../../labels/labels.service';
import { Milestone, MilestonesService } from '../../milestones/milestones.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { GbtToastService } from '@masmarino/gabarit';
import { provideFerrisgitIcons } from '../../shared/register-icons';
import { atPhoneWidth, daysAgo, inShellContentArea } from '../../shared/layout/page-story-helpers';
import {
  ALICE,
  BASTIEN,
  BUG,
  DOCS,
  FLORIAN,
  GOOD_FIRST,
  LABELS,
  LONG_TITLE,
  LONG_USERNAME,
  MILESTONES,
  PERF,
  TITLE_SUBJECTS,
  TITLE_VERBS,
  UI,
  URGENT,
  fakeRepositoryContextService,
  fakeToast,
  label,
} from '../issue-story-fixtures';

let nextId = 0;
function issue(fields: Partial<Issue> & Pick<Issue, 'number' | 'title'>): Issue {
  return {
    id: `issue-${++nextId}`,
    authorId: ALICE.id,
    assigneeId: fields.assignee?.id ?? null,
    description: '',
    status: 'todo',
    kind: 'task',
    parentIssueId: null,
    createdAt: daysAgo(2),
    closedAt: fields.status === 'done' ? daysAgo(1) : null,
    milestoneId: null,
    labels: [],
    author: ALICE,
    assignee: null,
    commentCount: 0,
    ...fields,
  };
}

const MIXED: Issue[] = [
  issue({ number: 12, title: 'La pagination saute une page quand on filtre par label', kind: 'bug', labels: [BUG, URGENT], milestoneId: 'm1', assignee: BASTIEN, commentCount: 3 }),
  issue({ number: 8, title: 'Refonte du moteur de recherche', kind: 'epic', milestoneId: 'm2', commentCount: 1 }),
  issue({ number: 7, title: 'Mettre en cache les avatars', labels: [PERF], assignee: ALICE }),
  issue({ number: 11, title: 'Ajouter un mode sombre à l’éditeur de wiki', kind: 'feature', status: 'in_progress', labels: [UI], assignee: FLORIAN, commentCount: 12 }),
  issue({ number: 9, title: 'Documenter l’enregistrement des runners', status: 'in_review', labels: [DOCS], milestoneId: 'm1', assignee: BASTIEN }),
  issue({ number: 4, title: 'Écrire la doc d’onboarding', status: 'done', labels: [DOCS], commentCount: 5, assignee: ALICE }),
  issue({ number: 2, title: 'Le bouton « Cloner » ne copie rien sous Firefox', kind: 'bug', status: 'done', labels: [BUG] }),
];

const LONG: Issue[] = [
  issue({
    number: 1287,
    title: LONG_TITLE,
    kind: 'bug',
    labels: [BUG, URGENT],
    milestoneId: 'm1',
    assignee: { id: 'u9', username: LONG_USERNAME },
    commentCount: 128,
  }),
  issue({ number: 1286, title: 'Proposer des modèles de tickets par dépôt', kind: 'feature', status: 'in_progress', labels: [UI, DOCS, PERF, GOOD_FIRST, URGENT, BUG], milestoneId: 'm2', commentCount: 2 }),
  issue({ number: 1285, title: 'Ticketsansespacesdansletitrequidoitquandmêmepasseràlalignesansdéborderdelacarte', status: 'in_review', assignee: FLORIAN }),
  issue({ number: 1284, title: 'Un label au nom très long', status: 'done', labels: [label('l-long', 'un label dont le nom est vraiment beaucoup trop long pour une carte', '#0f766e')] }),
];

const MANY_IN_ONE_LANE: Issue[] = [
  ...Array.from({ length: 24 }, (_, index) =>
    issue({
      number: 200 - index,
      title: `${TITLE_VERBS[index % TITLE_VERBS.length]} ${TITLE_SUBJECTS[index % TITLE_SUBJECTS.length]}`,
      kind: (['task', 'bug', 'feature'] as const)[index % 3],
      labels: index % 4 === 0 ? [BUG] : index % 5 === 0 ? [UI, PERF] : [],
      assignee: index % 3 === 0 ? BASTIEN : null,
      commentCount: index % 4,
    }),
  ),
  issue({ number: 150, title: 'Migrer les runners vers la nouvelle API', status: 'in_progress', assignee: FLORIAN }),
  issue({ number: 149, title: 'Relire la politique de rétention des artefacts', status: 'in_review', commentCount: 2 }),
];

function fakeIssuesService(issues: Issue[], overrides: Partial<Record<keyof IssuesService, unknown>> = {}) {
  return {
    list: () => of(issues),
    updateStatus: () => of(issues[0]),
    ...overrides,
  };
}

function withData(options: { issues?: Issue[]; role?: 'owner' | 'reader'; labels?: Label[]; milestones?: Milestone[]; issuesService?: unknown } = {}) {
  return moduleMetadata({
    providers: [
      // A fresh copy per story, so a drag in one doesn't leak into the next.
      { provide: IssuesService, useValue: options.issuesService ?? fakeIssuesService((options.issues ?? MIXED).map((item) => ({ ...item }))) },
      { provide: LabelsService, useValue: { listForRepository: () => of(options.labels ?? LABELS) } },
      { provide: MilestonesService, useValue: { listForRepository: () => of(options.milestones ?? MILESTONES) } },
      { provide: RepositoryContextService, useValue: fakeRepositoryContextService(options.role ?? 'owner') },
      { provide: GbtToastService, useValue: fakeToast },
    ],
  });
}

const rect = (el: Element) => el.getBoundingClientRect();

/** Layout checks jsdom can't make: only the board scrolls sideways, four level lanes of at least 260px, cards, titles and menus inside their boxes. */
function assertBoardLayout(canvas: HTMLElement): void {
  const board = canvas.querySelector('.issue-kanban__board');
  const lanes = Array.from(canvas.querySelectorAll('.issue-kanban__lane'));
  if (!board || lanes.length !== 4) throw new Error('board not rendered yet');

  const doc = canvas.ownerDocument.documentElement;
  if (doc.scrollWidth > doc.clientWidth + 1) throw new Error(`page overflows sideways: ${doc.scrollWidth}px of content in ${doc.clientWidth}px`);
  if (rect(board).right > doc.clientWidth + 0.5) throw new Error('the board is wider than the page');

  for (const [index, lane] of lanes.entries()) {
    if (rect(lane).width < 259.5) throw new Error(`lane ${index} is narrower than 260px`);
    if (Math.abs(rect(lane).top - rect(lanes[0]).top) > 0.5) throw new Error('lanes are not level');
    if (index > 0 && rect(lane).left < rect(lanes[index - 1]).right) throw new Error('lanes overlap');
    const list = lane.querySelector('.issue-kanban__cards');
    for (const card of Array.from(lane.querySelectorAll('.issue-kanban__card'))) {
      if (rect(card).left < rect(lane).left || rect(card).right > rect(lane).right + 0.5) throw new Error('card outside its lane');
      const link = card.querySelector('.issue-kanban__card-link')!;
      if (rect(link).right > rect(card).right) throw new Error('title spills out of its card');
      const trigger = card.querySelector('.issue-kanban__move .gbt-menu__trigger');
      if (trigger) {
        const firstLine = rect(link).top + 11;
        const centre = rect(trigger).top + rect(trigger).height / 2;
        if (Math.abs(centre - firstLine) > 1.5) throw new Error('card menu off the title line');
      }
      for (const tag of Array.from(card.querySelectorAll('.issue-kanban__card-tags > *'))) {
        if (rect(tag).right > rect(card).right) throw new Error('tag spills out of its card');
      }
    }
    if (list && rect(list).bottom > rect(lane).bottom + 0.5) throw new Error('card list taller than its lane');
  }
}

async function expectBoardLayout({ canvasElement }: { canvasElement: HTMLElement }) {
  await waitFor(() => assertBoardLayout(canvasElement), { timeout: 3000 });
}

const laneCards = (canvas: HTMLElement, status: string) => Array.from(canvas.querySelectorAll<HTMLElement>(`.issue-kanban__lane[data-status="${status}"] .issue-kanban__card`));

const meta: Meta<IssueKanban> = {
  title: 'Issues/IssueKanban',
  component: IssueKanban,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  args: { repositoryId: 'repo-1', path: ['alice', 'ferrisgit'] },
  decorators: [applicationConfig({ providers: [provideRouter([], withDisabledInitialNavigation()), provideFerrisgitIcons()] }), inShellContentArea],
};

export default meta;
type Story = StoryObj<IssueKanban>;

export const Populated: Story = {
  decorators: [withData({ issues: MIXED.filter((item) => item.number !== 9) })],
  play: async (context) => {
    await expectBoardLayout(context);
    await expect(context.canvasElement.querySelector('.issue-kanban__lane[data-status="in_review"] .issue-kanban__empty')?.textContent?.trim()).toBe('Aucun ticket');
  },
};

/** Phone width: the search field has to take a full line below 560px via `@container gbt-page-layout (max-width: 560px)`. Rename the container and it stays at its 22rem maximum, failing this check. */
export const PhoneWidth: Story = {
  decorators: [withData({ issues: MIXED.filter((item) => item.number !== 9) }), atPhoneWidth],
  play: async (context) => {
    await expectBoardLayout(context);
    const { canvasElement } = context;
    await expect(canvasElement.querySelector('gbt-page-layout')!.getBoundingClientRect().width).toBeLessThan(561);
    const toolbar = canvasElement.querySelector('.issue-kanban__toolbar')!.getBoundingClientRect();
    const search = canvasElement.querySelector('.issue-kanban__search')!.getBoundingClientRect();
    await expect(Math.round(search.width), 'search field spans the toolbar').toBe(Math.round(toolbar.width));
  },
};

export const Empty: Story = {
  decorators: [withData({ issues: [], labels: [], milestones: [] })],
  play: expectBoardLayout,
};

export const ManyCardsInOneLane: Story = {
  decorators: [withData({ issues: MANY_IN_ONE_LANE })],
  play: async (context) => {
    await expectBoardLayout(context);
    const list = context.canvasElement.querySelector<HTMLElement>('.issue-kanban__lane[data-status="todo"] .issue-kanban__cards')!;
    await expect(list.scrollHeight, 'the lane scrolls its cards').toBeGreaterThan(list.clientHeight);
  },
};

export const LongTitlesAndLabels: Story = {
  decorators: [withData({ issues: LONG })],
  play: expectBoardLayout,
};

export const Filtered: Story = {
  decorators: [withData()],
  play: async (context) => {
    const input = await waitFor(() => {
      const found = context.canvasElement.querySelector<HTMLInputElement>('.issue-kanban__search input');
      if (!found) throw new Error('search field not rendered yet');
      return found;
    });
    input.value = 'doc';
    input.dispatchEvent(new Event('input'));
    await waitFor(() => expect(context.canvasElement.querySelector('.issue-kanban__summary')?.textContent?.trim()).toBe('2 tickets sur 7'));
    await expect(context.canvasElement.querySelector('.issue-kanban__reset')).not.toBeNull();
    await expectBoardLayout(context);
  },
};

export const ReadOnly: Story = {
  decorators: [withData({ role: 'reader' })],
  play: async (context) => {
    await expectBoardLayout(context);
    await expect(context.canvasElement.querySelector('.issue-kanban__move')).toBeNull();
    await expect(context.canvasElement.querySelector('.issue-kanban__card--draggable')).toBeNull();
  },
};

export const Loading: Story = {
  decorators: [withData({ issuesService: fakeIssuesService([], { list: () => NEVER }) })],
};

export const LoadError: Story = {
  decorators: [withData({ issuesService: fakeIssuesService([], { list: () => throwError(() => new Error('500')) }) })],
};

export const MoveMenuOpen: Story = {
  decorators: [withData()],
  play: async (context) => {
    await expectBoardLayout(context);
    const trigger = context.canvasElement.querySelector<HTMLButtonElement>('.issue-kanban__lane[data-status="in_progress"] .issue-kanban__move .gbt-menu__trigger')!;
    trigger.click();
    await waitFor(() => expect(context.canvasElement.querySelectorAll('[role="menuitem"]').length).toBe(3));
  },
};

/**
 * Drags `card` over `target` with real mouse events, the way the CDK listens (a pressed button and a click count,
 * or it takes them for a screen reader's fake events). Releases unless told not to.
 */
async function dragCard(card: HTMLElement, target: HTMLElement, options: { release: boolean }): Promise<void> {
  const doc = card.ownerDocument;
  const view = doc.defaultView!;
  const from = rect(card);
  const to = rect(target);
  const pointer = (x: number, y: number, buttons: number) => ({ bubbles: true, cancelable: true, view, button: 0, buttons, detail: 1, clientX: x, clientY: y });
  const start = { x: from.left + 24, y: from.bottom - 12 };
  const end = { x: to.left + 60, y: to.top + 40 };
  card.dispatchEvent(new MouseEvent('mousedown', pointer(start.x, start.y, 1)));
  const steps = 12;
  for (let step = 1; step <= steps; step++) {
    doc.dispatchEvent(new MouseEvent('mousemove', pointer(start.x + ((end.x - start.x) * step) / steps, start.y + ((end.y - start.y) * step) / steps, 1)));
    await new Promise((resolve) => setTimeout(resolve, 16));
  }
  if (options.release) {
    doc.dispatchEvent(new MouseEvent('mouseup', pointer(end.x, end.y, 0)));
  }
}

const cardNumbered = (canvas: HTMLElement, status: string, number: string) => laneCards(canvas, status).find((item) => item.querySelector('.issue-kanban__number')?.textContent === number)!;
const cardList = (canvas: HTMLElement, status: string) => canvas.querySelector<HTMLElement>(`.issue-kanban__lane[data-status="${status}"] .issue-kanban__cards`)!;

export const DragToAnotherLane: Story = {
  decorators: [withData({ issues: MIXED.filter((item) => item.number !== 9) })],
  play: async (context) => {
    const canvas = context.canvasElement;
    await expectBoardLayout(context);
    await dragCard(cardNumbered(canvas, 'todo', '#7'), cardList(canvas, 'in_review'), { release: true });
    await waitFor(() => expect(laneCards(canvas, 'in_review').map((item) => item.querySelector('.issue-kanban__number')?.textContent)).toEqual(['#7']), { timeout: 5000 });
    await expect(laneCards(canvas, 'todo').length).toBe(2);
    await expect(canvas.querySelector('.issue-kanban__lane[data-status="in_review"] .issue-kanban__lane-header gbt-badge')?.textContent?.trim()).toBe('1');
  },
};

export const DragInProgress: Story = {
  decorators: [withData({ issues: MIXED.filter((item) => item.number !== 9) })],
  play: async (context) => {
    const canvas = context.canvasElement;
    await expectBoardLayout(context);
    await dragCard(cardNumbered(canvas, 'todo', '#7'), cardList(canvas, 'in_review'), { release: false });
    await waitFor(() => expect(cardList(canvas, 'in_review').querySelector('.cdk-drag-placeholder')).not.toBeNull(), { timeout: 5000 });
    await expect(canvas.ownerDocument.querySelector('.cdk-drag-preview')).not.toBeNull();
    await waitFor(() => expect(Array.from(canvas.querySelectorAll('.cdk-drop-list-dragging'), (list) => list.id)).toEqual(['column-in_review']), { timeout: 5000 });
    await expect(getComputedStyle(cardList(canvas, 'in_review').querySelector('.issue-kanban__empty')!).display).toBe('none');
  },
};
