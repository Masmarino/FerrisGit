import type { Meta, StoryObj } from '@storybook/angular-vite';
import { RepositoryDetail } from './repository-detail';
import { inShellContentArea } from '../../shared/layout/page-story-helpers';
import { EMPTY_REPO, fakeRepositoriesService, LOADING_REPOSITORIES, POPULATED, withRepository, withRouterAndIcons } from '../repository-story-fixtures';

const meta: Meta<RepositoryDetail> = {
  title: 'Repositories/RepositoryDetail',
  component: RepositoryDetail,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [withRouterAndIcons, inShellContentArea],
  argTypes: {
    repositoryId: { control: 'text' },
  },
};

export default meta;
type Story = StoryObj<RepositoryDetail>;

export const Populated: Story = {
  args: { repositoryId: 'repo-1', path: ['florian', 'ferrisgit'] },
  decorators: [withRepository(fakeRepositoriesService(POPULATED))],
};

export const EmptyRepository: Story = {
  args: { repositoryId: 'repo-3', path: ['florian', 'nouveau-projet'] },
  decorators: [withRepository(fakeRepositoriesService({ ...POPULATED, repo: EMPTY_REPO, entries: [], readme: null, commits: [], languages: [], contributors: [], treeStatus: 404 }), { branches: [], tags: [] })],
};

export const Loading: Story = {
  args: { repositoryId: 'repo-1', path: ['florian', 'ferrisgit'] },
  decorators: [withRepository(LOADING_REPOSITORIES)],
};
