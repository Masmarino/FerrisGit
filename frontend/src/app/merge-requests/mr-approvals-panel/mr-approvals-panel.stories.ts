import type { Meta, StoryObj } from '@storybook/angular-vite';
import { componentWrapperDecorator, moduleMetadata } from '@storybook/angular-vite';
import { MrApprovalsPanel } from './mr-approvals-panel';
import { Review, ReviewSummary } from '../merge-requests.service';
import { withFerrisgitIcons } from '../../shared/layout/page-story-helpers';
import { Panel } from '@masmarino/gabarit/panel';

function makeReview(overrides: Partial<Review>): Review {
  return { userId: 'u1', username: 'alice', decision: 'approved', stale: false, createdAt: '2026-09-23T10:00:00Z', ...overrides };
}

const needsApprovals: ReviewSummary = {
  reviews: [makeReview({ userId: 'u1', username: 'alice' })],
  requiredApprovals: 2,
  liveApprovalCount: 1,
  blocked: true,
};

const changesRequested: ReviewSummary = {
  reviews: [
    makeReview({ userId: 'u1', username: 'alice' }),
    makeReview({ userId: 'u2', username: 'bob', decision: 'changes_requested', createdAt: '2026-09-23T11:30:00Z' }),
  ],
  requiredApprovals: 2,
  liveApprovalCount: 1,
  blocked: true,
};

const satisfied: ReviewSummary = {
  reviews: [
    makeReview({ userId: 'u1', username: 'alice' }),
    makeReview({ userId: 'u3', username: 'carol', createdAt: '2026-09-23T12:00:00Z' }),
    makeReview({ userId: 'u2', username: 'bob', decision: 'changes_requested', stale: true, createdAt: '2026-09-22T16:00:00Z' }),
  ],
  requiredApprovals: 2,
  liveApprovalCount: 2,
  blocked: false,
};

const meta: Meta<MrApprovalsPanel> = {
  title: 'MergeRequests/MrApprovalsPanel',
  component: MrApprovalsPanel,
  tags: ['autodocs'],
  decorators: [
    withFerrisgitIcons,
    moduleMetadata({ imports: [Panel] }),
    componentWrapperDecorator(
      (story) => `<div style="width: 300px; max-width: 100%; padding: 1rem; background: var(--bg-panel);"><gbt-panel heading="Approbations" [headingLevel]="2">${story}</gbt-panel></div>`,
    ),
  ],
  args: {
    summary: needsApprovals,
    status: 'open',
    canWrite: true,
  },
};

export default meta;
type Story = StoryObj<MrApprovalsPanel>;

export const Open_NeedsApprovals: Story = {};

export const Open_ChangesRequested: Story = {
  args: { summary: changesRequested },
};

export const Open_Satisfied: Story = {
  args: { summary: satisfied },
};

export const ReadOnly: Story = {
  args: { summary: needsApprovals, canWrite: false },
};

export const Merged: Story = {
  args: { summary: satisfied, status: 'merged' },
};
