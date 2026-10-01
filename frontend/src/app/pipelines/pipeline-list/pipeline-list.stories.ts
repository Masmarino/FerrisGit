import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { expect, waitFor } from 'storybook/test';
import { NEVER, of, throwError } from 'rxjs';
import { provideRouter, withDisabledInitialNavigation } from '@angular/router';
import { PipelineList } from './pipeline-list';
import { PipelineSummary, PipelinesService } from '../pipelines.service';
import { GbtToastService } from '@masmarino/gabarit';
import { provideFerrisgitIcons } from '../../shared/register-icons';
import { daysAgo, hoursAgo, inShellContentArea, minutesAgo } from '../../shared/layout/page-story-helpers';

const ALICE = { id: 'u1', username: 'alice' };
const BASTIEN = { id: 'u2', username: 'bastien' };
const FLORIAN = { id: 'u3', username: 'florian' };

const secondsAfter = (iso: string, seconds: number) => new Date(Date.parse(iso) + seconds * 1000).toISOString();

let nextId = 0;
function pipeline(fields: Partial<PipelineSummary> & { took?: number } = {}): PipelineSummary {
  const n = ++nextId;
  const hex = (seed: number, length: number) => Array.from({ length }, (_, i) => ((seed * 7 + i * 13) % 16).toString(16)).join('');
  const { took, ...rest } = fields;
  const createdAt = rest.createdAt ?? hoursAgo(n);
  return {
    id: `${hex(n, 8)}-${hex(n + 1, 4)}-4${hex(n + 2, 3)}-9${hex(n + 3, 3)}-${hex(n + 4, 12)}`,
    commitSha: hex(n + 5, 40),
    status: 'success',
    triggeredBy: ALICE,
    commitMessage: 'Paginer la liste des tickets',
    ...rest,
    createdAt,
    finishedAt: rest.finishedAt !== undefined ? rest.finishedAt : took !== undefined ? secondsAfter(createdAt, took) : null,
  };
}

const MIXED: PipelineSummary[] = [
  pipeline({ status: 'running', commitMessage: 'Ajouter la connexion via SSO', createdAt: minutesAgo(3), triggeredBy: BASTIEN }),
  pipeline({ status: 'pending', commitMessage: 'Documenter l’enregistrement des runners', createdAt: minutesAgo(1), triggeredBy: FLORIAN }),
  pipeline({ status: 'failed', commitMessage: 'Corriger la pagination quand on filtre par label', createdAt: hoursAgo(2), took: 212 }),
  pipeline({ status: 'success', commitMessage: 'Mettre en cache les avatars', createdAt: hoursAgo(5), took: 95, triggeredBy: BASTIEN }),
  pipeline({ status: 'canceled', commitMessage: 'Nettoyer le code mort', createdAt: daysAgo(1), took: 38, triggeredBy: null }),
  pipeline({ status: 'success', commitMessage: null, createdAt: daysAgo(3), took: 4_020 }),
  pipeline({ status: 'success', commitMessage: 'Préparer la version 1.0', createdAt: daysAgo(40), finishedAt: null, triggeredBy: FLORIAN }),
];

const LONG: PipelineSummary[] = [
  pipeline({
    status: 'failed',
    commitMessage:
      'Quand on renomme une branche protégée depuis l’interface, les règles de protection doivent suivre le nouveau nom au lieu de rester attachées à l’ancien, y compris pour les pushs forcés',
    createdAt: minutesAgo(12),
    took: 3_725,
    triggeredBy: { id: 'u9', username: 'Maximilien de La Tour d’Auvergne' },
  }),
  pipeline({ status: 'running', commitMessage: 'Unmessagedecommitsansespacesquidoitquandmêmesetronqueràlafindelalignesansdéborderdelacarte', createdAt: minutesAgo(48) }),
  pipeline({ status: 'success', commitMessage: 'Proposer des modèles de demandes de fusion par dépôt', createdAt: hoursAgo(3), took: 61 }),
];

