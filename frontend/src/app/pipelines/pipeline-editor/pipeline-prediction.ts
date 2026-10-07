import { BuilderJob, BuilderState, uniqueName } from './pipeline-builder-model';
import { NPM_CACHE, RUST_CACHE } from './pipeline-catalog';
import { DetectedProject, PackageManager, RepositoryProfile } from './pipeline-definitions.service';

/**
 * A pipeline made for one repository, from what the server read in it: one set of jobs per project, with the images,
 * tools and scripts the project itself names, and why. Only what works without a secret is laid out: an image to
 * publish or a deployment is pointed at instead, with the tile that does it.
 */

/** The order the jobs run in: what fails fast and cheap first. */
export const PREDICTED_STAGES = ['check', 'test', 'build'] as const;
type Stage = (typeof PREDICTED_STAGES)[number];

/** What a job is for in its project; jobs of a project wait for those of the earlier stages. */
type Role = 'format' | 'lint' | 'clippy' | 'vet' | 'test' | 'build';

const STAGE_OF: Record<Role, Stage> = { format: 'check', lint: 'check', clippy: 'check', vet: 'check', test: 'test', build: 'build' };

/** One project, as the proposal explains it: the files it was read from, then what was made of them. */
export interface PredictionReason {
  evidence: string[];
  text: string;
}

export interface PredictedJob {
  job: BuilderJob;
  /** The project, as a person names it: `Rust`, `frontend`. */
  project: string;
  /** What the job does, for the tile picker. */
  title: string;
}

export interface Prediction {
  /** What the repository is, in a few words: « Rust et Angular ». */
  title: string;
  reasons: PredictionReason[];
  /** What was seen but not laid out (an image to publish, a chart), and what will need a hand. */
  notes: string[];
  jobs: PredictedJob[];
  state: BuilderState;
}

interface Draft {
  role: Role;
  title: string;
  image: string;
  script: string[];
  variables?: Record<string, string>;
  cache?: string[];
}

interface ProjectPlan {
  label: string;
  prefix: string;
  reason: PredictionReason;
  notes: string[];
  drafts: Draft[];
}

const where = (dir: string) => (dir === '' ? 'à la racine' : `dans ${dir}`);
const inDir = (dir: string, script: string[]) => (dir === '' ? script : [`cd ${dir}`, ...script]);
const slug = (text: string) => text.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'app';
const list = (items: string[]) => (items.length <= 1 ? items.join('') : `${items.slice(0, -1).join(', ')} et ${items[items.length - 1]}`);

function rustPlan(project: Extract<DetectedProject, { kind: 'rust' }>): ProjectPlan {
  const image = project.toolchain ? `rust:${project.toolchain}` : 'rust:1';
  const all = project.workspace ? ' --workspace' : '';
  const variables = { ...RUST_CACHE.variables, ...(project.sqlxOffline ? { SQLX_OFFLINE: 'true' } : {}) };
  const cache = [...RUST_CACHE.cache];
  const pinned = project.evidence.some((file) => file.endsWith('rust-toolchain.toml') || file.endsWith('rust-toolchain'));
  const version = project.toolchain ? ` La version ${project.toolchain}${pinned ? ', épinglée par le dépôt,' : ' demandée par le manifeste'} donne l'image ${image}.` : ` Sans version demandée, l'image est ${image}, la dernière stable.`;
  const sqlx = project.sqlxOffline ? ' Le dossier .sqlx permet de compiler sans base de données : SQLX_OFFLINE=true.' : '';
  return {
    label: project.dir === '' ? 'Rust' : project.dir,
    prefix: project.dir === '' ? 'rust' : slug(project.dir.split('/').pop()!),
    reason: { evidence: project.evidence, text: `${project.workspace ? 'Un workspace Rust' : 'Un projet Rust'} ${where(project.dir)}.${version}${sqlx}` },
    notes: [],
    drafts: [
      { role: 'format', title: 'Vérifier le format', image, script: inDir(project.dir, ['rustup component add rustfmt', 'cargo fmt --all -- --check']) },
      { role: 'clippy', title: 'Analyser avec Clippy', image, script: inDir(project.dir, ['rustup component add clippy', `cargo clippy${all} --all-targets -- -D warnings`]), variables, cache },
      { role: 'test', title: 'Lancer les tests', image, script: inDir(project.dir, [`cargo test${all} --all-targets`]), variables, cache },
    ],
  };
}

