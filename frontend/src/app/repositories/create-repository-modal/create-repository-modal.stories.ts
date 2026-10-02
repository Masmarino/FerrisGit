import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { of, throwError } from 'rxjs';
import { userEvent, within } from 'storybook/test';
import { CreateRepositoryModal } from './create-repository-modal';
import { RepositoriesService } from '../repositories.service';
import { GroupsService, WritableGroup } from '../../groups/groups.service';
import { provideFerrisgitIcons } from '../../shared/register-icons';

// provideFerrisgitIcons returns EnvironmentProviders, which only fit in an ApplicationConfig.
const withIcons = applicationConfig({ providers: [provideFerrisgitIcons()] });

function withGroups(groups: WritableGroup[], create: () => unknown = () => of(undefined)) {
  return moduleMetadata({
    providers: [
      { provide: GroupsService, useValue: { listWritable: () => of(groups) } },
      { provide: RepositoriesService, useValue: { create } },
    ],
  });
}

const WRITABLE_GROUPS: WritableGroup[] = [
  { id: 'group-1', path: 'acme-france' },
  { id: 'group-2', path: 'acme-france/produits/web' },
  { id: 'group-3', path: 'communaute' },
];

const meta: Meta<CreateRepositoryModal> = {
  title: 'Repositories/CreateRepositoryModal',
  component: CreateRepositoryModal,
  tags: ['autodocs'],
  decorators: [withIcons],
};

export default meta;
type Story = StoryObj<CreateRepositoryModal>;

export const Personal: Story = {
  decorators: [withGroups([])],
};

export const WithWritableGroups: Story = {
  decorators: [withGroups(WRITABLE_GROUPS)],
};

export const AdvancedOptionsOpen: Story = {
  decorators: [withGroups(WRITABLE_GROUPS)],
  play: async ({ canvasElement }) => {
    const body = within(canvasElement.ownerDocument.body);
    await userEvent.click(await body.findByRole('button', { name: /Options avancées/ }));
  },
};

export const NameError: Story = {
  decorators: [withGroups(WRITABLE_GROUPS)],
  play: async ({ canvasElement }) => {
    const body = within(canvasElement.ownerDocument.body);
    await userEvent.click(await body.findByRole('button', { name: 'Créer le dépôt' }));
  },
};

export const DefaultLocation: Story = {
  args: { defaultLocation: 'acme-france/produits/web' },
  decorators: [withGroups(WRITABLE_GROUPS)],
};

export const NameTaken: Story = {
  decorators: [withGroups(WRITABLE_GROUPS, () => throwError(() => ({ status: 409 })))],
  play: async ({ canvasElement }) => {
    const body = within(canvasElement.ownerDocument.body);
    await userEvent.type(await body.findByLabelText(/Nom du dépôt/), 'facturation-api');
    await userEvent.click(await body.findByRole('button', { name: 'Créer le dépôt' }));
  },
};
