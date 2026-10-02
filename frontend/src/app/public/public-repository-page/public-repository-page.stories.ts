import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig } from '@storybook/angular-vite';
import { HttpErrorResponse } from '@angular/common/http';
import { expect, waitFor } from 'storybook/test';
import { of, throwError } from 'rxjs';
import { PublicLayout } from '../public-layout/public-layout';
import { PublicRepositoryPage } from './public-repository-page';
import { publicRepositoryMatcher } from '../public.guards';
import { RepositoriesService, ResolvedPath } from '../../repositories/repositories.service';
import { READ_ONLY_REPOSITORY } from '../../repositories/read-only-repository';
import { MergeRequestsService } from '../../merge-requests/merge-requests.service';
import { ReleaseDetail, ReleasesService, ReleaseSummary } from '../../releases/releases.service';
import { BRANCHES, COMMITS, fakeRepositoriesService, POPULATED, REPO, TAGS } from '../../repositories/repository-story-fixtures';
import { atPhoneWidth, daysAgo, inDarkTheme, withFerrisgitIcons } from '../../shared/layout/page-story-helpers';
import { withPublicCatalog } from '../public-story-fixtures';

const ALICE = { id: 'u1', username: 'florian' };

const RELEASES: ReleaseSummary[] = [
  { id: 'r1', tagName: 'v1.4.0', title: 'FerrisGit 1.4', draft: false, prerelease: false, authorId: ALICE.id, author: ALICE, notesExcerpt: '## Nouveautés\n\n- Pages publiques des dépôts\n- Catalogue sans compte', assetCount: 2, createdAt: daysAgo(3), publishedAt: daysAgo(3) },
  { id: 'r2', tagName: 'v1.4.0-rc.1', title: 'Release candidate 1', draft: false, prerelease: true, authorId: ALICE.id, author: ALICE, notesExcerpt: 'Première candidate.', assetCount: 0, createdAt: daysAgo(9), publishedAt: daysAgo(9) },
];

const RELEASE: ReleaseDetail = {
  id: 'r1',
  tagName: 'v1.4.0',
  title: 'FerrisGit 1.4',
  notes: '## Nouveautés\n\n- Pages publiques des dépôts\n- Catalogue sans compte\n\n## Corrections\n\n- Le bouton « Cloner » copie l’URL sous Firefox',
  draft: false,
  prerelease: false,
  authorId: ALICE.id,
  author: ALICE,
  createdAt: daysAgo(3),
  publishedAt: daysAgo(3),
  targetCommitSha: COMMITS[0].sha,
  assets: [
    { id: 'a1', filename: 'ferrisgit-1.4.0-x86_64-linux.tar.gz', contentType: 'application/gzip', sizeBytes: 18_400_000, uploadedBy: ALICE.id, uploader: ALICE, createdAt: daysAgo(3) },
    { id: 'a2', filename: 'SHA256SUMS', contentType: 'text/plain', sizeBytes: 180, uploadedBy: ALICE.id, uploader: ALICE, createdAt: daysAgo(3) },
  ],
};

const PUBLIC_REPO = { ...REPO, isStarred: undefined };

/** The public repository pages in the public layout, on fake reads, starting at `url`. */
function atRepository(url: string, resolved: () => ReturnType<RepositoriesService['resolve']> = () => of<ResolvedPath>({ type: 'personalRepository', repositoryId: REPO.id })) {
  return [
    withPublicCatalog({ url, routes: [{ matcher: publicRepositoryMatcher, component: PublicRepositoryPage }] }),
    applicationConfig({
      providers: [
        { provide: RepositoriesService, useValue: { ...fakeRepositoriesService({ ...POPULATED, repo: PUBLIC_REPO }), resolve: resolved } },
        { provide: MergeRequestsService, useValue: { listBranches: () => of(BRANCHES) } },
        { provide: ReleasesService, useValue: { listTags: () => of(TAGS), list: () => of(RELEASES), detail: () => of(RELEASE), downloadAsset: () => of(new Blob()) } },
        { provide: READ_ONLY_REPOSITORY, useValue: true },
      ],
    }),
  ];
}

async function expectPublicRepository({ canvasElement }: { canvasElement: HTMLElement }) {
  await waitFor(() => {
    if (!canvasElement.querySelector('gbt-nav-tabs a[aria-current="page"]')) throw new Error('page not rendered yet');
  });
  await expect(canvasElement.querySelector('.repository-header__star'), 'no star button for a visitor').toBeNull();
  const doc = canvasElement.ownerDocument.documentElement;
  await expect(doc.scrollWidth, 'no horizontal overflow').toBeLessThanOrEqual(doc.clientWidth + 1);
}

const meta: Meta<PublicLayout> = {
  title: 'Public/PublicRepositoryPage',
  component: PublicLayout,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [withFerrisgitIcons],
};

export default meta;
type Story = StoryObj<PublicLayout>;

export const Overview: Story = {
  decorators: atRepository('/repositories/florian/ferrisgit'),
  play: expectPublicRepository,
};

export const OverviewDark: Story = {
  decorators: [...atRepository('/repositories/florian/ferrisgit'), inDarkTheme],
  play: expectPublicRepository,
};

export const OverviewPhone: Story = {
  decorators: [...atRepository('/repositories/florian/ferrisgit'), atPhoneWidth],
  play: expectPublicRepository,
};

export const File: Story = {
  decorators: atRepository('/repositories/florian/ferrisgit/-/blob/main/README.md'),
};

export const Commits: Story = {
  decorators: atRepository('/repositories/florian/ferrisgit/-/commits'),
  play: expectPublicRepository,
};

export const Releases: Story = {
  decorators: atRepository('/repositories/florian/ferrisgit/-/releases'),
  play: async (context) => {
    await expectPublicRepository(context);
    await expect(context.canvasElement.textContent, 'no "Nouvelle release" for a visitor').not.toContain('Nouvelle release');
  },
};

export const ReleaseWithAssets: Story = {
  decorators: atRepository('/repositories/florian/ferrisgit/-/releases/v1.4.0'),
};

export const ReleaseWithAssetsDark: Story = {
  decorators: [...atRepository('/repositories/florian/ferrisgit/-/releases/v1.4.0'), inDarkTheme],
};

export const NotFound: Story = {
  decorators: atRepository('/repositories/acme/secret', () => throwError(() => new HttpErrorResponse({ status: 404, statusText: 'Not Found' }))),
};

export const NotFoundDark: Story = {
  decorators: [...atRepository('/repositories/acme/secret', () => throwError(() => new HttpErrorResponse({ status: 404, statusText: 'Not Found' }))), inDarkTheme],
};

export const RateLimited: Story = {
  decorators: atRepository('/repositories/florian/ferrisgit', () => throwError(() => new HttpErrorResponse({ status: 429, statusText: 'Too Many Requests' }))),
};
