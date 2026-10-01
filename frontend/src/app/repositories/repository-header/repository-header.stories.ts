import type { Meta, StoryObj } from '@storybook/angular-vite';
import { RepositoryHeader } from './repository-header';
import { inShellContentArea } from '../../shared/layout/page-story-helpers';
import { fakeRepositoriesService, POPULATED, PRIVATE_REPO, REPO, withRepository, withRouterAndIcons } from '../repository-story-fixtures';

const meta: Meta<RepositoryHeader> = {
  title: 'Repositories/RepositoryHeader',
  component: RepositoryHeader,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  args: { ref: 'HEAD' },
  decorators: [withRouterAndIcons, inShellContentArea, withRepository(fakeRepositoriesService(POPULATED))],
};

export default meta;
type Story = StoryObj<RepositoryHeader>;

export const PublicNotStarred: Story = {
  args: { repo: REPO },
};

export const Starred: Story = {
  args: { repo: { ...REPO, isStarred: true, starCount: 129 } },
};

export const PrivateOnABranch: Story = {
  args: { repo: PRIVATE_REPO, ref: 'feature/refonte-des-pages-du-depot' },
};

export const NoStarsYet: Story = {
  args: { repo: { ...REPO, starCount: 0, isStarred: false } },
};

export const LongName: Story = {
  args: { repo: { ...REPO, name: 'une-bibliotheque-de-composants-au-nom-particulierement-long-pour-tester-le-retour-a-la-ligne' } },
};

export const WithoutRefs: Story = {
  args: { repo: { ...REPO, starCount: undefined, isStarred: undefined } },
  decorators: [withRepository(fakeRepositoriesService(POPULATED), { branches: [], tags: [] })],
};

export const Loading: Story = {
  args: { repo: null },
};
