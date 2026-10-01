import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { expect, waitFor } from 'storybook/test';
import { NEVER, of, throwError } from 'rxjs';
import { provideRouter, withDisabledInitialNavigation } from '@angular/router';
import { ReleaseDetail } from './release-detail';
import { ReleaseAsset, ReleaseDetail as ReleaseDetailResponse, ReleasesService } from '../releases.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { GbtToastService } from '@masmarino/gabarit';
import { provideFerrisgitIcons } from '../../shared/register-icons';
import { daysAgo, hoursAgo, inShellContentArea } from '../../shared/layout/page-story-helpers';

const ALICE = { id: 'u1', username: 'alice' };
const BASTIEN = { id: 'u2', username: 'bastien' };

const NOTES = `Cette version apporte des **corrections** de bugs et de nouvelles fonctionnalités.

## Nouveautés

- Les journaux des jobs se mettent à jour en direct
- Nouvelle page de statistiques du dépôt
- Recherche plus rapide dans les gros dépôts

## Corrections

- Correction d'un crash au démarrage quand \`FERRISGIT_DATA_DIR\` n'existe pas
- Le bouton « Cloner » copie enfin l'URL sous Firefox

## Mise à jour

\`\`\`bash
docker compose pull && docker compose up -d
\`\`\`
`;

let nextAsset = 0;
function asset(filename: string, sizeBytes: number, fields: Partial<ReleaseAsset> = {}): ReleaseAsset {
  return { id: `asset-${++nextAsset}`, filename, contentType: 'application/octet-stream', sizeBytes, uploadedBy: ALICE.id, uploader: ALICE, createdAt: daysAgo(2), ...fields };
}

const ASSETS: ReleaseAsset[] = [
  asset('ferrisgit-2.1.0-linux-x86_64.tar.gz', 15_728_640),
  asset('ferrisgit-2.1.0-macos-arm64.tar.gz', 12_582_912),
  asset('SHA256SUMS.txt', 412, { uploader: BASTIEN, createdAt: hoursAgo(3) }),
];

const PUBLISHED: ReleaseDetailResponse = {
  id: '1',
  tagName: 'v2.1.0',
  title: 'v2.1.0 — Améliorations diverses',
  notes: NOTES,
  draft: false,
  prerelease: false,
  authorId: ALICE.id,
  author: ALICE,
  createdAt: daysAgo(3),
  publishedAt: daysAgo(2),
  targetCommitSha: 'a1b2c3d4e5f60718293a4b5c6d7e8f9012345678',
  assets: ASSETS,
};

const MANY_ASSETS: ReleaseAsset[] = [
  ...['linux-x86_64', 'linux-aarch64', 'macos-arm64', 'macos-x86_64', 'windows-x86_64'].flatMap((target, index) => [
    asset(`ferrisgit-2.1.0-${target}.tar.gz`, 14_000_000 + index * 850_000),
    asset(`ferrisgit-2.1.0-${target}.tar.gz.sig`, 566, { uploader: null }),
  ]),
  asset('ferrisgit-2.1.0-un-nom-de-fichier-vraiment-tres-long-pour-verifier-la-troncature-des-lignes.tar.gz', 2_147_483_648, { uploader: { id: 'u9', username: 'Maximilien de La Tour d’Auvergne' } }),
];

const LONG_NOTES = Array.from(
  { length: 6 },
  (_, index) => `## Section ${index + 1}

Un paragraphe de notes suffisamment long pour remplir plusieurs lignes et vérifier le rythme de lecture de la carte, avec du \`code\`, un [lien](https://example.com) et une liste :

- premier point
- deuxième point, un peu plus long que le premier pour voir le retour à la ligne
- troisième point

| Plateforme | Archive |
| --- | --- |
| Linux | \`tar.gz\` |
| macOS | \`tar.gz\` |
`,
).join('\n');

