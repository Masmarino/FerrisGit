import type { Meta, StoryObj } from '@storybook/angular-vite';
import { moduleMetadata } from '@storybook/angular-vite';
import { expect, waitFor } from 'storybook/test';
import { NEVER, of, throwError } from 'rxjs';
import { MergeRequestList } from './merge-request-list';
import { BranchInfo, MergeRequestSummary, MergeRequestsService } from '../merge-requests.service';
import { Label, LabelsService } from '../../labels/labels.service';
import { Milestone, MilestonesService } from '../../milestones/milestones.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { withRouterAndIcons } from '../../repositories/repository-story-fixtures';
import { GbtToastService } from '@masmarino/gabarit';
import { daysAgo, hoursAgo, inShellContentArea, minutesAgo } from '../../shared/layout/page-story-helpers';
import { fakeToast } from '../../shared/layout/settings-story-helpers';
import { ALICE, fakeRepositoryContext, label, mergeRequestFixture, milestone } from '../merge-request-fixtures';

const LABELS: Label[] = [
  label('l-bug', 'bug', '#dc2626'),
  label('l-urgent', 'urgent', '#f97316'),
  label('l-ui', 'interface', '#6366f1'),
  label('l-docs', 'documentation', '#0ea5e9'),
  label('l-perf', 'performance', '#16a34a'),
  label('l-security', 'sécurité', '#a855f7'),
];
const [BUG, URGENT, UI, DOCS, PERF, SECURITY] = LABELS;

const MILESTONES: Milestone[] = [
  milestone({ id: 'm1', title: 'v1.0', description: 'Première version stable', dueDate: '2026-11-01' }),
  milestone({ id: 'm2', title: 'v1.1', createdAt: '2026-02-01T00:00:00Z' }),
];

const BRANCHES: BranchInfo[] = [
  { name: 'main', tipSha: 'aaa1111111', isDefault: true },
  { name: 'feature/sso-login', tipSha: 'bbb2222222', isDefault: false },
  { name: 'fix/pagination-labels', tipSha: 'ccc3333333', isDefault: false },
  { name: 'release/1.0', tipSha: 'ddd4444444', isDefault: false },
];

const BASTIEN = { id: 'u2', username: 'bastien' };
const FLORIAN = { id: 'u3', username: 'florian' };

const mergeRequest = (fields: Parameters<typeof mergeRequestFixture>[0]) => mergeRequestFixture({ createdAt: daysAgo(2), ...fields });

const MIXED: MergeRequestSummary[] = [
  mergeRequest({ title: 'Ajoute la connexion via SSO', sourceBranch: 'feature/sso-login', labels: [SECURITY, UI], milestoneId: 'm1', commentCount: 3, createdAt: hoursAgo(2) }),
  mergeRequest({ title: 'Corrige la pagination quand on filtre par label', sourceBranch: 'fix/pagination-labels', labels: [BUG, URGENT], commentCount: 12, createdAt: daysAgo(1), author: BASTIEN }),
  mergeRequest({ title: 'Met en cache les avatars', sourceBranch: 'perf/avatar-cache', labels: [PERF], milestoneId: 'm2', createdAt: daysAgo(4), author: FLORIAN }),
  mergeRequest({ title: 'Prépare la version 1.0', sourceBranch: 'main', targetBranch: 'release/1.0', milestoneId: 'm1', commentCount: 1, createdAt: daysAgo(9), author: null }),
  mergeRequest({ title: 'Documente l’enregistrement des runners', sourceBranch: 'docs/runners', status: 'merged', labels: [DOCS], createdAt: daysAgo(6), closedAt: hoursAgo(1), commentCount: 5, author: BASTIEN }),
  mergeRequest({ title: 'Corrige une faute de frappe dans le README', sourceBranch: 'fix/typo', status: 'merged', createdAt: daysAgo(12), closedAt: daysAgo(11) }),
  mergeRequest({ title: 'Nettoyage du code mort', sourceBranch: 'chore/cleanup', status: 'closed', createdAt: daysAgo(20), closedAt: minutesAgo(40), commentCount: 2, author: FLORIAN }),
];

