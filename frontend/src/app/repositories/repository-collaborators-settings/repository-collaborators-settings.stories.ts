import type { Meta, StoryObj } from '@storybook/angular-vite';
import { moduleMetadata } from '@storybook/angular-vite';
import { expect, userEvent, waitFor } from 'storybook/test';
import { NEVER, of, throwError } from 'rxjs';
import { RepositoryCollaboratorsSettings } from './repository-collaborators-settings';
import { CollaboratorSummary, RepositorySettingsService } from '../repository-settings.service';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { daysAgo, hoursAgo, withFerrisgitIcons } from '../../shared/layout/page-story-helpers';
import { expectRows, expectSettingsLayout, fakeToast, inSettingsColumn, rendered } from '../../shared/layout/settings-story-helpers';

const COLLABORATORS: CollaboratorSummary[] = [
  { userId: 'u1', username: 'camille.durand', role: 'maintainer', createdAt: daysAgo(120) },
  { userId: 'u2', username: 'julien.martin', role: 'contributor', createdAt: daysAgo(21) },
  { userId: 'u3', username: 'sophie.bernard', role: 'contributor', createdAt: daysAgo(3) },
  { userId: 'u4', username: 'lucas.petit', role: 'reader', createdAt: hoursAgo(5) },
  { userId: 'u5', username: 'maximilien.de-la-tour-d-auvergne-et-de-bouillon', role: 'reader', createdAt: hoursAgo(1) },
];

function fakeRepositorySettingsService(overrides: Record<string, unknown> = {}) {
  return {
    listCollaborators: () => of(COLLABORATORS),
    addCollaborator: () => of(undefined),
    setCollaboratorRole: () => of(undefined),
    removeCollaborator: () => of(undefined),
    ...overrides,
  };
}

const withService = (overrides: Record<string, unknown>) =>
  moduleMetadata({ providers: [{ provide: RepositorySettingsService, useValue: fakeRepositorySettingsService(overrides) }] });

const meta: Meta<RepositoryCollaboratorsSettings> = {
  title: 'Repositories/Settings/RepositoryCollaboratorsSettings',
  component: RepositoryCollaboratorsSettings,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [
    withFerrisgitIcons,
    moduleMetadata({
      providers: [
        { provide: RepositorySettingsService, useValue: fakeRepositorySettingsService() },
        { provide: GbtToastService, useValue: fakeToast },
      ],
    }),
    inSettingsColumn,
  ],
  args: { repositoryId: 'repo-1' },
  play: async ({ canvasElement }) => {
    await rendered(canvasElement, 'gbt-card');
    await expectSettingsLayout(canvasElement);
    await expectRows(canvasElement);
  },
};

export default meta;
type Story = StoryObj<RepositoryCollaboratorsSettings>;

export const Populated: Story = {};

export const SingleMaintainer: Story = {
  decorators: [withService({ listCollaborators: () => of([COLLABORATORS[0]]) })],
};

export const RoleSelectOpen: Story = {
  play: async ({ canvasElement }) => {
    const trigger = await rendered(canvasElement, 'gbt-list-row .gbt-select__trigger');
    await userEvent.click(trigger);
    await waitFor(() => expect(canvasElement.querySelector('gbt-list-row [role="listbox"]')).not.toBeNull());
  },
};

export const ConfirmRemoval: Story = {
  play: async ({ canvasElement }) => {
    const button = await rendered(canvasElement, '[aria-label="Retirer julien.martin"]');
    await userEvent.click(button);
    const dialog = await waitFor(() => {
      const found = canvasElement.ownerDocument.querySelector('gbt-confirm-danger-modal [role="dialog"]');
      if (!found) throw new Error('dialog not rendered yet');
      return found;
    });
    await expect(dialog.getAttribute('aria-label')).toBe('Retirer le collaborateur');
    await expect(dialog.querySelector('input')).toBeNull();
  },
};

export const Empty: Story = {
  decorators: [withService({ listCollaborators: () => of([]) })],
};

export const Loading: Story = {
  decorators: [withService({ listCollaborators: () => NEVER })],
  play: async ({ canvasElement }) => {
    await rendered(canvasElement, 'gbt-skeleton-list [role="status"]');
    await expect(canvasElement.querySelector('gbt-skeleton-list [role="status"]')?.textContent?.trim()).toBe('Chargement des collaborateurs…');
  },
};

export const LoadFailed: Story = {
  decorators: [withService({ listCollaborators: () => throwError(() => ({ status: 500 })) })],
};
