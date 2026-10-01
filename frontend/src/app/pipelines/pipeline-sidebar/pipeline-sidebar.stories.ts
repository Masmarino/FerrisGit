import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig } from '@storybook/angular-vite';
import { expect } from 'storybook/test';
import { provideRouter, withDisabledInitialNavigation } from '@angular/router';
import { PipelineSidebar } from './pipeline-sidebar';
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
const failedParallelB = makeJob({
  id: 'job-4',
  stage: 'check',
  name: 'parallel-b',
  status: 'failed',
  needs: ['hello', 'app-health'],
  startedAt: at(16),
  finishedAt: at(62),
  logs: "cargo clippy\nthread 'main' panicked at src/lib.rs:42\ntest result: FAILED. 3 passed; 1 failed\n",
});

const pendingSummary = makeJob({ id: 'job-5', stage: 'report', name: 'summary', status: 'pending', needs: ['parallel-a', 'parallel-b'] });
const canceledSummary = makeJob({ id: 'job-5', stage: 'report', name: 'summary', status: 'canceled', needs: ['parallel-a', 'parallel-b'] });

const runningGroups: StageGroup[] = groupByStage([hello, appHealth, parallelA, runningParallelB, pendingSummary]);
const failedGroups: StageGroup[] = groupByStage([hello, appHealth, parallelA, failedParallelB, canceledSummary]);

const meta: Meta<PipelineSidebar> = {
  title: 'Pipelines/PipelineSidebar',
  component: PipelineSidebar,
  tags: ['autodocs'],
  decorators: [applicationConfig({ providers: [provideRouter([], withDisabledInitialNavigation())] })],
  args: {
    path: ['acme', 'widget'],
    pipelineId: 'pipeline-1',
    groups: runningGroups,
    selectedJobId: null,
    now: NOW,
  },
};

export default meta;
type Story = StoryObj<PipelineSidebar>;

export const Running: Story = {
  play: async ({ canvasElement }) => {
    const glyphs = Array.from(canvasElement.querySelectorAll('.pipeline-sidebar__link gbt-job-status'));
    await expect(glyphs.map((glyph) => glyph.getAttribute('data-status'))).toEqual(['success', 'success', 'success', 'running', 'pending']);
    await expect(glyphs.map((glyph) => glyph.querySelector('.sr-only')?.textContent?.trim())).toEqual(['Réussi', 'Réussi', 'Réussi', 'En cours', 'En attente']);
  },
};

export const Failed: Story = {
  args: { groups: failedGroups },
  play: async ({ canvasElement }) => {
    const glyphs = Array.from(canvasElement.querySelectorAll('.pipeline-sidebar__link gbt-job-status'));
    await expect(glyphs.map((glyph) => glyph.getAttribute('data-status'))).toEqual(['success', 'success', 'success', 'failed', 'canceled']);
    await expect(glyphs.map((glyph) => glyph.querySelector('.sr-only')?.textContent?.trim())).toEqual(['Réussi', 'Réussi', 'Réussi', 'Échoué', 'Annulé']);
  },
};

export const JobSelected: Story = {
  args: { groups: failedGroups, selectedJobId: 'job-4' },
};
