import { BuilderState } from './pipeline-builder-model';
import { DetectedProject, RepositoryProfile } from './pipeline-definitions.service';
import { missingJobs, predictPipeline } from './pipeline-prediction';

const profile = (projects: DetectedProject[], extra: Partial<RepositoryProfile> = {}): RepositoryProfile => ({ projects, dockerfiles: [], helmCharts: [], ...extra });

const node = (overrides: Partial<Extract<DetectedProject, { kind: 'node' }>> = {}): DetectedProject => ({
  kind: 'node',
  dir: '',
  evidence: ['package.json', 'package-lock.json'],
  packageManager: 'npm',
  nodeVersion: null,
  scripts: {},
  framework: null,
  testRunner: null,
  ...overrides,
});

/** This repository, as the server reads it. */
const FERRISGIT = profile(
  [
    { kind: 'rust', dir: '', evidence: ['Cargo.toml', 'rust-toolchain.toml'], workspace: true, toolchain: '1.98.1', sqlxOffline: true },
    node({ dir: 'frontend', evidence: ['frontend/package.json', 'frontend/package-lock.json', 'frontend/angular.json'], nodeVersion: '26', scripts: { build: 'ng build', test: 'ng test' }, framework: 'angular', testRunner: 'vitest' }),
    node({ dir: 'website', evidence: ['website/package.json', 'website/package-lock.json'], nodeVersion: '26', scripts: { build: 'ng build', test: 'ng test --watch=false', lint: 'eslint .' }, framework: 'angular', testRunner: 'vitest' }),
  ],
  { dockerfiles: [''], helmCharts: ['helm/ferrisgit'] },
);

