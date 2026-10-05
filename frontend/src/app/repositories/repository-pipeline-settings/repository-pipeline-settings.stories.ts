import type { Meta, StoryObj } from '@storybook/angular-vite';
import { moduleMetadata } from '@storybook/angular-vite';
import { expect, fireEvent, userEvent, waitFor } from 'storybook/test';
import { NEVER, of, switchMap, throwError, timer } from 'rxjs';
import { RepositoryPipelineSettings } from './repository-pipeline-settings';
import { RepositorySettings, RepositorySettingsService } from '../repository-settings.service';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { withFerrisgitIcons } from '../../shared/layout/page-story-helpers';
import { expectSettingsLayout, fakeToast, inSettingsColumn, rendered } from '../../shared/layout/settings-story-helpers';

const ENABLED_SETTINGS: RepositorySettings = { pipelineFilePath: '.ferrisgit-ci.yml', ciEnabled: true, requiredApprovals: 2 };

function fakeRepositorySettingsService(overrides: Record<string, unknown> = {}) {
  let current = { ...ENABLED_SETTINGS };
  return {
    get: () => of(current),
    update: (_id: string, update: Partial<RepositorySettings>) => {
      current = { ...current, ...update };
      return of(current);
    },
    ...overrides,
  };
}

const withService = (overrides: Record<string, unknown>) =>
  moduleMetadata({ providers: [{ provide: RepositorySettingsService, useValue: fakeRepositorySettingsService(overrides) }] });

const meta: Meta<RepositoryPipelineSettings> = {
  title: 'Repositories/Settings/RepositoryPipelineSettings',
  component: RepositoryPipelineSettings,
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
  },
};

export default meta;
type Story = StoryObj<RepositoryPipelineSettings>;

export const Enabled: Story = {};

export const Disabled: Story = {
  decorators: [withService({ get: () => of({ pipelineFilePath: '.ferrisgit-ci.yml', ciEnabled: false, requiredApprovals: 0 }) })],
};

export const SavedAndInvalid: Story = {
  play: async ({ canvasElement }) => {
    const path = await rendered<HTMLInputElement>(canvasElement, '[data-field="pipelineFilePath"] input');
    await userEvent.clear(path);
    await userEvent.type(path, 'ci/pipeline.yml');
    fireEvent.blur(path); // blur() does nothing while the preview frame lacks focus.
    const approvals = canvasElement.querySelector<HTMLInputElement>('[data-field="requiredApprovals"] input')!;
    await userEvent.clear(approvals);
    await userEvent.type(approvals, 'deux');
    fireEvent.blur(approvals);

    await waitFor(() => expect(canvasElement.querySelector('[data-field="pipelineFilePath"] [role="status"]')?.textContent?.trim()).toBe('Enregistré'));
    await waitFor(() =>
      expect(canvasElement.querySelector('[data-field="requiredApprovals"] .gbt-input__error')?.textContent?.trim()).toBe('Entrez un nombre entier, 0 ou plus'),
    );
    await expectSettingsLayout(canvasElement);
  },
};

export const SaveFailed: Story = {
  // Fail a bit later, like a real HTTP error, not synchronously.
  decorators: [withService({ update: () => timer(200).pipe(switchMap(() => throwError(() => ({ status: 500 })))) })],
  play: async ({ canvasElement }) => {
    const path = await rendered<HTMLInputElement>(canvasElement, '[data-field="pipelineFilePath"] input');
    await userEvent.clear(path);
    await userEvent.type(path, 'ci/pipeline.yml');
    fireEvent.blur(path); // blur() does nothing while the preview frame lacks focus.
    await waitFor(() => expect(canvasElement.querySelector('[data-field="pipelineFilePath"] [role="status"]')?.textContent?.trim()).toBe('Non enregistré'));
    const ciSwitch = canvasElement.querySelector<HTMLInputElement>('[data-field="ciEnabled"] input[role="switch"]')!;
    await userEvent.click(ciSwitch);
    await waitFor(() => expect(canvasElement.querySelector('[data-field="ciEnabled"] [role="status"]')?.textContent?.trim()).toBe('Non enregistré'));
    await waitFor(() => expect(ciSwitch.checked).toBe(true));
  },
};

export const Loading: Story = {
  decorators: [withService({ get: () => NEVER })],
  play: async ({ canvasElement }) => {
    await rendered(canvasElement, '[aria-busy="true"]');
  },
};

export const LoadFailed: Story = {
  decorators: [withService({ get: () => throwError(() => ({ status: 500 })) })],
};
