import type { Meta, StoryObj } from '@storybook/angular-vite';
import { expect, waitFor } from 'storybook/test';
import { StatusBadge } from './status-badge';
import { withFerrisgitIcons } from '../page-story-helpers';

const ISSUE_STATUSES = ['todo', 'in_progress', 'in_review', 'done'];
const MERGE_REQUEST_STATUSES = ['open', 'merged', 'closed'];
const PIPELINE_STATUSES = ['pending', 'running', 'success', 'failed', 'canceled'];
const RELEASE_STATUSES = ['draft', 'prerelease', 'published'];

const row = 'display: flex; flex-wrap: wrap; align-items: center; gap: 0.5rem; margin: 0 0 1rem;';
const caption = 'width: 9rem; color: var(--text-secondary); font-size: 0.8125rem;';

async function expectIcons({ canvasElement }: { canvasElement: HTMLElement }) {
  const icons = await waitFor(() => {
    const found = Array.from(canvasElement.querySelectorAll('fg-status-badge gbt-icon'));
    if (found.length === 0) throw new Error('badges not rendered yet');
    return found;
  });
  for (const icon of icons) {
    const label = icon.closest('fg-status-badge')?.textContent?.trim();
    await expect(icon.querySelector('svg'), `the "${label}" badge's icon is registered`).not.toBeNull();
  }
}

const meta: Meta<StatusBadge> = {
  title: 'Shared/Layout/StatusBadge',
  component: StatusBadge,
  tags: ['autodocs'],
  decorators: [withFerrisgitIcons],
  argTypes: {
    kind: { control: 'inline-radio', options: ['issue', 'merge-request', 'pipeline', 'release'] },
    status: {
      control: 'select',
      options: [...new Set([...ISSUE_STATUSES, ...MERGE_REQUEST_STATUSES, ...PIPELINE_STATUSES, ...RELEASE_STATUSES]), 'blocked'],
    },
  },
  args: { kind: 'issue', status: 'in_progress' },
};

export default meta;
type Story = StoryObj<StatusBadge>;

export const Playground: Story = { play: expectIcons };

export const AllStatuses: Story = {
  play: expectIcons,
  render: () => ({
    props: {
      issueStatuses: ISSUE_STATUSES,
      mergeRequestStatuses: MERGE_REQUEST_STATUSES,
      pipelineStatuses: PIPELINE_STATUSES,
      releaseStatuses: RELEASE_STATUSES,
    },
    template: `
      <div style="${row}">
        <span style="${caption}">Tickets</span>
        @for (status of issueStatuses; track status) {
          <fg-status-badge kind="issue" [status]="status" />
        }
      </div>
      <div style="${row}">
        <span style="${caption}">Demandes de fusion</span>
        @for (status of mergeRequestStatuses; track status) {
          <fg-status-badge kind="merge-request" [status]="status" />
        }
      </div>
      <div style="${row}">
        <span style="${caption}">Pipelines</span>
        @for (status of pipelineStatuses; track status) {
          <fg-status-badge kind="pipeline" [status]="status" />
        }
      </div>
      <div style="${row}">
        <span style="${caption}">Releases</span>
        @for (status of releaseStatuses; track status) {
          <fg-status-badge kind="release" [status]="status" />
        }
      </div>`,
  }),
};

export const UnknownStatus: Story = {
  play: expectIcons,
  render: () => ({
    template: `
      <div style="${row}">
        <fg-status-badge kind="issue" status="blocked" />
        <fg-status-badge kind="merge-request" status="draft" />
        <fg-status-badge kind="pipeline" status="skipped" />
        <fg-status-badge kind="release" status="archived" />
      </div>`,
  }),
};

export const InAHeader: Story = {
  play: expectIcons,
  render: () => ({
    template: `
      <div style="display: flex; flex-wrap: wrap; align-items: center; gap: 0.75rem;">
        <h1 style="margin: 0; color: var(--text-primary); font-size: 1.5rem; font-weight: 600;">Paginer la liste des tickets côté serveur</h1>
        <fg-status-badge kind="merge-request" status="merged" />
      </div>
      <p style="margin: 0.5rem 0 0; color: var(--text-secondary); font-size: 0.875rem;">
        <fg-status-badge kind="issue" status="in_review" />
        &nbsp;alice a ouvert ce ticket il y a 2 j
      </p>`,
  }),
};