function withData(options: { release?: ReleaseDetailResponse; role?: 'owner' | 'maintainer' | 'reader'; detail?: () => unknown } = {}) {
  const release = options.release ?? PUBLISHED;
  return moduleMetadata({
    providers: [
      {
        provide: ReleasesService,
        useValue: {
          detail: options.detail ?? (() => of(release)),
          update: (_repositoryId: string, _tagName: string, patch: object) => of({ ...release, ...patch, publishedAt: release.publishedAt ?? new Date().toISOString() }),
          delete: () => of(undefined),
          uploadAsset: () => of(asset('nouveau-fichier.zip', 1_048_576, { createdAt: new Date().toISOString() })),
          deleteAsset: () => of(undefined),
          downloadAsset: () => of(new Blob(['fake content'])),
        },
      },
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

/** Layout checks jsdom cannot make: no horizontal overflow, aside beside the main column from 769px, file-row buttons and names inside their card. */
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

  const card = canvas.querySelector('.release-detail__assets')!;
  for (const row of Array.from(canvas.querySelectorAll('.release-detail__asset'))) {
    const actions = row.querySelector('.release-detail__asset-actions')!;
    if (Math.abs(centreY(actions) - centreY(row)) > 1) throw new Error('file buttons off the row centre');
    if (rect(actions).right > rect(card).right + 0.5) throw new Error('file buttons spill out of the card');
    const name = row.querySelector('.release-detail__asset-name')!;
    if (rect(name).right > rect(actions).left + 0.5) throw new Error('file name runs under the buttons');
  }
}

async function expectPageLayout({ canvasElement }: { canvasElement: HTMLElement }) {
  await waitFor(() => assertPageLayout(canvasElement), { timeout: 3000 });
}

function headerButton(canvasElement: HTMLElement, label: string): Promise<HTMLButtonElement> {
  return waitFor(() => {
    const found = Array.from(canvasElement.querySelectorAll<HTMLButtonElement>('.gbt-page-header__actions button')).find((b) => b.textContent?.trim() === label);
    if (!found) throw new Error(`"${label}" not rendered yet`);
    return found;
  });
}

const meta: Meta<ReleaseDetail> = {
  title: 'Releases/ReleaseDetail',
  component: ReleaseDetail,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  args: { repositoryId: 'repo-1', path: ['alice', 'ferrisgit'], tagName: 'v2.1.0' },
  decorators: [applicationConfig({ providers: [provideRouter([], withDisabledInitialNavigation()), provideFerrisgitIcons()] }), inShellContentArea],
};

export default meta;
type Story = StoryObj<ReleaseDetail>;

export const Published: Story = {
  decorators: [withData()],
  play: async (context) => {
    await expectPageLayout(context);
    await expect(context.canvasElement.querySelector('.gbt-button--primary'), 'no primary once published').toBeNull();
  },
};

export const Draft: Story = {
  decorators: [withData({ release: { ...PUBLISHED, tagName: 'v2.2.0', title: 'v2.2.0 (à venir)', draft: true, publishedAt: null, notes: '- [ ] Rédiger les notes\n- [ ] Joindre les binaires', assets: [] } })],
  play: async (context) => {
    await expectPageLayout(context);
    const primaries = Array.from(context.canvasElement.querySelectorAll('.gbt-button--primary'), (b) => b.textContent?.trim());
    await expect(primaries).toEqual(['Publier']);
  },
};

export const PreRelease: Story = {
  decorators: [withData({ release: { ...PUBLISHED, tagName: 'v2.1.0-rc.2', title: 'Release candidate 2', prerelease: true, author: BASTIEN } })],
  play: expectPageLayout,
};

export const ReaderView: Story = {
  decorators: [withData({ role: 'reader' })],
  play: async (context) => {
    await expectPageLayout(context);
    await expect(context.canvasElement.querySelector('.gbt-page-header__actions gbt-button')).toBeNull();
    await expect(context.canvasElement.querySelector('gbt-file-upload')).toBeNull();
  },
};

export const DeletedTag: Story = {
  decorators: [withData({ release: { ...PUBLISHED, targetCommitSha: null, author: null } })],
  play: expectPageLayout,
};

export const ManyAssets: Story = {
  decorators: [withData({ release: { ...PUBLISHED, assets: MANY_ASSETS } })],
  play: expectPageLayout,
};

export const LongNotes: Story = {
  decorators: [withData({ release: { ...PUBLISHED, notes: LONG_NOTES, assets: [] } })],
  play: expectPageLayout,
};

export const Editing: Story = {
  decorators: [withData()],
  play: async ({ canvasElement }) => {
    (await headerButton(canvasElement, 'Modifier')).click();
    await waitFor(() => expect(canvasElement.querySelector('.release-detail__edit')).not.toBeNull());
    await waitFor(() => expect(canvasElement.ownerDocument.activeElement?.closest('.release-detail__edit-title')).not.toBeNull());
  },
};

export const ConfirmDelete: Story = {
  decorators: [withData()],
  play: async ({ canvasElement }) => {
    (await headerButton(canvasElement, 'Supprimer')).click();
    await waitFor(() => expect(canvasElement.ownerDocument.querySelector('[role="dialog"]')?.getAttribute('aria-label')).toBe('Supprimer la release'));
  },
};

export const ConfirmDeleteAsset: Story = {
  decorators: [withData()],
  play: async ({ canvasElement }) => {
    const button = await waitFor(() => {
      const found = canvasElement.querySelector<HTMLButtonElement>('button[aria-label="Supprimer SHA256SUMS.txt"]');
      if (!found) throw new Error('file row not rendered yet');
      return found;
    });
    button.click();
    const dialog = await waitFor(() => {
      const found = canvasElement.ownerDocument.querySelector<HTMLElement>('gbt-confirm-danger-modal [role="dialog"]');
      if (!found) throw new Error('dialog not rendered yet');
      return found;
    });
    await expect(dialog.getAttribute('aria-label')).toBe('Supprimer le fichier');
    await expect(dialog.querySelector('input')).toBeNull();
  },
};

export const Loading: Story = {
  decorators: [withData({ detail: () => NEVER })],
};

export const NotFound: Story = {
  decorators: [withData({ role: 'reader', detail: () => throwError(() => new Error('404')) })],
};
