import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { NEVER, of, throwError } from 'rxjs';
import { userEvent, within } from 'storybook/test';
import { CreateGroupModal } from './create-group-modal';
import { Group, GroupsService } from '../groups.service';
import { provideFerrisgitIcons } from '../../shared/register-icons';
import { atPhoneWidth, inDarkTheme } from '../../shared/layout/page-story-helpers';

// Environment providers only work in `applicationConfig`, not `moduleMetadata`.
const withIcons = applicationConfig({ providers: [provideFerrisgitIcons()] });

const GROUP: Group = { id: 'g1', parentGroupId: null, name: 'acme-france', description: '', createdAt: '2026-01-01T00:00:00Z' };

const withCreate = (createRoot: () => unknown) => moduleMetadata({ providers: [{ provide: GroupsService, useValue: { createRoot } }] });

const meta: Meta<CreateGroupModal> = {
  title: 'Groups/CreateGroupModal',
  component: CreateGroupModal,
  tags: ['autodocs'],
  decorators: [withIcons, withCreate(() => of(GROUP))],
};

export default meta;
type Story = StoryObj<CreateGroupModal>;

export const Empty: Story = {};

export const Dark: Story = { decorators: [inDarkTheme] };

export const AtPhoneWidth: Story = { decorators: [atPhoneWidth] };

export const NameRequired: Story = {
  play: async ({ canvasElement }) => {
    const body = within(canvasElement.ownerDocument.body);
    await userEvent.click(await body.findByRole('button', { name: 'Créer le groupe' }));
  },
};

export const NameTaken: Story = {
  decorators: [withCreate(() => throwError(() => ({ status: 409 })))],
  play: async ({ canvasElement }) => {
    const body = within(canvasElement.ownerDocument.body);
    await userEvent.type(await body.findByLabelText(/Nom du groupe/), 'acme-france');
    await userEvent.click(await body.findByRole('button', { name: 'Créer le groupe' }));
  },
};

export const Creating: Story = {
  decorators: [withCreate(() => NEVER)],
  play: async ({ canvasElement }) => {
    const body = within(canvasElement.ownerDocument.body);
    await userEvent.type(await body.findByLabelText(/Nom du groupe/), 'acme-france');
    await userEvent.click(await body.findByRole('button', { name: /Créer le groupe/ }));
  },
};