/** Where each package manager keeps what it downloads, in a cache the next job finds (Kubernetes). */
const PACKAGE_CACHES: Record<PackageManager, { cache: string[]; variables: Record<string, string> }> = {
  npm: NPM_CACHE,
  pnpm: { cache: ['pnpm-store'], variables: { npm_config_store_dir: '/ferrisgit-cache/pnpm-store' } },
  yarnClassic: { cache: ['yarn'], variables: { YARN_CACHE_FOLDER: '/ferrisgit-cache/yarn' } },
  yarn: { cache: ['yarn'], variables: { YARN_CACHE_FOLDER: '/ferrisgit-cache/yarn' } },
  bun: { cache: ['bun'], variables: { BUN_INSTALL_CACHE_DIR: '/ferrisgit-cache/bun' } },
};

const FRAMEWORK_NAMES = { angular: 'Angular', react: 'React', vue: 'Vue', svelte: 'Svelte', next: 'Next.js' } as const;
const MANAGER_NAMES: Record<PackageManager, string> = { npm: 'npm', pnpm: 'pnpm', yarnClassic: 'Yarn 1', yarn: 'Yarn', bun: 'Bun' };
/** The scripts that check formatting without changing anything, as projects usually name them. */
const FORMAT_SCRIPTS = ['format:check', 'check:format', 'prettier:check', 'lint:format', 'fmt:check'];
/** What `npm init` writes in `test`: a script that only fails. */
const isPlaceholderTest = (command: string) => command.includes('no test specified');

function nodePlan(project: Extract<DetectedProject, { kind: 'node' }>): ProjectPlan {
  const manager = project.packageManager;
  const image = manager === 'bun' ? 'oven/bun:1' : `node:${project.nodeVersion ?? '22'}`;
  const locked = project.evidence.some((file) => /(package-lock\.json|pnpm-lock\.yaml|yarn\.lock|bun\.lockb?)$/.test(file));
  const install: string[] = {
    npm: [locked ? 'npm ci' : 'npm install'],
    pnpm: ['corepack enable', 'pnpm install --frozen-lockfile'],
    yarnClassic: ['yarn install --frozen-lockfile'],
    yarn: ['corepack enable', 'yarn install --immutable'],
    bun: ['bun install --frozen-lockfile'],
  }[manager];
  const run = (script: string, args = '') => {
    const extra = args === '' ? '' : manager === 'npm' ? ` -- ${args}` : ` ${args}`;
    if (manager === 'yarnClassic' || manager === 'yarn') {
      return `yarn ${script}${extra}`;
    }
    return script === 'test' && manager !== 'bun' ? `${manager} test${extra}` : `${manager} run ${script}${extra}`;
  };
  const cache = PACKAGE_CACHES[manager];
  // Test runners run once instead of watching when CI is set; `ng test` also needs to be told.
  const variables = { ...cache.variables, CI: 'true' };
  const scripts = project.scripts;
  const job = (role: Role, title: string, line: string): Draft => ({ role, title, image, script: inDir(project.dir, [...install, line]), variables, cache: cache.cache });

  const drafts: Draft[] = [];
  const used: string[] = [];
  const format = FORMAT_SCRIPTS.find((name) => name in scripts);
  if (format) {
    drafts.push(job('format', 'Vérifier le format', run(format)));
    used.push(format);
  }
  if ('lint' in scripts) {
    drafts.push(job('lint', "Analyser le code", run('lint')));
    used.push('lint');
  }
  const notes: string[] = [];
  const test = scripts['test'];
  if (test !== undefined && !isPlaceholderTest(test)) {
    const watchFlag = /\bng test\b/.test(test) && !test.includes('--watch') ? '--watch=false' : '';
    drafts.push(job('test', 'Lancer les tests', run('test', watchFlag)));
    used.push('test');
    if (project.testRunner === 'karma') {
      notes.push(`Les tests ${project.dir === '' ? 'du projet' : `de ${project.dir}`} passent par Karma, qui demande un navigateur : l'image ${image} n'en contient pas. Choisissez une image avec Chrome, ou passez à Vitest.`);
    }
  }
  if ('build' in scripts) {
    drafts.push(job('build', 'Construire', run('build')));
    used.push('build');
  }
  const label = project.dir === '' ? (project.framework ? FRAMEWORK_NAMES[project.framework] : 'Node') : project.dir;
  const what = project.framework ? `Une application ${FRAMEWORK_NAMES[project.framework]}` : 'Un projet Node';
  const versionSource = project.evidence.find((file) => file.endsWith('.nvmrc') || file.endsWith('.node-version'))?.split('/').pop() ?? 'package.json';
  const version = manager === 'bun' ? " L'image est oven/bun:1." : project.nodeVersion ? ` Node ${project.nodeVersion} d'après ${versionSource}.` : ` Sans version de Node demandée, l'image est ${image}.`;
  const taken = used.length > 0 ? ` Scripts repris : ${list(used)}.` : ' Aucun script à reprendre (lint, test, build) : rien à lancer pour lui.';
  return {
    label,
    prefix: project.dir === '' ? 'node' : slug(project.dir.split('/').pop()!),
    reason: { evidence: project.evidence, text: `${what} ${where(project.dir)}, avec ${MANAGER_NAMES[manager]}.${version}${taken}` },
    notes,
    drafts,
  };
}

