import type { Meta, StoryObj } from '@storybook/angular-vite';
import { HttpErrorResponse } from '@angular/common/http';
import { NEVER, of, throwError } from 'rxjs';
import { RepositoryCommitList } from './repository-commit-list';
import { inDarkTheme, inShellContentArea } from '../../shared/layout/page-story-helpers';
import { COMMITS, withRepository, withRouterAndIcons } from '../repository-story-fixtures';

const meta: Meta<RepositoryCommitList> = {
  title: 'Repositories/RepositoryCommitList',
  component: RepositoryCommitList,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [withRouterAndIcons, inShellContentArea],
  args: { repositoryId: 'repo-1', ref: 'HEAD' },
};

export default meta;
type Story = StoryObj<RepositoryCommitList>;

export const Default: Story = {
  decorators: [withRepository({ commitsById: () => of(COMMITS) })],
};

export const Dark: Story = {
  decorators: [withRepository({ commitsById: () => of(COMMITS) }), inDarkTheme],
};

export const OnABranch: Story = {
  args: { ref: 'develop' },
  decorators: [withRepository({ commitsById: () => of(COMMITS.slice(1)) })],
};

export const Empty: Story = {
  decorators: [withRepository({ commitsById: () => of([]) })],
};

export const Loading: Story = {
  decorators: [withRepository({ commitsById: () => NEVER })],
};

export const LoadFailed: Story = {
  decorators: [withRepository({ commitsById: () => throwError(() => new HttpErrorResponse({ status: 500 })) })],
};