const LONG: MergeRequestSummary[] = [
  mergeRequest({
    title: 'Quand on renomme une branche protégée depuis l’interface, les règles de protection doivent suivre le nouveau nom au lieu de rester attachées à l’ancien',
    sourceBranch: 'fix/protected-branch-rules-follow-renamed-branches-across-the-ui-and-api',
    targetBranch: 'maintenance/1.x-long-term-support',
    labels: [BUG, URGENT, SECURITY, UI, DOCS, PERF],
    milestoneId: 'm1',
    commentCount: 128,
    createdAt: hoursAgo(5),
    author: { id: 'u9', username: 'Maximilien de La Tour d’Auvergne' },
  }),
  mergeRequest({ title: 'Demandesansespacesdansletitrequidoitquandmêmesetronqueràlafindelaligne', sourceBranch: 'feature/x', createdAt: daysAgo(1) }),
  mergeRequest({ title: 'Proposer des modèles de demandes de fusion par dépôt', sourceBranch: 'feature/templates', labels: [UI, DOCS], milestoneId: 'm2', commentCount: 2, createdAt: daysAgo(2) }),
];

const verbs = ['Corrige', 'Ajoute', 'Documente', 'Teste', 'Simplifie'];
const subjects = ['la page des pipelines', 'les webhooks', 'la recherche', 'le wiki', 'les jetons d’API', 'la vue kanban'];
const MANY: MergeRequestSummary[] = Array.from({ length: 43 }, (_, index) =>
  mergeRequest({
    title: `${verbs[index % verbs.length]} ${subjects[index % subjects.length]}`,
    sourceBranch: `feature/lot-${100 - index}`,
    status: index >= 40 ? 'merged' : 'open',
    closedAt: index >= 40 ? daysAgo(1) : null,
    labels: index % 4 === 0 ? [BUG] : index % 5 === 0 ? [UI, PERF] : [],
    author: [ALICE, BASTIEN, FLORIAN][index % 3],
    commentCount: index % 4,
    createdAt: hoursAgo(index * 7 + 1),
  }),
);

function fakeMergeRequestsService(list: MergeRequestSummary[], overrides: Partial<Record<keyof MergeRequestsService, unknown>> = {}) {
  return {
    listBranches: () => of(BRANCHES),
    listForRepository: () => of(list),
    create: () => of(list[0]),
    close: () => of(undefined),
    merge: () => of({ ...list[0], outcome: 'merged' as const }),
    ...overrides,
  };
}

function withData(options: { list?: MergeRequestSummary[]; role?: 'owner' | 'reader' | 'contributor'; labels?: Label[]; milestones?: Milestone[]; service?: unknown } = {}) {
  return moduleMetadata({
    providers: [
      { provide: MergeRequestsService, useValue: options.service ?? fakeMergeRequestsService(options.list ?? MIXED) },
      { provide: LabelsService, useValue: { listForRepository: () => of(options.labels ?? LABELS) } },
      { provide: MilestonesService, useValue: { listForRepository: () => of(options.milestones ?? MILESTONES) } },
      { provide: RepositoryContextService, useValue: fakeRepositoryContext(options.role ?? 'owner', ['alice', 'ferrisgit']) },
      { provide: GbtToastService, useValue: fakeToast },
    ],
  });
}

const rect = (el: Element) => el.getBoundingClientRect();
const centreY = (el: Element) => rect(el).top + rect(el).height / 2;

/** Layout checks jsdom can't make: no horizontal overflow, aside beside the list from 769px, tabs in the header, row items on the title line. */
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

  const header = canvas.querySelector('gbt-list-card .gbt-list-card__header');
  const tabs = canvas.querySelector('.merge-request-list__tabs .gbt-segmented-control');
  if (header && tabs && rect(tabs).right > rect(header).right + 0.5) throw new Error('the state tabs spill out of the card');

  const rows = Array.from(canvas.querySelectorAll('gbt-list-card ul gbt-list-row'));
  for (const row of rows) {
    const title = row.querySelector('.gbt-list-row__title')!;
    const line = centreY(row.querySelector('.gbt-list-row__title > a')!);
    const icon = row.querySelector('.gbt-list-row__leading gbt-icon');
    if (!icon || Math.abs(centreY(icon) - line) > 1) throw new Error('status icon off the title line');
    const menu = row.querySelector('.merge-request-list__menu');
    if (menu && rect(menu).top < rect(row.querySelector('.gbt-list-row__meta')!).top && Math.abs(centreY(menu) - line) > 1) throw new Error('kebab off the title line');
    const link = row.querySelector('.gbt-list-row__title > a')!;
    if (rect(link).right > rect(title).right + 0.5) throw new Error('title link spills out of its column');
    const meta = row.querySelector('.gbt-list-row__meta')!;
    for (const chip of Array.from(row.querySelectorAll('.merge-request-list__branches gbt-badge'))) {
      if (rect(chip).right > rect(meta).right + 0.5) throw new Error('branch chip spills out of the meta line');
    }
    if (rect(row).right > rect(main).right + 0.5) throw new Error('row wider than the main column');
  }
  return rows.length;
}

