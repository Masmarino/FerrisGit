import type { Meta, StoryObj } from '@storybook/angular-vite';
import { MrSystemNote } from './mr-system-note';
import { TimelineEvent } from '../merge-requests.service';
import { onTimelineRail } from '../timeline-story-helpers';
import { hoursAgo, withFerrisgitIcons } from '../../shared/layout/page-story-helpers';
import { eventFixture } from '../merge-request-fixtures';

function makeEvent(overrides: Partial<TimelineEvent>): TimelineEvent {
  return eventFixture({ id: 'e1', createdAt: hoursAgo(3), kind: 'closed', ...overrides });
}

const meta: Meta<MrSystemNote> = {
  title: 'MergeRequests/MrSystemNote',
  component: MrSystemNote,
  tags: ['autodocs'],
  decorators: [withFerrisgitIcons, onTimelineRail],
  args: {
    event: makeEvent({ kind: 'review_submitted', payload: { decision: 'approved' } }),
  },
};

export default meta;
type Story = StoryObj<MrSystemNote>;

export const Approved: Story = {};

export const ChangesRequested: Story = {
  args: { event: makeEvent({ kind: 'review_submitted', payload: { decision: 'changes_requested' } }) },
};

export const LabelsAdded: Story = {
  args: { event: makeEvent({ kind: 'labels_changed', payload: { added: [{ id: 'l1', name: 'bug', color: '#d73a4a' }], removed: [] } }) },
};

export const LabelsAddedAndRemoved: Story = {
  args: {
    event: makeEvent({
      kind: 'labels_changed',
      payload: {
        added: [
          { id: 'l1', name: 'bug', color: '#d73a4a' },
          { id: 'l2', name: 'urgent', color: '#e99695' },
        ],
        removed: [{ id: 'l3', name: 'documentation', color: '#0075ca' }],
      },
    }),
  },
};

export const MilestoneChanged: Story = {
  args: { event: makeEvent({ kind: 'milestone_changed', payload: { from: 'v1.0', to: 'v1.1' } }) },
};

export const TitleChanged: Story = {
  args: { event: makeEvent({ kind: 'title_changed', payload: { from: 'Session UTC', to: 'Passer les sessions en DateTime<Utc>' } }) },
};

export const CommitsPushed: Story = {
  args: { event: makeEvent({ kind: 'commits_pushed', payload: { fromSha: '1111111aaaaaaa', toSha: 'a1b2c3d4e5f6a7b8' } }) },
};

export const Merged: Story = {
  args: { event: makeEvent({ kind: 'merged', payload: { mergeCommitSha: 'f00dbabe12345678' } }) },
};

export const Closed: Story = {
  args: { event: makeEvent({ kind: 'closed' }) },
};

export const MergedWithoutActor: Story = {
  args: { event: makeEvent({ actor: null, kind: 'merged', payload: { mergeCommitSha: 'f00dbabe12345678' } }) },
};

export const LabelWithoutColor: Story = {
  args: { event: makeEvent({ kind: 'labels_changed', payload: { added: [{ id: 'l1', name: 'sans-couleur' }], removed: [] } }) },
};

export const OldEvent: Story = {
  args: { event: makeEvent({ kind: 'commits_pushed', createdAt: '2026-01-05T09:30:00Z', payload: { toSha: 'a1b2c3d4e5f6a7b8' } }) },
};
