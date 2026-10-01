import type { Meta, StoryObj } from '@storybook/angular-vite';
import { componentWrapperDecorator, moduleMetadata } from '@storybook/angular-vite';
import { NEVER, of } from 'rxjs';
import { ContributorAvatars } from './contributor-avatars';
import { Contributor, RepositoriesService } from '../repositories.service';

function withContributors(contributors: Contributor[]) {
  return moduleMetadata({
    providers: [{ provide: RepositoriesService, useValue: { listContributors: () => of(contributors) } }],
  });
}

const CONTRIBUTORS: Contributor[] = [
  { name: 'Camille Martin', email: 'camille.martin@example.fr', commitCount: 142 },
  { name: 'Julien Dubois', email: 'julien.dubois@example.fr', commitCount: 87 },
  { name: 'Léa Bernard', email: 'lea.bernard@example.fr', commitCount: 64 },
  { name: 'Thomas Petit', email: 'thomas.petit@example.fr', commitCount: 51 },
  { name: 'Manon Lefèvre', email: 'manon.lefevre@example.fr', commitCount: 38 },
  { name: 'Nicolas Moreau', email: 'nicolas.moreau@example.fr', commitCount: 27 },
  { name: 'Chloé Fontaine', email: 'chloe.fontaine@example.fr', commitCount: 19 },
  { name: 'Antoine Girard', email: 'antoine.girard@example.fr', commitCount: 14 },
  { name: 'Sophie Rousseau', email: 'sophie.rousseau@example.fr', commitCount: 9 },
  { name: 'Hugo Lambert', email: 'hugo.lambert@example.fr', commitCount: 6 },
  { name: 'Inès Faure', email: 'ines.faure@example.fr', commitCount: 3 },
  { name: 'admin', email: 'admin@example.fr', commitCount: 1 },
];

const meta: Meta<ContributorAvatars> = {
  title: 'Repositories/ContributorAvatars',
  component: ContributorAvatars,
  tags: ['autodocs'],
  args: { repositoryId: 'repo-1', ref: 'HEAD' },
  decorators: [componentWrapperDecorator((story) => `<div style="width: 300px; max-width: 100%;">${story}</div>`)],
};

export default meta;
type Story = StoryObj<ContributorAvatars>;

export const SingleContributor: Story = {
  decorators: [withContributors(CONTRIBUTORS.slice(0, 1))],
};

export const ThreeContributors: Story = {
  decorators: [withContributors(CONTRIBUTORS.slice(0, 3))],
};

export const ManyContributorsWithOverflow: Story = {
  decorators: [withContributors(CONTRIBUTORS)],
};

export const ExactlyAtLimit: Story = {
  decorators: [withContributors(CONTRIBUTORS.slice(0, 8))],
};

export const NoContributors: Story = {
  decorators: [withContributors([])],
};

export const LongName: Story = {
  decorators: [withContributors([{ name: 'Maximilien de La Tour d’Auvergne-Montmorency', email: 'max@example.fr', commitCount: 1287 }, ...CONTRIBUTORS.slice(0, 2)])],
};

export const Loading: Story = {
  decorators: [moduleMetadata({ providers: [{ provide: RepositoriesService, useValue: { listContributors: () => NEVER } }] })],
};