const verbs = ['Corriger', 'Ajouter', 'Documenter', 'Tester', 'Simplifier'];
const subjects = ['la page des pipelines', 'les webhooks', 'la recherche', 'le wiki', 'les jetons d’API', 'la vue kanban'];
const statuses: PipelineSummary['status'][] = ['success', 'success', 'failed', 'success', 'canceled', 'success', 'failed'];
/** Like the API, only the 50 newest carry their commit message. */
const MANY: PipelineSummary[] = Array.from({ length: 60 }, (_, index) =>
  pipeline({
    status: index < 2 ? 'running' : statuses[index % statuses.length],
    commitMessage: index < 50 ? `${verbs[index % verbs.length]} ${subjects[index % subjects.length]}` : null,
    triggeredBy: [ALICE, BASTIEN, FLORIAN][index % 3],
    createdAt: hoursAgo(index * 5 + 0.1),
    took: index < 2 ? undefined : 40 + ((index * 37) % 400),
  }),
);

function withData(options: { list?: PipelineSummary[]; service?: unknown } = {}) {
  return moduleMetadata({
    providers: [
      { provide: PipelinesService, useValue: options.service ?? { listForRepository: () => of(options.list ?? MIXED) } },
      { provide: GbtToastService, useValue: { show: () => {}, dismiss: () => {} } },
    ],
  });
}

const rect = (el: Element) => el.getBoundingClientRect();
const centreY = (el: Element) => rect(el).top + rect(el).height / 2;

function assertPageLayout(canvas: HTMLElement): number {
  const main = canvas.querySelector('.gbt-page-layout__main');
  const card = canvas.querySelector('gbt-list-card');
  if (!main || !card) throw new Error('page layout not rendered yet');

  const doc = canvas.ownerDocument.documentElement;
  if (doc.scrollWidth > doc.clientWidth + 1) throw new Error(`horizontal overflow: ${doc.scrollWidth}px of content in ${doc.clientWidth}px`);
  if (Math.abs(rect(card).width - rect(main).width) > 1) throw new Error('the card should fill the main column');

  const header = canvas.querySelector('gbt-list-card .gbt-list-card__header');
  if (header) {
    for (const part of Array.from(header.querySelectorAll('.pipeline-list__tabs .gbt-segmented-control, .pipeline-list__search input, .pipeline-list__order .gbt-select__trigger'))) {
      if (rect(part).right > rect(header).right + 0.5 || rect(part).left < rect(header).left - 0.5) throw new Error(`${part.className} spills out of the header`);
    }
    const input = header.querySelector('.pipeline-list__search input');
    const select = header.querySelector('.pipeline-list__order .gbt-select__trigger');
    if (input && select && Math.abs(rect(input).height - rect(select).height) > 0.5) throw new Error('search and order heights differ');
  }

  const rows = Array.from(canvas.querySelectorAll('.pipeline-list__items gbt-list-row'));
  for (const row of rows) {
    const link = row.querySelector('.gbt-list-row__title > a')!;
    const line = centreY(link);
    const icon = row.querySelector('.gbt-list-row__leading gbt-icon');
    if (!icon || Math.abs(centreY(icon) - line) > 1) throw new Error('status icon off the title line');
    if (rect(link).right > rect(row.querySelector('.gbt-list-row__title')!).right + 0.5) throw new Error('title link spills out of its column');
    const sha = row.querySelector('.pipeline-list__sha')!;
    if (rect(sha).right > rect(row.querySelector('.gbt-list-row__meta')!).right + 0.5) throw new Error('SHA chip spills out of the meta line');
    // In a phone-width card the badge is visually hidden; the title takes the width.
    const badge = row.querySelector('.gbt-list-row__trailing fg-status-badge')!;
    const trailing = row.querySelector('.gbt-list-row__trailing')!;
    if (rect(trailing).width > 1) {
      if (rect(badge).right > rect(row).right + 0.5) throw new Error('status badge spills out of the row');
      if (Math.abs(centreY(badge) - line) > 1) throw new Error('status badge off the title line');
    } else if (Math.abs(rect(row.querySelector('.gbt-list-row__main')!).right - (rect(row).right - 16)) > 1) {
      throw new Error('the title should take the whole width when the badge is hidden');
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
    const found = canvasElement.querySelectorAll<HTMLButtonElement>('.pipeline-list__tabs [role="radio"]')[index];
    if (!found) throw new Error('tabs not rendered yet');
    return found;
  });
  tab.click();
  await waitFor(() => expect(canvasElement.querySelectorAll('.pipeline-list__items > li').length).toBe(expectedRows));
}

