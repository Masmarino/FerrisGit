import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig } from '@storybook/angular-vite';
import { provideRouter, withDisabledInitialNavigation } from '@angular/router';
import { PipelineSummary } from './pipeline-summary';
import { NOW, failedGroups, runningGroups, successGroups } from '../pipeline-story-fixtures';

const meta: Meta<PipelineSummary> = {
  title: 'Pipelines/PipelineSummary',
  component: PipelineSummary,
  tags: ['autodocs'],
  decorators: [applicationConfig({ providers: [provideRouter([], withDisabledInitialNavigation())] })],
  args: {
    path: ['acme', 'widget'],
    pipelineId: 'pipeline-1',
    groups: runningGroups,
    activeStageIndex: 1,
    selectedStage: null,
    now: NOW,
  },
};

export default meta;
type Story = StoryObj<PipelineSummary>;

export const Running: Story = {};

export const Failed: Story = {
  args: { groups: failedGroups, activeStageIndex: 1 },
};

export const StageSelected: Story = {
  args: { groups: failedGroups, activeStageIndex: 1, selectedStage: 'check' },
};

export const AllSucceeded: Story = {
  args: { groups: successGroups, activeStageIndex: 3 },
};

export const EmptyPipeline: Story = {
  args: { groups: [], activeStageIndex: 0 },
};