function goPlan(project: Extract<DetectedProject, { kind: 'go' }>): ProjectPlan {
  const image = `golang:${project.goVersion ?? '1.23'}`;
  return {
    label: project.dir === '' ? 'Go' : project.dir,
    prefix: project.dir === '' ? 'go' : slug(project.dir.split('/').pop()!),
    reason: { evidence: project.evidence, text: `Un module Go ${where(project.dir)}.${project.goVersion ? ` Go ${project.goVersion} d'après go.mod, image ${image}.` : ` Sans version dans go.mod, l'image est ${image}.`}` },
    notes: [],
    drafts: [
      { role: 'format', title: 'Vérifier le format', image, script: inDir(project.dir, ['test -z "$(gofmt -l .)"']) },
      { role: 'vet', title: 'Analyser avec go vet', image, script: inDir(project.dir, ['go vet ./...']) },
      { role: 'test', title: 'Lancer les tests', image, script: inDir(project.dir, ['go test ./...']) },
      { role: 'build', title: 'Compiler', image, script: inDir(project.dir, ['go build ./...']) },
    ],
  };
}

function pythonPlan(project: Extract<DetectedProject, { kind: 'python' }>): ProjectPlan {
  const image = `python:${project.pythonVersion ?? '3.13'}`;
  const requirements = project.evidence.some((file) => file.endsWith('requirements.txt'));
  const setup = { uv: ['pip install uv', 'uv sync --frozen'], poetry: ['pip install poetry', 'poetry install'], pip: [requirements ? 'pip install -r requirements.txt' : 'pip install -e .'] }[project.tool];
  const exec = { uv: 'uv run ', poetry: 'poetry run ', pip: '' }[project.tool];
  const drafts: Draft[] = [];
  if (project.ruff) {
    drafts.push({ role: 'lint', title: 'Analyser avec Ruff', image, script: inDir(project.dir, project.tool === 'pip' ? ['pip install ruff', 'ruff check .'] : [...setup, `${exec}ruff check .`]) });
  }
  if (project.pytest) {
    drafts.push({ role: 'test', title: 'Lancer les tests', image, script: inDir(project.dir, [...setup, `${exec}python -m pytest`]) });
  }
  const tools = [project.ruff ? 'Ruff' : '', project.pytest ? 'pytest' : ''].filter(Boolean);
  const manager = { uv: 'uv', poetry: 'Poetry', pip: 'pip' }[project.tool];
  return {
    label: project.dir === '' ? 'Python' : project.dir,
    prefix: project.dir === '' ? 'python' : slug(project.dir.split('/').pop()!),
    reason: {
      evidence: project.evidence,
      text: `Un projet Python ${where(project.dir)}, avec ${manager}.${project.pythonVersion ? ` Python ${project.pythonVersion}, image ${image}.` : ` Sans version demandée, l'image est ${image}.`}${tools.length > 0 ? ` ${list(tools)} repris.` : ' Ni pytest ni Ruff mentionnés : rien à lancer pour lui.'}`,
    },
    notes: [],
    drafts,
  };
}

function planOf(project: DetectedProject): ProjectPlan {
  switch (project.kind) {
    case 'rust':
      return rustPlan(project);
    case 'node':
      return nodePlan(project);
    case 'go':
      return goPlan(project);
    case 'python':
      return pythonPlan(project);
  }
}

/** The title of the proposal: the kinds of projects, each once, in the order found. */
function titleOf(profile: RepositoryProfile): string {
  const kinds = profile.projects.map((project) => {
    switch (project.kind) {
      case 'rust':
        return 'Rust';
      case 'node':
        return project.framework ? FRAMEWORK_NAMES[project.framework] : 'Node';
      case 'go':
        return 'Go';
      case 'python':
        return 'Python';
    }
  });
  return list([...new Set(kinds)]);
}