async function expectPageLayout({ canvasElement }: { canvasElement: HTMLElement }) {
  await waitFor(() => assertPageLayout(canvasElement), { timeout: 3000 });
}

async function clickTab(canvasElement: HTMLElement, index: number, expectedRows: number) {
  const tab = await waitFor(() => {
    const found = canvasElement.querySelectorAll<HTMLButtonElement>('.merge-request-list__tabs [role="radio"]')[index];
    if (!found) throw new Error('tabs not rendered yet');
    return found;
  });
  tab.click();
  await waitFor(() => expect(canvasElement.querySelectorAll('gbt-list-card ul > li').length).toBe(expectedRows));
}

async function openCreateModal(canvasElement: HTMLElement): Promise<HTMLElement> {
  const button = await waitFor(() => {
    const found = Array.from(canvasElement.querySelectorAll<HTMLButtonElement>('.gbt-page-header__actions button')).find((b) => b.textContent?.includes('Nouvelle demande de fusion'));
    if (!found) throw new Error('"Nouvelle demande de fusion" not rendered yet');
    return found;
  });
  button.click();
  return waitFor(() => {
    const dialog = canvasElement.ownerDocument.querySelector<HTMLElement>('[role="dialog"]');
    if (!dialog) throw new Error('dialog not open yet');
    return dialog;
  });
}

async function pickOption(dialog: HTMLElement, selectClass: string, optionLabel: string) {
  dialog.querySelector<HTMLButtonElement>(`${selectClass} .gbt-select__trigger`)!.click();
  const option = await waitFor(() => {
    const found = Array.from(dialog.ownerDocument.querySelectorAll<HTMLElement>('.gbt-select__option')).find((el) => el.textContent?.trim() === optionLabel);
    if (!found) throw new Error(`option ${optionLabel} not rendered yet`);
    return found;
  });
  option.click();
}

const meta: Meta<MergeRequestList> = {
  title: 'MergeRequests/MergeRequestList',
  component: MergeRequestList,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  args: { repositoryId: 'repo-1', path: ['alice', 'ferrisgit'] },
  decorators: [withRouterAndIcons, inShellContentArea],
};

export default meta;
type Story = StoryObj<MergeRequestList>;

export const Populated: Story = {
  decorators: [withData()],
  play: async (context) => {
    await expectPageLayout(context);
    const canvas = context.canvasElement;
    await expect(canvas.querySelector('gbt-pagination'), 'no pager for 4 merge requests').toBeNull();
    const tabs = Array.from(canvas.querySelectorAll('.merge-request-list__tabs [role="radio"]'), (el) => el.textContent?.trim());
    await expect(tabs).toEqual(['Ouvertes (4)', 'Fusionnées (2)', 'Fermées (1)']);
    const titles = Array.from(canvas.querySelectorAll('gbt-list-card ul .gbt-list-row__title > a'), (el) => el.textContent?.trim());
    await expect(titles[0], 'newest first').toBe('Ajoute la connexion via SSO');
    await expect(canvas.querySelectorAll('gbt-list-card ul .gbt-menu__trigger').length, 'a kebab on every open row').toBe(4);
  },
};

export const MergedTab: Story = {
  decorators: [withData()],
  play: async (context) => {
    await clickTab(context.canvasElement, 1, 2);
    await expectPageLayout(context);
    await expect(context.canvasElement.querySelector('gbt-list-card ul .gbt-menu__trigger')).toBeNull();
  },
};

