import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { expect, waitFor } from 'storybook/test';
import { NEVER, of, throwError } from 'rxjs';
import { provideRouter, withDisabledInitialNavigation } from '@angular/router';
import { IssueList } from './issue-list';
import { Issue, IssuesService } from '../issues.service';
import { Label, LabelsService } from '../../labels/labels.service';
import { Milestone, MilestonesService } from '../../milestones/milestones.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { MeService } from '../../shell/me.service';
import { GbtToastService } from '@masmarino/gabarit';
import { provideFerrisgitIcons } from '../../shared/register-icons';
import { daysAgo, hoursAgo, inShellContentArea, minutesAgo } from '../../shared/layout/page-story-helpers';
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
    closedAt: null,
    milestoneId: null,
    labels: [],
    author: ALICE,
    assignee: null,
    commentCount: 0,
    ...fields,
  };
}

const MIXED: Issue[] = [
  issue({ number: 12, title: 'La pagination saute une page quand on filtre par label', kind: 'bug', labels: [BUG, URGENT], milestoneId: 'm1', assignee: BASTIEN, commentCount: 3, createdAt: hoursAgo(2) }),
  issue({ number: 11, title: 'Ajouter un mode sombre à l’éditeur de wiki', kind: 'feature', status: 'in_progress', labels: [UI], assignee: FLORIAN, commentCount: 12, createdAt: daysAgo(1), author: FLORIAN }),
  issue({ number: 9, title: 'Documenter l’enregistrement des runners', status: 'in_review', labels: [DOCS], milestoneId: 'm1', createdAt: daysAgo(6), author: BASTIEN }),
  issue({ number: 8, title: 'Refonte du moteur de recherche', kind: 'epic', milestoneId: 'm2', commentCount: 1, createdAt: daysAgo(9), author: null }),
  issue({ number: 7, title: 'Mettre en cache les avatars', labels: [PERF], assignee: ALICE, createdAt: daysAgo(12) }),
  issue({ number: 4, title: 'Écrire la doc d’onboarding', status: 'done', labels: [DOCS], createdAt: daysAgo(20), closedAt: daysAgo(3), commentCount: 5, assignee: ALICE }),
  issue({ number: 2, title: 'Le bouton « Cloner » ne copie rien sous Firefox', kind: 'bug', status: 'done', labels: [BUG], createdAt: daysAgo(25), closedAt: minutesAgo(40), author: BASTIEN }),
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
    createdAt: hoursAgo(5),
  }),
  issue({
    number: 1286,
    title: 'Proposer des modèles de tickets par dépôt',
    kind: 'feature',
    labels: [UI, DOCS, PERF, GOOD_FIRST, URGENT, BUG],
    milestoneId: 'm2',
    commentCount: 2,
    createdAt: daysAgo(1),
  }),
  issue({ number: 1285, title: 'Ticketsansespacesdansletitrequidoitquandmêmesetronqueràlafindelaligne', createdAt: daysAgo(2), assignee: FLORIAN }),
];

const MANY: Issue[] = Array.from({ length: 43 }, (_, index) =>
  issue({
    number: 100 - index,
    title: `${TITLE_VERBS[index % TITLE_VERBS.length]} ${TITLE_SUBJECTS[index % TITLE_SUBJECTS.length]}`,
    kind: (['task', 'bug', 'feature'] as const)[index % 3],
    status: index >= 40 ? 'done' : (['todo', 'in_progress', 'in_review'] as const)[index % 3],
    closedAt: index >= 40 ? daysAgo(1) : null,
    labels: index % 4 === 0 ? [BUG] : index % 5 === 0 ? [UI, PERF] : [],
    assignee: index % 3 === 0 ? BASTIEN : null,
    commentCount: index % 4,
    createdAt: hoursAgo(index * 7 + 1),
  }),
);

