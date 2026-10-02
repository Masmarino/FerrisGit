import type { Meta, StoryObj } from '@storybook/angular-vite';
import { PipelineJob } from './pipeline-job';
import { groupByStage } from '../pipeline-helpers';
import { NOW, appHealth, at, hello, makeJob, parallelA, pendingSummary, runningGroups, runningParallelB, skippedSummary, successGroups, successSummary } from '../pipeline-story-fixtures';

const LONG_LOGS = Array.from({ length: 120 }, (_, index) => {
  const line = String(index + 1).padStart(3, '0');
  return index === 117
    ? `[${line}] error[E0308]: mismatched types in src/pipelines/runner.rs:42`
    : `[${line}] Compiling ferrisgit-module-${index + 1} v0.1.0`;
})
  .concat(["thread 'main' panicked at src/lib.rs:42", 'test result: FAILED. 3 passed; 1 failed'])
  .join('\n');

const failedWithLongLogs = makeJob({
  id: 'job-4',
  stage: 'check',
  name: 'parallel-b',
  status: 'failed',
  needs: ['hello', 'app-health'],
  startedAt: at(16),
  finishedAt: at(62),
  logs: `${LONG_LOGS}\n`,
});

const meta: Meta<PipelineJob> = {
  title: 'Pipelines/PipelineJob',
  component: PipelineJob,
  tags: ['autodocs'],
  args: {
    job: runningParallelB,
    groups: runningGroups,
    activeStageIndex: 1,
    now: NOW,
  },
};

export default meta;
type Story = StoryObj<PipelineJob>;

export const RunningWithLogs: Story = {};

export const FailedWithLongLogs: Story = {
  args: {
    job: failedWithLongLogs,
    groups: groupByStage([hello, appHealth, parallelA, failedWithLongLogs, skippedSummary]),
  },
};

/** A job that never started because one it depends on failed. */
export const Skipped: Story = {
  args: {
    job: skippedSummary,
    groups: groupByStage([hello, appHealth, parallelA, failedWithLongLogs, skippedSummary]),
    activeStageIndex: 1,
  },
};

export const PendingWithoutLogs: Story = {
  args: { job: pendingSummary, activeStageIndex: 1 },
};

/** The instance's log retention emptied the log of this finished job. */
export const LogPurgedByRetention: Story = {
  args: {
    job: makeJob({ ...successSummary, logs: '', logsPurgedAt: '2026-03-12T12:00:00Z' }),
    groups: successGroups,
    activeStageIndex: 3,
  },
};

export const WithNeeds: Story = {
  args: { job: successSummary, groups: successGroups, activeStageIndex: 3 },
};
