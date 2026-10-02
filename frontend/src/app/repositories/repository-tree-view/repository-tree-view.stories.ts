import type { Meta, StoryObj } from '@storybook/angular-vite';
import { NEVER, of } from 'rxjs';
import { expect, userEvent, waitFor } from 'storybook/test';
import { RepositoryTreeView } from './repository-tree-view';
import { inShellContentArea } from '../../shared/layout/page-story-helpers';
import {
  COMMITS,
  EMPTY_REPO,
  fakeRepositoriesService,
  LOADING_REPOSITORIES,
  LONG_NAME_ENTRIES,
  LONG_README,
  POPULATED,
  PRIVATE_REPO,
  REPO,
  withRepository,
  withRouterAndIcons,
} from '../repository-story-fixtures';

/** Needs real layout, which jsdom lacks. With table-layout: fixed the header cells set the column widths, so hiding them gave three equal thirds. */
async function expectFileColumns({ canvasElement }: { canvasElement: HTMLElement }) {
  const table = await waitFor(() => {
    const found = canvasElement.querySelector<HTMLTableElement>('.repository-tree-view__table');
    if (!found) throw new Error('files table not rendered yet');
    return found;
  });
  const width = (el: Element) => el.getBoundingClientRect().width;
  const tableWidth = width(table);
  const [name, message, date] = Array.from(table.querySelectorAll(':scope > tbody > tr:first-child > td'), width);

  await expect(
    [name, message, date].every((column) => Math.abs(column - tableWidth / 3) < 2),
    `equal-thirds columns in a ${Math.round(tableWidth)}px table`,
  ).toBe(false);
  await expect(name, 'the name column is the widest').toBeGreaterThan(Math.max(message, date));
  await expect(date, 'the date column keeps a fixed narrow width').toBeLessThanOrEqual(128.5);

  const header = Array.from(table.querySelectorAll(':scope > thead > tr > th'), (th) => Math.round(width(th)));
  await expect(header, 'the header cells carry the column widths').toEqual([name, message, date].map(Math.round));

  const avatar = canvasElement.querySelector('.repository-tree-view__latest-author gbt-avatar');
  const latestMessage = canvasElement.querySelector('.repository-tree-view__latest-message');
  if (avatar && latestMessage) {
    await expect(avatar.getBoundingClientRect().right, 'the banner message starts after the avatar').toBeLessThanOrEqual(latestMessage.getBoundingClientRect().left);
  }
}

const meta: Meta<RepositoryTreeView> = {
  title: 'Repositories/RepositoryTreeView',
  component: RepositoryTreeView,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [withRouterAndIcons, inShellContentArea],
  args: { repositoryId: 'repo-1', ref: 'main', treePath: [] },
};

export default meta;
type Story = StoryObj<RepositoryTreeView>;

export const Overview: Story = {
  play: expectFileColumns,
  decorators: [withRepository(fakeRepositoriesService(POPULATED))],
};

export const Subfolder: Story = {
  play: expectFileColumns,
  args: { treePath: ['crates'] },
  decorators: [withRepository(fakeRepositoriesService({ ...POPULATED, entries: POPULATED.folderEntries }))],
};

export const PrivateRepositoryStarred: Story = {
  play: expectFileColumns,
  args: { ref: 'develop' },
  decorators: [withRepository(fakeRepositoriesService({ ...POPULATED, repo: PRIVATE_REPO }))],
};

export const LongNamesAndReadme: Story = {
  play: expectFileColumns,
  decorators: [
    withRepository(
      fakeRepositoriesService({
        ...POPULATED,
        repo: { ...REPO, name: 'une-bibliotheque-de-composants-au-nom-particulierement-long', description: '' },
        entries: LONG_NAME_ENTRIES,
        readme: LONG_README,
        commits: [{ ...COMMITS[0], authorName: 'Maximilien de La Tour d’Auvergne', message: 'Un message de commit particulièrement long qui ne tient pas sur une seule ligne du bandeau, même sur un grand écran' }],
      }),
    ),
  ],
};

export const Sparse: Story = {
  play: expectFileColumns,
  decorators: [withRepository(fakeRepositoriesService({ ...POPULATED, entries: POPULATED.entries.slice(4, 6), readme: null, commits: [], languages: [], contributors: [] }))],
};

export const EmptyRepository: Story = {
  args: { ref: 'HEAD' },
  decorators: [withRepository(fakeRepositoriesService({ ...POPULATED, repo: EMPTY_REPO, entries: [], readme: null, commits: [], languages: [], contributors: [], treeStatus: 404 }), { branches: [], tags: [] })],
};

export const NotFound: Story = {
  args: { ref: 'n-existe-pas', treePath: ['docs'] },
  decorators: [withRepository(fakeRepositoriesService({ ...POPULATED, treeStatus: 404 }))],
};

export const LoadFailed: Story = {
  decorators: [withRepository(fakeRepositoriesService({ ...POPULATED, treeStatus: 500 }))],
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.querySelector('gbt-alert .gbt-alert[data-variant="error"]')).not.toBeNull());
  },
};

export const Loading: Story = {
  decorators: [withRepository(LOADING_REPOSITORIES)],
};

export const ExpandingFolder: Story = {
  decorators: [withRepository({ ...fakeRepositoriesService(POPULATED), treeAt: (_id: string, _ref: string, path: string[]) => (path.length === 0 ? of(POPULATED.entries) : NEVER) })],
  play: async ({ canvasElement }) => {
    const toggle = await waitFor(() => {
      const found = canvasElement.querySelector<HTMLButtonElement>('.repository-tree-view__toggle');
      if (!found) throw new Error('no folder row rendered yet');
      return found;
    });
    await userEvent.click(toggle);
    await waitFor(() => expect(canvasElement.querySelector('gbt-spinner')).not.toBeNull());
  },
};