function fakeIssuesService(issues: Issue[], overrides: Partial<Record<keyof IssuesService, unknown>> = {}) {
  return {
    list: () => of(issues),
    create: () => of(issues[0]),
    close: () => of(issues[0]),
    reopen: () => of(issues[0]),
    assign: () => of(issues[0]),
    ...overrides,
  };
}

const fakeMeService = { id: () => 'u1', username: () => 'alice', email: () => 'alice@example.com', isAdmin: () => false };

function withData(options: { issues?: Issue[]; role?: 'owner' | 'reader'; labels?: Label[]; milestones?: Milestone[]; issuesService?: unknown } = {}) {
  return moduleMetadata({
    providers: [
      { provide: IssuesService, useValue: options.issuesService ?? fakeIssuesService(options.issues ?? MIXED) },
      { provide: LabelsService, useValue: { listForRepository: () => of(options.labels ?? LABELS) } },
      { provide: MilestonesService, useValue: { listForRepository: () => of(options.milestones ?? MILESTONES) } },
      { provide: RepositoryContextService, useValue: fakeRepositoryContextService(options.role ?? 'owner') },
      { provide: MeService, useValue: fakeMeService },
      { provide: GbtToastService, useValue: fakeToast },
    ],
  });
}

const rect = (el: Element) => el.getBoundingClientRect();
const centreY = (el: Element) => rect(el).top + rect(el).height / 2;

/** Layout checks jsdom can't make: no horizontal overflow, aside beside the list from 769px, status icon, title and trailing items on one line. */
function assertPageLayout(canvas: HTMLElement): number {
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

  const rows = Array.from(canvas.querySelectorAll('gbt-list-card ul gbt-list-row'));
  for (const row of rows) {
    const line = centreY(row.querySelector('.gbt-list-row__title > a')!);
    const icon = row.querySelector('.gbt-list-row__leading gbt-icon');
    if (!icon || Math.abs(centreY(icon) - line) > 1) throw new Error('status icon off the title line');
    const trailing = Array.from(row.querySelectorAll('.issue-list__trailing > *'));
    // A trailing column wrapping under the title (phone, long assignee name) is fine.
    const onTitleLine = trailing.filter((item) => rect(item).top < rect(row.querySelector('.gbt-list-row__meta') ?? row).top);
    for (const item of onTitleLine) {
      if (Math.abs(centreY(item) - line) > 1) throw new Error('trailing item off the title line');
    }
    const link = row.querySelector('.gbt-list-row__title > a')!;
    if (rect(link).right > rect(row.querySelector('.gbt-list-row__title')!).right + 0.5) throw new Error('title link spills out of its column');
    if (rect(row).right > rect(main).right + 0.5) throw new Error('row wider than the main column');
  }
  return rows.length;
}

async function expectPageLayout({ canvasElement }: { canvasElement: HTMLElement }) {
  await waitFor(() => assertPageLayout(canvasElement), { timeout: 3000 });
}

async function openCreateModal(canvasElement: HTMLElement): Promise<HTMLElement> {
  const button = await waitFor(() => {
    const found = Array.from(canvasElement.querySelectorAll<HTMLButtonElement>('.gbt-page-header__actions button')).find((b) => b.textContent?.includes('Nouveau ticket'));
    if (!found) throw new Error('"Nouveau ticket" not rendered yet');
    return found;
  });
  button.click();
  return waitFor(() => {
    const dialog = canvasElement.ownerDocument.querySelector<HTMLElement>('[role="dialog"]');
    if (!dialog) throw new Error('dialog not open yet');
    return dialog;
  });
}

const meta: Meta<IssueList> = {
  title: 'Issues/IssueList',
  component: IssueList,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  args: { repositoryId: 'repo-1', path: ['alice', 'ferrisgit'] },
  decorators: [applicationConfig({ providers: [provideRouter([], withDisabledInitialNavigation()), provideFerrisgitIcons()] }), inShellContentArea],
};

export default meta;
type Story = StoryObj<IssueList>;

