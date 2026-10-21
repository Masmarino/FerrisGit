import type { Meta, StoryObj } from '@storybook/angular-vite';
import { moduleMetadata } from '@storybook/angular-vite';
import { NEVER, of, throwError } from 'rxjs';
import { RepositoryProtectedBranches } from './repository-protected-branches';
import { ProtectedBranchRule, RepositorySettingsService } from '../repository-settings.service';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { withFerrisgitIcons } from '../../shared/layout/page-story-helpers';
import { expectRows, expectSettingsLayout, fakeToast, inSettingsColumn, rendered } from '../../shared/layout/settings-story-helpers';

const RULES: ProtectedBranchRule[] = [
  { id: 'p1', pattern: 'main', requiredApprovals: 0, requiredChecks: [], createdAt: '2026-01-01T00:00:00Z' },
  { id: 'p2', pattern: 'release/*', requiredApprovals: 0, requiredChecks: [], createdAt: '2026-01-02T00:00:00Z' },
  { id: 'p3', pattern: '*-stable', requiredApprovals: 0, requiredChecks: [], createdAt: '2026-01-03T00:00:00Z' },
];

function fakeRepositorySettingsService(overrides: Record<string, unknown> = {}) {
  return {
    listProtectedBranches: () => of(RULES),
    protectBranch: () => of(RULES[0]),
    unprotectBranch: () => of(undefined),
    ...overrides,
  };
}

const withService = (overrides: Record<string, unknown>) =>
  moduleMetadata({ providers: [{ provide: RepositorySettingsService, useValue: fakeRepositorySettingsService(overrides) }] });

const meta: Meta<RepositoryProtectedBranches> = {
  title: 'Repositories/Settings/RepositoryProtectedBranches',
  component: RepositoryProtectedBranches,
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
type Story = StoryObj<RepositoryProtectedBranches>;

export const Populated: Story = {};

export const Empty: Story = {
  decorators: [withService({ listProtectedBranches: () => of([]) })],
};

export const Loading: Story = {
  decorators: [withService({ listProtectedBranches: () => NEVER })],
  play: async ({ canvasElement }) => {
    await rendered(canvasElement, 'gbt-skeleton-list [role="status"]');
  },
};

export const LoadFailed: Story = {
  decorators: [withService({ listProtectedBranches: () => throwError(() => ({ status: 500 })) })],
};
