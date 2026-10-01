import type { Meta, StoryObj } from '@storybook/angular-vite';
import { PipelineJob } from './pipeline-job';
import { JobSummary } from '../pipelines.service';
import { StageGroup, groupByStage } from '../pipeline-helpers';

function makeJob(overrides: Partial<JobSummary> = {}): JobSummary {
  return {
    id: 'job-1',
    stage: 'prepare',
    name: 'hello',
    status: 'pending',
    needs: [],
    tags: [],
    logs: '',
    createdAt: '2026-01-01T00:00:00Z',
    startedAt: null,
    finishedAt: null,
    ...overrides,
  };
}

const START = Date.parse('2026-01-01T00:00:00Z');
const NOW = Date.parse('2026-01-01T00:02:00Z');

function at(seconds: number): string {
  return new Date(START + seconds * 1000).toISOString();
}

const hello = makeJob({ id: 'job-1', stage: 'prepare', name: 'hello', status: 'success', startedAt: at(2), finishedAt: at(9), logs: 'Clonage du dépôt...\nhello from FerrisGit\n' });
const appHealth = makeJob({ id: 'job-2', stage: 'prepare', name: 'app-health', status: 'success', startedAt: at(2), finishedAt: at(14), logs: 'GET /health -> 200 OK\napp-health: ok\n' });
const parallelA = makeJob({ id: 'job-3', stage: 'check', name: 'parallel-a', status: 'success', needs: ['hello'], startedAt: at(16), finishedAt: at(58), logs: 'running 42 tests...\ntest result: ok. 42 passed; 0 failed\n' });

const runningParallelB = makeJob({ id: 'job-4', stage: 'check', name: 'parallel-b', status: 'running', needs: ['hello', 'app-health'], startedAt: at(16), logs: 'cargo clippy\nchecking ferrisgit v0.1.0\n' });
const successParallelB = makeJob({ id: 'job-4', stage: 'check', name: 'parallel-b', status: 'success', needs: ['hello', 'app-health'], startedAt: at(16), finishedAt: at(71), logs: 'cargo clippy\nno warnings\n' });

const pendingSummary = makeJob({ id: 'job-5', stage: 'report', name: 'summary', status: 'pending', needs: ['parallel-a', 'parallel-b'] });
const canceledSummary = makeJob({ id: 'job-5', stage: 'report', name: 'summary', status: 'canceled', needs: ['parallel-a', 'parallel-b'] });
const successSummary = makeJob({ id: 'job-5', stage: 'report', name: 'summary', status: 'success', needs: ['parallel-a', 'parallel-b'], startedAt: at(73), finishedAt: at(93), logs: 'Tous les contrôles sont passés.\n' });

const runningGroups: StageGroup[] = groupByStage([hello, appHealth, parallelA, runningParallelB, pendingSummary]);
const successGroups: StageGroup[] = groupByStage([hello, appHealth, parallelA, successParallelB, successSummary]);

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
    groups: groupByStage([hello, appHealth, parallelA, failedWithLongLogs, canceledSummary]),
  },
};

export const PendingWithoutLogs: Story = {
  args: { job: pendingSummary, activeStageIndex: 1 },
};

export const WithNeeds: Story = {
  args: { job: successSummary, groups: successGroups, activeStageIndex: 3 },
};
