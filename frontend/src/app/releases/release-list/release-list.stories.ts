import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { expect, waitFor } from 'storybook/test';
import { NEVER, of, throwError } from 'rxjs';
import { provideRouter, withDisabledInitialNavigation } from '@angular/router';
import { ReleaseList } from './release-list';
import { ReleaseSummary, ReleasesService, TagSummary } from '../releases.service';
import { BranchInfo, MergeRequestsService } from '../../merge-requests/merge-requests.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { GbtToastService } from '@masmarino/gabarit';
import { provideFerrisgitIcons } from '../../shared/register-icons';
import { daysAgo, hoursAgo, inShellContentArea } from '../../shared/layout/page-story-helpers';

const ALICE = { id: 'u1', username: 'alice' };
const BASTIEN = { id: 'u2', username: 'bastien' };

let nextId = 0;
function release(fields: Partial<ReleaseSummary> & Pick<ReleaseSummary, 'tagName' | 'title'>): ReleaseSummary {
  const createdAt = fields.createdAt ?? daysAgo(2);
  return {
    id: `release-${++nextId}`,
    draft: false,
    prerelease: false,
    authorId: ALICE.id,
    author: ALICE,
    notesExcerpt: '',
    assetCount: 0,
    createdAt,
    publishedAt: fields.draft ? null : createdAt,
    ...fields,
  };
}

const MIXED: ReleaseSummary[] = [
  release({
    tagName: 'v2.1.0',
    title: 'v2.1.0 — Améliorations diverses',
    notesExcerpt:
      '## Nouveautés\n\n- **Pipelines** : les journaux des jobs se mettent à jour en direct\n- **Wiki** : table des matières et historique des pages\n- Recherche plus rapide dans les gros dépôts\n\n## Corrections\n\n- Le bouton « Cloner » copie enfin l’URL sous Firefox',
    assetCount: 3,
    createdAt: hoursAgo(5),
  }),
  release({
    tagName: 'v2.1.0-rc.2',
    title: 'Release candidate 2',
    prerelease: true,
    notesExcerpt: 'Deuxième candidate : corrige la migration des webhooks signalée sur la rc.1.',
    assetCount: 1,
    createdAt: daysAgo(3),
    author: BASTIEN,
  }),
  release({ tagName: 'v2.2.0', title: 'v2.2.0 (à venir)', draft: true, notesExcerpt: '- [ ] Notes à rédiger', createdAt: daysAgo(1) }),
  release({ tagName: 'v2.0.0', title: 'FerrisGit 2.0', notesExcerpt: 'Une version majeure : nouvelle interface, pipelines, releases et wiki.', assetCount: 5, createdAt: daysAgo(40), author: null }),
  release({ tagName: 'v1.4.3', title: 'Correctif de sécurité', createdAt: daysAgo(75) }),
];

const LONG: ReleaseSummary[] = [
  release({
    tagName: 'v2024.11.28-nightly+build.4187-linux-x86_64',
    title: 'Une version nocturne au titre très long, publiée automatiquement par la CI, qui ne doit jamais faire déborder la carte',
    prerelease: true,
    notesExcerpt: 'Pasd’espacesdanscetextequidoitquandmêmesereplierproprementàlalignesansfairedéborderlacartedelarelease '.repeat(3),
    assetCount: 12,
    author: { id: 'u9', username: 'Maximilien de La Tour d’Auvergne' },
    createdAt: hoursAgo(2),
  }),
  ...MIXED.slice(0, 2),
];

const MANY: ReleaseSummary[] = Array.from({ length: 23 }, (_, index) =>
  release({
    tagName: `v1.${22 - index}.0`,
    title: `Version 1.${22 - index}`,
    notesExcerpt: index % 3 === 0 ? '' : 'Corrections et petites améliorations.',
    assetCount: index % 4,
    createdAt: daysAgo(index * 7 + 1),
  }),
);

const TAGS: TagSummary[] = [{ name: 'v2.1.0', targetSha: 'd4e5f60718293a4b5c6d7e8f9012345678901234' }];
const BRANCHES: BranchInfo[] = [{ name: 'main', tipSha: 'a1b2c3d4e5f60718293a4b5c6d7e8f9012345678', isDefault: true }];

function withData(options: { releases?: ReleaseSummary[]; role?: 'owner' | 'maintainer' | 'reader'; list?: () => unknown } = {}) {
  const releases = options.releases ?? MIXED;
  return moduleMetadata({
    providers: [
      {
        provide: ReleasesService,
        useValue: { list: options.list ?? (() => of(releases)), listTags: () => of(TAGS), create: () => of(releases[0]), deleteTag: () => of(undefined) },
      },
      { provide: MergeRequestsService, useValue: { listBranches: () => of(BRANCHES) } },
      {
        provide: RepositoryContextService,
        useValue: { current: () => ({ repositoryId: 'repo-1', path: ['alice', 'ferrisgit'], role: options.role ?? 'maintainer', ancestors: [], groupId: null }) },
      },
      { provide: GbtToastService, useValue: { show: () => {}, dismiss: () => {} } },
    ],
  });
}