const meta: Meta<PipelineList> = {
  title: 'Pipelines/PipelineList',
  component: PipelineList,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  args: { repositoryId: 'repo-1', path: ['alice', 'ferrisgit'] },
  decorators: [applicationConfig({ providers: [provideRouter([], withDisabledInitialNavigation()), provideFerrisgitIcons()] }), inShellContentArea],
};

export default meta;
type Story = StoryObj<PipelineList>;

export const Populated: Story = {
  decorators: [withData()],
  play: async (context) => {
    await expectPageLayout(context);
    const canvas = context.canvasElement;
    const tabs = Array.from(canvas.querySelectorAll('.pipeline-list__tabs [role="radio"]'), (el) => el.textContent?.trim());
    await expect(tabs).toEqual(['Tous (7)', 'En cours (2)', 'Réussis (3)', 'Échoués (1)']);
    await expect(canvas.querySelectorAll('.pipeline-list__items > li').length).toBe(7);
    await expect(canvas.querySelector('gbt-pagination'), 'no pager for 7 pipelines').toBeNull();
  },
};

export const InProgressTab: Story = {
  decorators: [withData()],
  play: async (context) => {
    await clickTab(context.canvasElement, 1, 2);
    await expectPageLayout(context);
  },
};

export const FailedTab: Story = {
  decorators: [withData()],
  play: async (context) => {
    await clickTab(context.canvasElement, 3, 1);
    await expectPageLayout(context);
  },
};

export const LongCommitMessages: Story = {
  decorators: [withData({ list: LONG })],
  play: async (context) => {
    await expectPageLayout(context);
    const link = context.canvasElement.querySelector<HTMLElement>('.pipeline-list__items .gbt-list-row__title > a')!;
    await expect(link.scrollWidth, 'the long message is cut with an ellipsis').toBeGreaterThan(link.clientWidth);
  },
};

export const ManyPipelines: Story = {
  decorators: [withData({ list: MANY })],
  play: async (context) => {
    await expectPageLayout(context);
    await expect(context.canvasElement.querySelectorAll('.pipeline-list__items > li').length).toBe(25);
    await expect(context.canvasElement.querySelector('gbt-pagination')).not.toBeNull();
  },
};

export const NoMatchingSearch: Story = {
  decorators: [withData()],
  play: async (context) => {
    const input = await waitFor(() => {
      const found = context.canvasElement.querySelector<HTMLInputElement>('.pipeline-list__search input');
      if (!found) throw new Error('search field not rendered yet');
      return found;
    });
    input.value = 'introuvable';
    input.dispatchEvent(new Event('input'));
    await waitFor(() => expect(context.canvasElement.querySelector('[list-card-message]')?.textContent).toContain('ne correspond à cette recherche'));
    await expectPageLayout(context);
  },
};

export const Empty: Story = {
  decorators: [withData({ list: [] })],
};

export const Loading: Story = {
  decorators: [withData({ service: { listForRepository: () => NEVER } })],
};

export const LoadError: Story = {
  decorators: [withData({ service: { listForRepository: () => throwError(() => new Error('500')) } })],
};