export const Populated: Story = {
  decorators: [withData()],
  play: async (context) => {
    await expectPageLayout(context);
    await expect(context.canvasElement.querySelector('gbt-pagination'), 'no pager for 5 issues').toBeNull();
    const numbers = Array.from(context.canvasElement.querySelectorAll('gbt-list-card ul .issue-list__number'), (el) => el.textContent?.trim());
    await expect(numbers, 'newest first').toEqual(['#12', '#11', '#9', '#8', '#7']);
  },
};

export const ClosedTab: Story = {
  decorators: [withData()],
  play: async (context) => {
    const closedTab = await waitFor(() => {
      const found = context.canvasElement.querySelectorAll<HTMLButtonElement>('.issue-list__tabs [role="radio"]')[1];
      if (!found) throw new Error('tabs not rendered yet');
      return found;
    });
    closedTab.click();
    await waitFor(() => expect(context.canvasElement.querySelectorAll('gbt-list-card ul > li').length).toBe(2));
    await expectPageLayout(context);
  },
};

export const LongTitlesAndManyLabels: Story = {
  decorators: [withData({ issues: LONG })],
  play: expectPageLayout,
};

export const ManyIssues: Story = {
  decorators: [withData({ issues: MANY })],
  play: async (context) => {
    await expectPageLayout(context);
    await expect(context.canvasElement.querySelectorAll('gbt-list-card ul > li').length).toBe(25);
    await expect(context.canvasElement.querySelector('gbt-pagination')).not.toBeNull();
  },
};

export const NoMatchingFilters: Story = {
  decorators: [withData()],
  play: async (context) => {
    const input = await waitFor(() => {
      const found = context.canvasElement.querySelector<HTMLInputElement>('.issue-list__search input');
      if (!found) throw new Error('search field not rendered yet');
      return found;
    });
    input.value = 'introuvable';
    input.dispatchEvent(new Event('input'));
    await waitFor(() => expect(context.canvasElement.querySelector('[list-card-message]')?.textContent).toContain('ne correspond à ces filtres'));
    await waitFor(() => expect(context.canvasElement.querySelector('.issue-list__reset')).not.toBeNull());
    await expectPageLayout(context);
  },
};

export const ReadOnly: Story = {
  decorators: [withData({ role: 'reader' })],
  play: async (context) => {
    await expectPageLayout(context);
    await expect(context.canvasElement.querySelector('.gbt-menu__trigger')).toBeNull();
    await expect(context.canvasElement.querySelector('.gbt-button--primary')).toBeNull();
  },
};

export const Empty: Story = {
  decorators: [withData({ issues: [], labels: [], milestones: [] })],
};

export const EmptyReadOnly: Story = {
  decorators: [withData({ issues: [], labels: [], milestones: [], role: 'reader' })],
};

export const Loading: Story = {
  decorators: [withData({ issuesService: fakeIssuesService([], { list: () => NEVER }) })],
};

export const LoadError: Story = {
  decorators: [withData({ issuesService: fakeIssuesService([], { list: () => throwError(() => new Error('500')) }) })],
};

export const CreateModalWithValidationError: Story = {
  decorators: [withData()],
  play: async ({ canvasElement }) => {
    const dialog = await openCreateModal(canvasElement);
    const title = await waitFor(() => {
      const input = dialog.querySelector<HTMLInputElement>('.issue-list__create-title input');
      if (!input) throw new Error('title field not rendered yet');
      return input;
    });
    // A real focus/blur pair only fires when the story's window has focus, so dispatch the blur ourselves.
    title.dispatchEvent(new FocusEvent('blur'));
    await waitFor(() => expect(dialog.querySelector('.gbt-input__error')?.textContent).toContain('Le titre est requis'));
    const submit = Array.from(dialog.querySelectorAll('button')).find((b) => b.textContent?.includes('Créer le ticket'));
    await expect(submit?.disabled).toBe(true);
  },
};