const rect = (el: Element) => el.getBoundingClientRect();
const centreY = (el: Element) => rect(el).top + rect(el).height / 2;

/** Layout checks jsdom cannot make: no horizontal overflow, aside beside the list from 769px, tag chip, title and status on one line in a wide card. */
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

  const cards = Array.from(canvas.querySelectorAll<HTMLElement>('.release-card'));
  if (cards.length === 0) throw new Error('no release card rendered yet');
  for (const card of cards) {
    const tag = card.querySelector('.release-card__tag')!;
    const title = card.querySelector('.release-card__title')!;
    const status = card.querySelector('.release-card__status')!;
    if (rect(card).width >= 520) {
      const line = centreY(title);
      if (Math.abs(centreY(tag) - line) > 1 || Math.abs(centreY(status) - line) > 1) throw new Error('tag, title and status off the header line');
    } else if (rect(title).top < rect(tag).bottom) {
      throw new Error('narrow card: the title should sit under the tag and status');
    }
    for (const part of [tag, title, status, card.querySelector('.release-card__excerpt')!, card.querySelector('.release-card__meta')!]) {
      if (rect(part).right > rect(card).right + 0.5) throw new Error('content spills out of its card');
    }
    if (rect(card).right > rect(main).right + 0.5) throw new Error('card wider than the main column');
  }
  return cards.length;
}

async function expectPageLayout({ canvasElement }: { canvasElement: HTMLElement }) {
  await waitFor(() => assertPageLayout(canvasElement), { timeout: 3000 });
}

async function openCreateModal(canvasElement: HTMLElement): Promise<HTMLElement> {
  const button = await waitFor(() => {
    const found = Array.from(canvasElement.querySelectorAll<HTMLButtonElement>('.gbt-page-header__actions button')).find((b) => b.textContent?.includes('Nouvelle release'));
    if (!found) throw new Error('"Nouvelle release" not rendered yet');
    return found;
  });
  button.click();
  return waitFor(() => {
    const dialog = canvasElement.ownerDocument.querySelector<HTMLElement>('[role="dialog"]');
    if (!dialog) throw new Error('dialog not open yet');
    return dialog;
  });
}

const meta: Meta<ReleaseList> = {
  title: 'Releases/ReleaseList',
  component: ReleaseList,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  args: { repositoryId: 'repo-1', path: ['alice', 'ferrisgit'] },
  decorators: [applicationConfig({ providers: [provideRouter([], withDisabledInitialNavigation()), provideFerrisgitIcons()] }), inShellContentArea],
};

export default meta;
type Story = StoryObj<ReleaseList>;

export const Populated: Story = {
  decorators: [withData()],
  play: async (context) => {
    await expectPageLayout(context);
    const tags = Array.from(context.canvasElement.querySelectorAll('.release-card__tag'), (el) => el.textContent?.trim());
    await expect(tags, 'newest first').toEqual(['v2.1.0', 'v2.2.0', 'v2.1.0-rc.2', 'v2.0.0', 'v1.4.3']);
  },
};

export const ReaderView: Story = {
  decorators: [withData({ role: 'reader', releases: MIXED.filter((r) => !r.draft) })],
  play: async (context) => {
    await expectPageLayout(context);
    await expect(context.canvasElement.querySelector('.gbt-button--primary')).toBeNull();
  },
};

export const LongContent: Story = {
  decorators: [withData({ releases: LONG })],
  play: expectPageLayout,
};

export const ManyReleases: Story = {
  decorators: [withData({ releases: MANY })],
  play: async (context) => {
    await expectPageLayout(context);
    await expect(context.canvasElement.querySelectorAll('.release-card').length).toBe(20);
    await expect(context.canvasElement.querySelector('gbt-pagination')).not.toBeNull();
  },
};

export const NoMatchingSearch: Story = {
  decorators: [withData()],
  play: async (context) => {
    const input = await waitFor(() => {
      const found = context.canvasElement.querySelector<HTMLInputElement>('.release-list__search input');
      if (!found) throw new Error('search field not rendered yet');
      return found;
    });
    input.value = 'introuvable';
    input.dispatchEvent(new Event('input'));
    await waitFor(() => expect(context.canvasElement.querySelector('.release-list__no-results')).not.toBeNull());
  },
};

export const Empty: Story = {
  decorators: [withData({ releases: [] })],
};

export const EmptyReadOnly: Story = {
  decorators: [withData({ releases: [], role: 'reader' })],
};

export const Loading: Story = {
  decorators: [withData({ list: () => NEVER })],
};

export const LoadError: Story = {
  decorators: [withData({ list: () => throwError(() => new Error('500')) })],
};

export const CreateDialog: Story = {
  decorators: [withData()],
  play: async ({ canvasElement }) => {
    const dialog = await openCreateModal(canvasElement);
    await waitFor(() => expect(dialog.querySelector('.create-release__target')?.textContent).toContain('main'));
  },
};