export const ClosedTab: Story = {
  decorators: [withData()],
  play: async (context) => {
    await clickTab(context.canvasElement, 2, 1);
    await expectPageLayout(context);
  },
};

export const LongTitlesAndBranches: Story = {
  decorators: [withData({ list: LONG })],
  play: expectPageLayout,
};

export const ManyMergeRequests: Story = {
  decorators: [withData({ list: MANY })],
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
      const found = context.canvasElement.querySelector<HTMLInputElement>('.merge-request-list__search input');
      if (!found) throw new Error('search field not rendered yet');
      return found;
    });
    input.value = 'introuvable';
    input.dispatchEvent(new Event('input'));
    await waitFor(() => expect(context.canvasElement.querySelector('[list-card-message]')?.textContent).toContain('ne correspond à ces filtres'));
    await waitFor(() => expect(context.canvasElement.querySelector('.merge-request-list__reset')).not.toBeNull());
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

/** A Contributor can close a merge request from the row menu but not merge it. */
export const ContributorRowMenu: Story = {
  decorators: [withData({ role: 'contributor' })],
  play: async (context) => {
    await expectPageLayout(context);
    const trigger = await waitFor(() => {
      const found = context.canvasElement.querySelector<HTMLButtonElement>('.gbt-menu__trigger');
      if (!found) throw new Error('row menu not rendered yet');
      return found;
    });
    trigger.click();
    await waitFor(() => {
      const items = Array.from(context.canvasElement.ownerDocument.querySelectorAll('[role="menuitem"]')).map((item) => item.textContent?.trim());
      expect(items).toEqual(['Fermer']);
    });
  },
};

export const Empty: Story = {
  decorators: [withData({ list: [], labels: [], milestones: [] })],
};

export const EmptyReadOnly: Story = {
  decorators: [withData({ list: [], labels: [], milestones: [], role: 'reader' })],
};

export const Loading: Story = {
  decorators: [withData({ service: fakeMergeRequestsService([], { listForRepository: () => NEVER }) })],
};

export const LoadError: Story = {
  decorators: [withData({ service: fakeMergeRequestsService([], { listForRepository: () => throwError(() => new Error('500')) }) })],
};

export const CreateModalWithBranchError: Story = {
  decorators: [withData()],
  play: async ({ canvasElement }) => {
    const dialog = await openCreateModal(canvasElement);
    await waitFor(() => expect(dialog.querySelector('.merge-request-list__create-target .gbt-select__trigger')?.textContent?.trim()).toBe('main'));
    await pickOption(dialog, '.merge-request-list__create-source', 'main');
    await waitFor(() => expect(dialog.querySelector('.merge-request-list__create-target .gbt-select__error')?.textContent).toContain('Identique à la branche source'));
    // A real blur only fires when the story's window has focus, so dispatch it by hand.
    dialog.querySelector<HTMLInputElement>('.merge-request-list__create-title input')!.dispatchEvent(new FocusEvent('blur'));
    await waitFor(() => expect(dialog.querySelector('.gbt-input__error')?.textContent).toContain('Le titre est requis'));
    const submit = Array.from(dialog.querySelectorAll('button')).find((b) => b.textContent?.includes('Créer la demande de fusion'));
    await expect(submit?.disabled).toBe(true);
  },
};

export const CreateModalFilled: Story = {
  decorators: [withData()],
  play: async ({ canvasElement }) => {
    const dialog = await openCreateModal(canvasElement);
    await pickOption(dialog, '.merge-request-list__create-source', 'feature/sso-login');
    const title = dialog.querySelector<HTMLInputElement>('.merge-request-list__create-title input')!;
    title.value = 'Ajoute la connexion via SSO';
    title.dispatchEvent(new Event('input'));
    const description = dialog.querySelector<HTMLTextAreaElement>('.merge-request-list__create-description textarea')!;
    description.value = 'Branche le fournisseur OIDC sur la page de connexion. Testé avec Keycloak en local.';
    description.dispatchEvent(new Event('input'));
    await waitFor(() => {
      const submit = Array.from(dialog.querySelectorAll('button')).find((b) => b.textContent?.includes('Créer la demande de fusion'));
      return expect(submit?.disabled).toBe(false);
    });
  },
};