describe('pipeline prediction', () => {
  it('has nothing to propose for a repository where nothing was recognised', () => {
    expect(predictPipeline(null)).toBeNull();
    expect(predictPipeline(profile([]))).toBeNull();
    expect(predictPipeline(profile([node()]))).toBeNull();
  });

  describe('for this repository', () => {
    const prediction = predictPipeline(FERRISGIT)!;
    const job = (name: string) => prediction.state.jobs.find((candidate) => candidate.name === name)!;

    it('names it by what it holds', () => {
      expect(prediction.title).toBe('Rust et Angular');
    });

    it('lays out every project in check, test and build, each job named after its project', () => {
      expect(prediction.state.stages).toEqual(['check', 'test', 'build']);
      expect(prediction.state.jobs.map((j) => `${j.stage}:${j.name}`)).toEqual([
        'check:rust-format',
        'check:rust-clippy',
        'check:website-lint',
        'test:rust-test',
        'test:frontend-test',
        'test:website-test',
        'build:frontend-build',
        'build:website-build',
      ]);
    });

    it('runs Rust on the pinned toolchain, across the workspace, without a database', () => {
      expect(job('rust-clippy')).toMatchObject({ image: 'rust:1.98.1', script: ['rustup component add clippy', 'cargo clippy --workspace --all-targets -- -D warnings'], cache: ['cargo-home', 'cargo-target'] });
      expect(job('rust-test').variables).toContainEqual({ key: 'SQLX_OFFLINE', value: 'true' });
      expect(job('rust-test').needs).toEqual(['rust-format', 'rust-clippy']);
    });

    it('works in each app folder, with its Node version, and tells ng test to run once', () => {
      expect(job('frontend-test')).toMatchObject({ image: 'node:26', script: ['cd frontend', 'npm ci', 'npm test -- --watch=false'], needs: [] });
      expect(job('website-test').script.at(-1)).toBe('npm test');
      expect(job('website-test').needs).toEqual(['website-lint']);
      expect(job('frontend-build').needs).toEqual(['frontend-test']);
      expect(job('frontend-build').variables).toContainEqual({ key: 'CI', value: 'true' });
    });

    it('says what each project was read from, and points at what it ships', () => {
      expect(prediction.reasons[0]).toEqual({
        evidence: ['Cargo.toml', 'rust-toolchain.toml'],
        text: "Un workspace Rust à la racine. La version 1.98.1, épinglée par le dépôt, donne l'image rust:1.98.1. Le dossier .sqlx permet de compiler sans base de données : SQLX_OFFLINE=true.",
      });
      expect(prediction.reasons[1].text).toBe("Une application Angular dans frontend, avec npm. Node 26 d'après package.json. Scripts repris : test et build.");
      expect(prediction.notes).toEqual([expect.stringMatching(/^Un Dockerfile à la racine : la tuile « Construire et publier une image Docker »/), expect.stringMatching(/^Un chart Helm dans helm\/ferrisgit/)]);
    });
  });

  it('keeps the plain job names when there is one project', () => {
    const prediction = predictPipeline(profile([{ kind: 'go', dir: '', evidence: ['go.mod'], goVersion: '1.23' }]))!;

    expect(prediction.title).toBe('Go');
    expect(prediction.state.jobs.map((j) => j.name)).toEqual(['format', 'vet', 'test', 'build']);
    expect(prediction.state.jobs[0]).toMatchObject({ image: 'golang:1.23', script: ['test -z "$(gofmt -l .)"'] });
    expect(prediction.state.jobs[2].needs).toEqual(['format', 'vet']);
  });

  it("installs and runs with the project's own package manager", () => {
    const lines = (manager: Extract<DetectedProject, { kind: 'node' }>['packageManager']) =>
      predictPipeline(profile([node({ packageManager: manager, evidence: ['package.json', 'x.lock'], scripts: { lint: 'eslint .' } })]))!.state.jobs[0].script;

    expect(lines('pnpm')).toEqual(['corepack enable', 'pnpm install --frozen-lockfile', 'pnpm run lint']);
    expect(lines('yarnClassic')).toEqual(['yarn install --frozen-lockfile', 'yarn lint']);
    expect(lines('yarn')).toEqual(['corepack enable', 'yarn install --immutable', 'yarn lint']);
    expect(lines('bun')).toEqual(['bun install --frozen-lockfile', 'bun run lint']);
    expect(predictPipeline(profile([node({ evidence: ['package.json'], scripts: { lint: 'eslint .' } })]))!.state.jobs[0].script).toEqual(['npm install', 'npm run lint']);
  });

  it('leaves out the test script npm init writes, and the scripts that are not there', () => {
    const prediction = predictPipeline(profile([node({ scripts: { test: 'echo "Error: no test specified" && exit 1', build: 'tsc' } })]))!;

    expect(prediction.state.jobs.map((j) => j.name)).toEqual(['build']);
    expect(prediction.reasons[0].text).toContain('Scripts repris : build.');
  });

  it('warns that Karma needs a browser the node image does not have', () => {
    const prediction = predictPipeline(profile([node({ scripts: { test: 'ng test' }, framework: 'angular', testRunner: 'karma' })]))!;

    expect(prediction.notes[0]).toContain('Karma, qui demande un navigateur');
  });

  it('runs Python with uv or Poetry when the project uses them', () => {
    const prediction = predictPipeline(profile([{ kind: 'python', dir: 'ml', evidence: ['ml/pyproject.toml', 'ml/uv.lock'], tool: 'uv', pythonVersion: '3.12', pytest: true, ruff: true }]))!;

    expect(prediction.state.jobs.map((j) => [j.name, j.image, j.script.at(-1)])).toEqual([
      ['lint', 'python:3.12', 'uv run ruff check .'],
      ['test', 'python:3.12', 'uv run python -m pytest'],
    ]);
    expect(prediction.state.jobs[1].script.slice(0, 3)).toEqual(['cd ml', 'pip install uv', 'uv sync --frozen']);
  });

  it('gives two folders of the same name jobs of their own', () => {
    const prediction = predictPipeline(profile([node({ dir: 'apps/web', scripts: { build: 'vite build' } }), node({ dir: 'packages/web', scripts: { build: 'tsc' } })]))!;

    expect(prediction.state.jobs.map((j) => j.name)).toEqual(['web-build', 'web-2-build']);
  });

  describe('what a pipeline still lacks', () => {
    const prediction = predictPipeline(FERRISGIT)!;

    it('is every predicted job for an empty pipeline', () => {
      expect(missingJobs(prediction, { stages: ['build'], jobs: [] })).toHaveLength(8);
    });

    it('counts a job that does the same in the same folder, whatever its options, and only there', () => {
      const state: BuilderState = {
        stages: ['test'],
        jobs: [
          { name: 'checks', stage: 'test', image: 'rust:1', script: ['cargo clippy -- -D warnings'], variables: [], needs: [], tags: [], cache: [] },
          { name: 'bundle', stage: 'test', image: 'node:22', script: ['npm ci', 'npm run build'], variables: [], needs: [], tags: [], cache: [] },
          { name: 'site', stage: 'test', image: 'node:22', script: ['cd website', 'npm ci', 'npm test'], variables: [], needs: [], tags: [], cache: [] },
        ],
      };

      const missing = missingJobs(prediction, state).map((predicted) => predicted.job.name);

      expect(missing).not.toContain('rust-clippy');
      expect(missing).not.toContain('website-test');
      expect(missing).toContain('frontend-build');
      expect(missing).toContain('frontend-test');
    });

    it('leaves out the jobs the pipeline has, by name or by what they run', () => {
      const state: BuilderState = {
        stages: ['test'],
        jobs: [
          { name: 'rust-test', stage: 'test', image: 'rust:1', script: ['cargo test'], variables: [], needs: [], tags: [], cache: [] },
          { name: 'lint', stage: 'test', image: 'rust:1', script: ['cargo clippy --workspace --all-targets -- -D warnings'], variables: [], needs: [], tags: [], cache: [] },
        ],
      };

      expect(missingJobs(prediction, state).map((predicted) => predicted.job.name)).not.toContain('rust-test');
      expect(missingJobs(prediction, state).map((predicted) => predicted.job.name)).not.toContain('rust-clippy');
      expect(missingJobs(prediction, state).map((predicted) => predicted.job.name)).toContain('rust-format');
      expect(missingJobs(null, state)).toEqual([]);
    });
  });
});
