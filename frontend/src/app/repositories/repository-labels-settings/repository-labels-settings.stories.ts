import type { Meta, StoryObj } from '@storybook/angular-vite';
import { moduleMetadata } from '@storybook/angular-vite';
import { expect, userEvent, waitFor } from 'storybook/test';
import { NEVER, of, throwError } from 'rxjs';
import { RepositoryLabelsSettings } from './repository-labels-settings';
import { Label, LabelsService } from '../../labels/labels.service';
import { GbtToastService } from '@masmarino/gabarit';
import { withFerrisgitIcons } from '../../shared/layout/page-story-helpers';
import { expectRows, expectSettingsLayout, fakeToast, inSettingsColumn, rendered } from '../../shared/layout/settings-story-helpers';

const LABELS: Label[] = [
  { id: 'l1', name: 'bug', color: '#dc2626', repositoryId: 'repo-1', groupId: null, createdAt: '2026-01-05T09:00:00Z' },
  { id: 'l2', name: 'amélioration', color: '#2563eb', repositoryId: 'repo-1', groupId: null, createdAt: '2026-01-06T09:00:00Z' },
  { id: 'l3', name: 'documentation', color: '#16a34a', repositoryId: 'repo-1', groupId: null, createdAt: '2026-01-10T11:00:00Z' },
  { id: 'l4', name: 'urgent', color: '#ea580c', repositoryId: 'repo-1', groupId: null, createdAt: '2026-02-01T08:30:00Z' },
  { id: 'l5', name: 'bon premier ticket', color: '#7c3aed', repositoryId: 'repo-1', groupId: null, createdAt: '2026-02-14T15:00:00Z' },
  { id: 'l6', name: 'en attente', color: '#6b7280', repositoryId: 'repo-1', groupId: null, createdAt: '2026-03-02T13:20:00Z' },
  { id: 'l7', name: 'jaune', color: '#ca8a04', repositoryId: 'repo-1', groupId: null, createdAt: '2026-03-03T13:20:00Z' },
];

function fakeLabelsService(overrides: Record<string, unknown> = {}) {
  return {
    listForRepository: () => of(LABELS),
    create: () => of(LABELS[0]),
    delete: () => of(undefined),
    ...overrides,
  };
}

const withService = (overrides: Record<string, unknown>) => moduleMetadata({ providers: [{ provide: LabelsService, useValue: fakeLabelsService(overrides) }] });

const meta: Meta<RepositoryLabelsSettings> = {
  title: 'Repositories/Settings/RepositoryLabelsSettings',
  component: RepositoryLabelsSettings,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [
    withFerrisgitIcons,
    moduleMetadata({
      providers: [
        { provide: LabelsService, useValue: fakeLabelsService() },
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
type Story = StoryObj<RepositoryLabelsSettings>;

export const Populated: Story = {};

export const Composing: Story = {
  play: async ({ canvasElement }) => {
    const input = await rendered<HTMLInputElement>(canvasElement, 'form gbt-input input');
    await userEvent.type(input, 'à trier');
    await userEvent.click(canvasElement.querySelector<HTMLInputElement>('input[aria-label="Violet"]')!);
    await waitFor(() => expect(canvasElement.querySelector('.repository-labels-settings__preview gbt-tag')?.textContent?.trim()).toBe('à trier'));
    await expectSettingsLayout(canvasElement);
  },
};

export const Empty: Story = {
  decorators: [withService({ listForRepository: () => of([]) })],
};

export const Loading: Story = {
  decorators: [withService({ listForRepository: () => NEVER })],
  play: async ({ canvasElement }) => {
    await rendered(canvasElement, '[aria-busy="true"]');
  },
};

export const LoadFailed: Story = {
  decorators: [withService({ listForRepository: () => throwError(() => ({ status: 500 })) })],
};
