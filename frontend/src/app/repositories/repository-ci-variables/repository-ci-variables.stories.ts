import type { Meta, StoryObj } from '@storybook/angular-vite';
import { moduleMetadata } from '@storybook/angular-vite';
import { expect, userEvent, waitFor } from 'storybook/test';
import { NEVER, of, throwError } from 'rxjs';
import { RepositoryCiVariables } from './repository-ci-variables';
import { CiVariableSummary, RepositorySettingsService } from '../repository-settings.service';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { withFerrisgitIcons } from '../../shared/layout/page-story-helpers';
import { expectRows, expectSettingsLayout, fakeToast, inSettingsColumn, rendered } from '../../shared/layout/settings-story-helpers';

const VARIABLES: CiVariableSummary[] = [
  { id: 'v1', key: 'DATABASE_URL', masked: true },
  { id: 'v2', key: 'REGISTRY_TOKEN', masked: true },
  { id: 'v3', key: 'RUST_LOG', masked: false },
  { id: 'v4', key: 'DEPLOY_ENVIRONMENT', masked: false },
  { id: 'v5', key: 'SENTRY_DSN_FOR_THE_STAGING_ENVIRONMENT_OF_THE_PUBLIC_API_GATEWAY', masked: true },
];

function fakeRepositorySettingsService(overrides: Record<string, unknown> = {}) {
  return {
    listCiVariables: () => of(VARIABLES),
    setCiVariable: () => of({ id: 'v6', key: 'NEW_VAR', masked: true }),
    deleteCiVariable: () => of(undefined),
    ...overrides,
  };
}

const withService = (overrides: Record<string, unknown>) =>
  moduleMetadata({ providers: [{ provide: RepositorySettingsService, useValue: fakeRepositorySettingsService(overrides) }] });

const meta: Meta<RepositoryCiVariables> = {
  title: 'Repositories/Settings/RepositoryCiVariables',
  component: RepositoryCiVariables,
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
type Story = StoryObj<RepositoryCiVariables>;

export const Populated: Story = {};

export const Empty: Story = {
  decorators: [withService({ listCiVariables: () => of([]) })],
};

export const Loading: Story = {
  decorators: [withService({ listCiVariables: () => NEVER })],
  play: async ({ canvasElement }) => {
    await rendered(canvasElement, 'gbt-skeleton-list [role="status"]');
    await expect(canvasElement.querySelector('gbt-skeleton-list [role="status"]')?.textContent?.trim()).toBe('Chargement des variables…');
  },
};

export const LoadFailed: Story = {
  decorators: [withService({ listCiVariables: () => throwError(() => ({ status: 500 })) })],
};

export const ConfirmDelete: Story = {
  play: async ({ canvasElement }) => {
    const button = await rendered(canvasElement, '[aria-label="Supprimer la variable RUST_LOG"]');
    await userEvent.click(button);
    const dialog = await waitFor(() => {
      const found = canvasElement.ownerDocument.querySelector('gbt-confirm-danger-modal [role="dialog"]');
      if (!found) throw new Error('dialog not rendered yet');
      return found;
    });
    await expect(dialog.getAttribute('aria-label')).toBe('Supprimer la variable');
    await expect(dialog.querySelector('input')).toBeNull();
  },
};
