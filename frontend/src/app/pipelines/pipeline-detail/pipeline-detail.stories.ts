import type { Meta, StoryObj } from '@storybook/angular-vite';
import { applicationConfig, moduleMetadata } from '@storybook/angular-vite';
import { ActivatedRoute, convertToParamMap, provideRouter, withDisabledInitialNavigation } from '@angular/router';
import { of } from 'rxjs';
import { PipelineDetail } from './pipeline-detail';
import { JobSummary, PipelineDetail as PipelineDetailModel, PipelinesService } from '../pipelines.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { inShellContentArea, withFerrisgitIcons } from '../../shared/layout/page-story-helpers';

function makeJob(overrides: Partial<JobSummary> = {}): JobSummary {
  return {
    id: 'job-1',
    stage: 'build',
    name: 'compile',
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

function makePipeline(overrides: Partial<PipelineDetailModel> = {}): PipelineDetailModel {
  return {
    id: '3f2a9c1e-7b4d-4e21-9a0b-5c6d7e8f9a0b',
    commitSha: 'abcdef1234567890abcdef1234567890abcdef12',
    status: 'running',
    createdAt: '2026-01-01T00:00:00Z',
    finishedAt: null,
    triggeredBy: null,
    commitMessage: null,
    error: null,
    jobs: [],
    ...overrides,
  };
}

const BASE = Date.parse('2026-01-01T10:00:00Z');

function at(seconds: number): string {
  return new Date(BASE + seconds * 1000).toISOString();
}

const HELLO_LOGS = 'Cloning into workspace...\nhello from FerrisGit\n';
const HEALTH_LOGS = 'GET /health -> 200 OK\napp-health: ok\n';

const successPipeline = makePipeline({
  status: 'success',
  commitMessage: 'Mettre en cache les avatars',
  triggeredBy: { id: 'u1', username: 'alice' },
  createdAt: at(0),
  finishedAt: at(95),
  jobs: [
    makeJob({ id: 'job-1', stage: 'prepare', name: 'hello', status: 'success', startedAt: at(2), finishedAt: at(9), logs: HELLO_LOGS }),
    makeJob({ id: 'job-2', stage: 'prepare', name: 'app-health', status: 'success', startedAt: at(2), finishedAt: at(14), logs: HEALTH_LOGS }),
    makeJob({ id: 'job-3', stage: 'check', name: 'parallel-a', status: 'success', needs: ['hello'], startedAt: at(16), finishedAt: at(58), logs: 'running 42 tests...\ntest result: ok. 42 passed; 0 failed\n' }),
    makeJob({ id: 'job-4', stage: 'check', name: 'parallel-b', status: 'success', needs: ['hello', 'app-health'], startedAt: at(16), finishedAt: at(71), logs: 'cargo clippy\nno warnings\n' }),
    makeJob({ id: 'job-5', stage: 'report', name: 'summary', status: 'success', needs: ['parallel-a', 'parallel-b'], startedAt: at(73), finishedAt: at(93), logs: 'All checks passed.\n' }),
  ],
});

const runningPipeline = (() => {
  const now = Date.now();
  const ago = (seconds: number) => new Date(now - seconds * 1000).toISOString();
  return makePipeline({
    status: 'running',
    commitMessage: 'Ajouter la connexion via SSO',
    triggeredBy: { id: 'u2', username: 'bastien' },
    createdAt: ago(80),
    finishedAt: null,
    jobs: [
      makeJob({ id: 'job-1', stage: 'prepare', name: 'hello', status: 'success', startedAt: ago(78), finishedAt: ago(71), logs: HELLO_LOGS }),
      makeJob({ id: 'job-2', stage: 'prepare', name: 'app-health', status: 'success', startedAt: ago(78), finishedAt: ago(66), logs: HEALTH_LOGS }),
      makeJob({ id: 'job-3', stage: 'check', name: 'parallel-a', status: 'success', needs: ['hello'], startedAt: ago(64), finishedAt: ago(22), logs: 'running 42 tests...\ntest result: ok. 42 passed; 0 failed\n' }),
      makeJob({ id: 'job-4', stage: 'check', name: 'parallel-b', status: 'running', needs: ['hello', 'app-health'], startedAt: ago(64), logs: 'cargo clippy\nchecking ferrisgit v0.1.0\n' }),
      makeJob({ id: 'job-5', stage: 'report', name: 'summary', status: 'pending', needs: ['parallel-a', 'parallel-b'], logs: '' }),
    ],
  });
})();

const failedPipeline = makePipeline({
  status: 'failed',
  commitMessage: 'Corriger la pagination quand on filtre par label, et garder le filtre dans l’URL pour pouvoir partager la vue filtrée',
  createdAt: at(0),
  finishedAt: at(64),
  jobs: [
    makeJob({ id: 'job-1', stage: 'prepare', name: 'hello', status: 'success', startedAt: at(2), finishedAt: at(9), logs: HELLO_LOGS }),
    makeJob({ id: 'job-2', stage: 'prepare', name: 'app-health', status: 'success', startedAt: at(2), finishedAt: at(14), logs: HEALTH_LOGS }),
    makeJob({ id: 'job-3', stage: 'check', name: 'parallel-a', status: 'success', needs: ['hello'], startedAt: at(16), finishedAt: at(58), logs: 'running 42 tests...\ntest result: ok. 42 passed; 0 failed\n' }),
    makeJob({
      id: 'job-4',
      stage: 'check',
      name: 'parallel-b',
      status: 'failed',
      needs: ['hello', 'app-health'],
      startedAt: at(16),
      finishedAt: at(62),
      logs: "cargo clippy\nthread 'main' panicked at src/lib.rs:42\ntest result: FAILED. 3 passed; 1 failed\n",
    }),
    makeJob({ id: 'job-5', stage: 'report', name: 'summary', status: 'skipped', needs: ['parallel-a', 'parallel-b'], logs: '' }),
  ],
});

const invalidPipeline = makePipeline({
  status: 'failed',
  commitMessage: 'Ajouter une étape de déploiement',
  triggeredBy: { id: 'u1', username: 'alice' },
  createdAt: at(0),
  finishedAt: at(0),
  error: "job 'deploy' needs 'build', but that job's stage does not come before 'deploy''s own stage",
  jobs: [],
});

const invalidYamlPipeline = makePipeline({
  ...invalidPipeline,
  error: 'invalid YAML: jobs.compile: missing field `image` at line 4 column 3',
});

function fakePipelinesService(pipeline: PipelineDetailModel) {
  return {
    detail: () => of(pipeline),
    cancel: () => of(undefined),
  };
}

function fakeRepositoryContext(role: 'owner' | 'reader' | 'contributor' | 'maintainer' | null) {
  return {
    current: () => (role === null ? null : { repositoryId: 'repo-1', path: ['acme', 'widget'], role, ancestors: [], groupId: null }),
  };
}

const meta: Meta<PipelineDetail> = {
  title: 'Pipelines/PipelineDetail',
  component: PipelineDetail,
  tags: ['autodocs'],
  parameters: { layout: 'fullscreen' },
  decorators: [withFerrisgitIcons, inShellContentArea],
  args: {
    repositoryId: 'repo-1',
    pipelineId: 'pipeline-1',
    path: ['acme', 'widget'],
  },
};

export default meta;
type Story = StoryObj<PipelineDetail>;

export const Running: Story = {
  decorators: [
    applicationConfig({ providers: [provideRouter([], withDisabledInitialNavigation())] }),
    moduleMetadata({
      providers: [
        { provide: PipelinesService, useValue: fakePipelinesService(runningPipeline) },
        { provide: RepositoryContextService, useValue: fakeRepositoryContext('contributor') },
      ],
    }),
  ],
};

export const Success: Story = {
  decorators: [
    applicationConfig({ providers: [provideRouter([], withDisabledInitialNavigation())] }),
    moduleMetadata({
      providers: [
        { provide: PipelinesService, useValue: fakePipelinesService(successPipeline) },
        { provide: RepositoryContextService, useValue: fakeRepositoryContext('contributor') },
      ],
    }),
  ],
};

export const Failed: Story = {
  decorators: [
    applicationConfig({ providers: [provideRouter([], withDisabledInitialNavigation())] }),
    moduleMetadata({
      providers: [
        { provide: PipelinesService, useValue: fakePipelinesService(failedPipeline) },
        { provide: RepositoryContextService, useValue: fakeRepositoryContext('contributor') },
      ],
    }),
  ],
};

/** A pipeline file that does not parse: the pipeline failed at once, without job, and says why. */
export const InvalidPipelineFile: Story = {
  decorators: [
    applicationConfig({ providers: [provideRouter([], withDisabledInitialNavigation())] }),
    moduleMetadata({
      providers: [
        { provide: PipelinesService, useValue: fakePipelinesService(invalidPipeline) },
        { provide: RepositoryContextService, useValue: fakeRepositoryContext('contributor') },
      ],
    }),
  ],
};

export const InvalidYaml: Story = {
  decorators: [
    applicationConfig({ providers: [provideRouter([], withDisabledInitialNavigation())] }),
    moduleMetadata({
      providers: [
        { provide: PipelinesService, useValue: fakePipelinesService(invalidYamlPipeline) },
        { provide: RepositoryContextService, useValue: fakeRepositoryContext('contributor') },
      ],
    }),
  ],
};

/** The failed job, and the job behind it that never started: "Ignoré". */
export const SkippedJob: Story = {
  args: { jobId: 'job-5' },
  decorators: Failed.decorators,
};

export const StageSelected: Story = {
  decorators: [
    applicationConfig({ providers: [provideRouter([], withDisabledInitialNavigation())] }),
    moduleMetadata({
      providers: [
        { provide: PipelinesService, useValue: fakePipelinesService(failedPipeline) },
        { provide: RepositoryContextService, useValue: fakeRepositoryContext('contributor') },
        { provide: ActivatedRoute, useValue: { queryParamMap: of(convertToParamMap({ stage: 'check' })) } },
      ],
    }),
  ],
};

export const JobPage: Story = {
  args: { jobId: 'job-4' },
  decorators: Failed.decorators,
};

export const JobNotFound: Story = {
  args: { jobId: 'missing' },
  decorators: Failed.decorators,
};