/** What the repository ships besides its code, and the tile that would take it further. */
function shippingNotes(profile: RepositoryProfile): string[] {
  const notes: string[] = [];
  for (const dir of profile.dockerfiles) {
    notes.push(`Un Dockerfile ${where(dir)} : la tuile « Construire et publier une image Docker », à l'étape package, peut le publier. Elle demande un démon Docker distant et l'accès au registre.`);
  }
  for (const dir of profile.helmCharts) {
    notes.push(`Un chart Helm ${where(dir)} : la tuile « Déployer avec Helm » peut l'installer sur un cluster, avec le secret KUBE_CONFIG.`);
  }
  return notes;
}

/** The pipeline for this repository, or `null` when nothing in it was recognised. */
export function predictPipeline(profile: RepositoryProfile | null): Prediction | null {
  if (!profile || profile.projects.length === 0) {
    return null;
  }
  const plans = profile.projects.map(planOf);
  const several = plans.length > 1;
  const jobs: PredictedJob[] = [];
  const taken: string[] = [];
  const prefixes: string[] = [];
  for (const plan of plans) {
    // Two folders of the same name (apps/web, packages/web) still get jobs of their own: web-…, web-2-….
    const prefix = uniqueName(prefixes, plan.prefix);
    prefixes.push(prefix);
    const names = new Map<Role, string>();
    for (const draft of plan.drafts) {
      const name = uniqueName(taken, several ? `${prefix}-${draft.role}` : draft.role);
      taken.push(name);
      names.set(draft.role, name);
    }
    for (const draft of plan.drafts) {
      const stage = STAGE_OF[draft.role];
      // A job waits for the same project's jobs of the stage just before it that exist: a broken format stops the tests.
      const earlier = PREDICTED_STAGES.slice(0, PREDICTED_STAGES.indexOf(stage));
      const before = [...earlier].reverse().map((previous) => plan.drafts.filter((other) => STAGE_OF[other.role] === previous)).find((found) => found.length > 0) ?? [];
      jobs.push({
        project: plan.label,
        title: draft.title,
        job: {
          name: names.get(draft.role)!,
          stage,
          image: draft.image,
          script: [...draft.script],
          variables: Object.entries(draft.variables ?? {}).map(([key, value]) => ({ key, value })),
          needs: before.map((other) => names.get(other.role)!),
          tags: [],
          cache: [...(draft.cache ?? [])],
        },
      });
    }
  }
  if (jobs.length === 0) {
    return null;
  }
  const stages = PREDICTED_STAGES.filter((stage) => jobs.some((predicted) => predicted.job.stage === stage));
  const ordered = stages.flatMap((stage) => jobs.filter((predicted) => predicted.job.stage === stage));
  return {
    title: titleOf(profile),
    reasons: plans.map((plan) => plan.reason),
    notes: [...plans.flatMap((plan) => plan.notes), ...shippingNotes(profile)],
    jobs: ordered,
    state: { stages: [...stages], jobs: ordered.map((predicted) => predicted.job) },
  };
}

/** The folder a job works in: its leading `cd`, or the root. */
const folderOf = (job: BuilderJob) => job.script.find((line) => line.trim().startsWith('cd '))?.trim().slice(3).trim() ?? '';

/** What a command does, its options aside: `cargo clippy --workspace -- -D warnings` is `cargo clippy`. */
const intentOf = (line: string) => {
  const words = line.trim().split(/\s+/);
  const options = words.findIndex((word) => word.startsWith('-'));
  return (options === -1 ? words : words.slice(0, options)).join(' ');
};

/** Each command a job runs, as what it does in which folder. */
const intentsOf = (job: BuilderJob) => job.script.filter((line) => line.trim() !== '' && !line.trim().startsWith('cd ')).map((line) => `${folderOf(job)}|${intentOf(line)}`);

/**
 * The predicted jobs the pipeline does not have yet: none of its jobs bears the name or does what the predicted one is
 * for (its last command) in the same folder. Options do not count: a `cargo clippy` with other flags is still Clippy.
 */
export function missingJobs(prediction: Prediction | null, state: BuilderState): PredictedJob[] {
  if (!prediction) {
    return [];
  }
  const names = new Set(state.jobs.map((job) => job.name));
  const intents = new Set(state.jobs.flatMap(intentsOf));
  return prediction.jobs.filter((predicted) => !names.has(predicted.job.name) && !intents.has(intentsOf(predicted.job).at(-1) ?? ''));
}
