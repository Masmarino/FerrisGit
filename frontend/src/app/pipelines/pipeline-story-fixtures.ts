// Storybook fixtures shared by the pipeline stories, not imported by the app.
import { JobSummary } from './pipelines.service';
import { StageGroup, groupByStage } from './pipeline-helpers';

export function makeJob(overrides: Partial<JobSummary> = {}): JobSummary {
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
export const NOW = Date.parse('2026-01-01T00:02:00Z');

export function at(seconds: number): string {
  return new Date(START + seconds * 1000).toISOString();
}

export const hello = makeJob({ id: 'job-1', stage: 'prepare', name: 'hello', status: 'success', startedAt: at(2), finishedAt: at(9), logs: 'Clonage du dépôt...\nhello from FerrisGit\n' });
export const appHealth = makeJob({ id: 'job-2', stage: 'prepare', name: 'app-health', status: 'success', startedAt: at(2), finishedAt: at(14), logs: 'GET /health -> 200 OK\napp-health: ok\n' });
export const parallelA = makeJob({ id: 'job-3', stage: 'check', name: 'parallel-a', status: 'success', needs: ['hello'], startedAt: at(16), finishedAt: at(58), logs: 'running 42 tests...\ntest result: ok. 42 passed; 0 failed\n' });

export const runningParallelB = makeJob({ id: 'job-4', stage: 'check', name: 'parallel-b', status: 'running', needs: ['hello', 'app-health'], startedAt: at(16), logs: 'cargo clippy\nchecking ferrisgit v0.1.0\n' });
export const failedParallelB = makeJob({
  id: 'job-4',
  stage: 'check',
  name: 'parallel-b',
  status: 'failed',
  needs: ['hello', 'app-health'],
  startedAt: at(16),
  finishedAt: at(62),
  logs: "cargo clippy\nthread 'main' panicked at src/lib.rs:42\ntest result: FAILED. 3 passed; 1 failed\n",
});
export const successParallelB = makeJob({ id: 'job-4', stage: 'check', name: 'parallel-b', status: 'success', needs: ['hello', 'app-health'], startedAt: at(16), finishedAt: at(71), logs: 'cargo clippy\nno warnings\n' });

export const pendingSummary = makeJob({ id: 'job-5', stage: 'report', name: 'summary', status: 'pending', needs: ['parallel-a', 'parallel-b'] });
export const skippedSummary = makeJob({ id: 'job-5', stage: 'report', name: 'summary', status: 'skipped', needs: ['parallel-a', 'parallel-b'] });
export const successSummary = makeJob({ id: 'job-5', stage: 'report', name: 'summary', status: 'success', needs: ['parallel-a', 'parallel-b'], startedAt: at(73), finishedAt: at(93), logs: 'Tous les contrôles sont passés.\n' });

export const runningGroups: StageGroup[] = groupByStage([hello, appHealth, parallelA, runningParallelB, pendingSummary]);
export const failedGroups: StageGroup[] = groupByStage([hello, appHealth, parallelA, failedParallelB, skippedSummary]);
export const successGroups: StageGroup[] = groupByStage([hello, appHealth, parallelA, successParallelB, successSummary]);
