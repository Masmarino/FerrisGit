import type { Meta, StoryObj } from '@storybook/angular-vite';
import { moduleMetadata } from '@storybook/angular-vite';
import { expect, userEvent, waitFor } from 'storybook/test';
import { NEVER, of, throwError } from 'rxjs';
import { RepositoryMilestonesSettings } from './repository-milestones-settings';
import { Milestone, MilestonesService } from '../../milestones/milestones.service';
import { GbtToastService } from '@masmarino/gabarit';
import { withFerrisgitIcons } from '../../shared/layout/page-story-helpers';
import { expectRows, expectSettingsLayout, fakeToast, inSettingsColumn, rendered } from '../../shared/layout/settings-story-helpers';

function dueIn(days: number): string {
  const now = new Date();
  return new Date(Date.UTC(now.getFullYear(), now.getMonth(), now.getDate() + days)).toISOString();
}

function milestone(fields: Partial<Milestone> & Pick<Milestone, 'id' | 'title'>): Milestone {
  return { description: '', dueDate: null, state: 'open', repositoryId: 'repo-1', groupId: null, createdAt: '2026-01-05T09:00:00Z', ...fields };
}

const MILESTONES: Milestone[] = [
  milestone({ id: 'm1', title: 'v1.0 — première version stable', dueDate: dueIn(-120), state: 'closed' }),
  milestone({ id: 'm2', title: 'v1.1 — revue de code', dueDate: dueIn(-6) }),
  milestone({ id: 'm3', title: 'v2.0 — refonte des pipelines', dueDate: dueIn(45) }),
  milestone({ id: 'm4', title: 'Backlog sans échéance' }),
  milestone({
    id: 'm5',
    title: 'Un milestone au titre très long pour vérifier que la ligne le tronque proprement sans pousser les actions hors de la carte',
    dueDate: dueIn(200),
  }),
];

function fakeMilestonesService(overrides: Record<string, unknown> = {}) {
  return {
    listForRepository: () => of(MILESTONES),
    create: () => of(MILESTONES[1]),
    delete: () => of(undefined),
    ...overrides,
  };
}

const withService = (overrides: Record<string, unknown>) => moduleMetadata({ providers: [{ provide: MilestonesService, useValue: fakeMilestonesService(overrides) }] });

const meta: Meta<RepositoryMilestonesSettings> = {
  title: 'Repositories/Settings/RepositoryMilestonesSettings',
  component: RepositoryMilestonesSettings,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [
    withFerrisgitIcons,
    moduleMetadata({
      providers: [
        { provide: MilestonesService, useValue: fakeMilestonesService() },
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
type Story = StoryObj<RepositoryMilestonesSettings>;

export const Populated: Story = {};

export const DatePickerOpen: Story = {
  play: async ({ canvasElement }) => {
    const trigger = await rendered(canvasElement, 'gbt-date-picker button');
    await userEvent.click(trigger);
    await waitFor(() => expect(canvasElement.querySelector('gbt-date-picker [role="dialog"], gbt-date-picker [role="grid"]')).not.toBeNull());
  },
};

export const Empty: Story = {
  decorators: [withService({ listForRepository: () => of([]) })],
};

export const Loading: Story = {
  decorators: [withService({ listForRepository: () => NEVER })],
  play: async ({ canvasElement }) => {
    await rendered(canvasElement, 'gbt-skeleton-list [role="status"]');
    await expect(canvasElement.querySelector('gbt-skeleton-list [role="status"]')?.textContent?.trim()).toBe('Chargement des milestones…');
  },
};

export const LoadFailed: Story = {
  decorators: [withService({ listForRepository: () => throwError(() => ({ status: 500 })) })],
};
