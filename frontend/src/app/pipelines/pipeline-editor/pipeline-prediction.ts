import { BuilderJob, BuilderState, commandsOf, uniqueName } from './pipeline-builder-model';
import { GO, NODE, PYTHON, RUST } from './pipeline-recipes';
import { shQuote } from './pipeline-tiles-deploy';
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
/** A folder as a shell reads it: as it is when it is a plain path, quoted when it holds a space or a shell character. */
const shellPath = (dir: string) => (/^[A-Za-z0-9._/-]+$/.test(dir) ? dir : shQuote(dir));
const inDir = (dir: string, script: string[]) => (dir === '' ? script : [`cd ${shellPath(dir)}`, ...script]);
const slug = (text: string) => text.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'app';
const list = (items: string[]) => (items.length <= 1 ? items.join('') : `${items.slice(0, -1).join(', ')} et ${items[items.length - 1]}`);

function rustPlan(project: Extract<DetectedProject, { kind: 'rust' }>): ProjectPlan {
  const image = RUST.image(project.toolchain);
  const variables = { ...RUST.cache.variables, ...(project.sqlxOffline ? { SQLX_OFFLINE: 'true' } : {}) };
  const cache = [...RUST.cache.cache];
  const pinned = project.evidence.some((file) => file.endsWith('rust-toolchain.toml') || file.endsWith('rust-toolchain'));
  const version = project.toolchain ? ` La version ${project.toolchain}${pinned ? ', épinglée par le dépôt,' : ' demandée par le manifeste'} donne l'image ${image}.` : ` Sans version demandée, l'image est ${image}, la dernière stable.`;
  const sqlx =
    (project.sqlxOffline ? ' Le dossier .sqlx permet de compiler sans base de données : SQLX_OFFLINE=true.' : '') +
    (project.sqlxPostgres ? ' Ses tests sqlx demandent PostgreSQL : le job de tests en démarre un.' : '');
  // A job has no service beside it: tests that need PostgreSQL get one started in their own container (a Debian image).
  const database: { setup: string[]; variables: Record<string, string> } = project.sqlxPostgres
    ? {
        setup: ['apt-get update -qq', 'apt-get install -y -qq postgresql > /dev/null', 'pg_ctlcluster "$(ls /etc/postgresql)" main start', `runuser -u postgres -- psql -q -c "ALTER USER postgres PASSWORD 'postgres'"`],
        variables: { DATABASE_URL: 'postgres://postgres:postgres@localhost:5432/postgres' },
      }
    : { setup: [], variables: {} };
  return {
    label: project.dir === '' ? 'Rust' : project.dir,
    prefix: project.dir === '' ? 'rust' : slug(project.dir.split('/').pop()!),
    reason: { evidence: project.evidence, text: `${project.workspace ? 'Un workspace Rust' : 'Un projet Rust'} ${where(project.dir)}.${version}${sqlx}` },
    notes: [],
    drafts: [
      { role: 'format', title: 'Vérifier le format', image, script: inDir(project.dir, RUST.format()) },
      { role: 'clippy', title: 'Analyser avec Clippy', image, script: inDir(project.dir, RUST.clippy(project.workspace)), variables, cache },
      { role: 'test', title: 'Lancer les tests', image, script: inDir(project.dir, [...database.setup, ...RUST.test(project.workspace)]), variables: { ...variables, ...database.variables }, cache },
    ],
  };
}

const FRAMEWORK_NAMES = { angular: 'Angular', react: 'React', vue: 'Vue', svelte: 'Svelte', next: 'Next.js' } as const;
const MANAGER_NAMES: Record<PackageManager, string> = { npm: 'npm', pnpm: 'pnpm', yarnClassic: 'Yarn 1', yarn: 'Yarn', bun: 'Bun' };
/** The scripts that check formatting without changing anything, as projects usually name them. */
const FORMAT_SCRIPTS = ['format:check', 'check:format', 'prettier:check', 'lint:format', 'fmt:check'];
/** What `npm init` writes in `test`: a script that only fails. */
const isPlaceholderTest = (command: string) => command.includes('no test specified');

function nodePlan(project: Extract<DetectedProject, { kind: 'node' }>): ProjectPlan {
  const manager = project.packageManager;
  const image = NODE.image(manager, project.nodeVersion);
  const locked = project.evidence.some((file) => /(package-lock\.json|pnpm-lock\.yaml|yarn\.lock|bun\.lockb?)$/.test(file));
  const install = NODE.install(manager, locked);
  const cache = NODE.cache(manager);
  // Test runners run once instead of watching when CI is set.
  const variables = { ...cache.variables, CI: 'true' };
  const scripts = project.scripts;
  const test = scripts['test'];
  const runsTests = test !== undefined && !isPlaceholderTest(test);
  const format = FORMAT_SCRIPTS.find((name) => name in scripts);
  // The scripts the project has, in the order their jobs run; `ng test` is told to run once.
  const picked: { role: Role; title: string; script: string; args?: string }[] = [
    ...(format ? [{ role: 'format' as const, title: 'Vérifier le format', script: format }] : []),
    ...('lint' in scripts ? [{ role: 'lint' as const, title: 'Analyser le code', script: 'lint' }] : []),
    ...(runsTests ? [{ role: 'test' as const, title: 'Lancer les tests', script: 'test', args: /\bng test\b/.test(test) && !test.includes('--watch') ? '--watch=false' : '' }] : []),
    ...('build' in scripts ? [{ role: 'build' as const, title: 'Construire', script: 'build' }] : []),
  ];
  const drafts = picked.map(({ role, title, script, args }): Draft => ({ role, title, image, script: inDir(project.dir, [...install, NODE.run(script, manager, args)]), variables, cache: cache.cache }));
  const used = picked.map(({ script }) => script);
  const notes =
    runsTests && project.testRunner === 'karma'
      ? [`Les tests ${project.dir === '' ? 'du projet' : `de ${project.dir}`} passent par Karma, qui demande un navigateur : l'image ${image} n'en contient pas. Choisissez une image avec Chrome, ou passez à Vitest.`]
      : [];
  return {
    label: project.dir === '' ? (project.framework ? FRAMEWORK_NAMES[project.framework] : 'Node') : project.dir,
    prefix: project.dir === '' ? 'node' : slug(project.dir.split('/').pop()!),
    reason: { evidence: project.evidence, text: nodeReason(project, image, used) },
    notes,
    drafts,
  };
}

/** What was read of a Node project, and what was taken from it. */
function nodeReason(project: Extract<DetectedProject, { kind: 'node' }>, image: string, used: string[]): string {
  const what = project.framework ? `Une application ${FRAMEWORK_NAMES[project.framework]}` : 'Un projet Node';
  const versionSource = project.evidence.find((file) => file.endsWith('.nvmrc') || file.endsWith('.node-version'))?.split('/').pop() ?? 'package.json';
  const version =
    project.packageManager === 'bun' ? " L'image est oven/bun:1." : project.nodeVersion ? ` Node ${project.nodeVersion} d'après ${versionSource}.` : ` Sans version de Node demandée, l'image est ${image}.`;
  const taken = used.length > 0 ? ` Scripts repris : ${list(used)}.` : ' Aucun script à reprendre (lint, test, build) : rien à lancer pour lui.';
  return `${what} ${where(project.dir)}, avec ${MANAGER_NAMES[project.packageManager]}.${version}${taken}`;
}

function goPlan(project: Extract<DetectedProject, { kind: 'go' }>): ProjectPlan {
  const image = GO.image(project.goVersion);
  return {
    label: project.dir === '' ? 'Go' : project.dir,
    prefix: project.dir === '' ? 'go' : slug(project.dir.split('/').pop()!),
    reason: { evidence: project.evidence, text: `Un module Go ${where(project.dir)}.${project.goVersion ? ` Go ${project.goVersion} d'après go.mod, image ${image}.` : ` Sans version dans go.mod, l'image est ${image}.`}` },
    notes: [],
    drafts: [
      { role: 'format', title: 'Vérifier le format', image, script: inDir(project.dir, GO.format()) },
      { role: 'vet', title: 'Analyser avec go vet', image, script: inDir(project.dir, GO.vet()) },
      { role: 'test', title: 'Lancer les tests', image, script: inDir(project.dir, GO.test()) },
      { role: 'build', title: 'Compiler', image, script: inDir(project.dir, GO.build()) },
    ],
  };
}

function pythonPlan(project: Extract<DetectedProject, { kind: 'python' }>): ProjectPlan {
  const image = PYTHON.image(project.pythonVersion);
  const requirements = project.evidence.some((file) => file.endsWith('requirements.txt'));
  const setup = PYTHON.setup(project.tool, requirements);
  const drafts: Draft[] = [];
  if (project.ruff) {
    drafts.push({ role: 'lint', title: 'Analyser avec Ruff', image, script: inDir(project.dir, project.tool === 'pip' ? ['pip install ruff', 'ruff check .'] : [...setup, PYTHON.exec('ruff check .', project.tool)]) });
  }
  if (project.pytest) {
    drafts.push({ role: 'test', title: 'Lancer les tests', image, script: inDir(project.dir, [...setup, PYTHON.exec('python -m pytest', project.tool)]) });
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
/**
 * The jobs a project's plan makes. They are named after the project when there are several (`frontend-test`), each
 * name free of those `taken`, which it adds to.
 */
function jobsOfPlan(plan: ProjectPlan, prefix: string | null, taken: string[]): PredictedJob[] {
  const names = new Map<Role, string>();
  for (const draft of plan.drafts) {
    const name = uniqueName(taken, prefix ? `${prefix}-${draft.role}` : draft.role);
    taken.push(name);
    names.set(draft.role, name);
  }
  return plan.drafts.map((draft) => ({
    project: plan.label,
    title: draft.title,
    job: {
      name: names.get(draft.role)!,
      stage: STAGE_OF[draft.role],
      image: draft.image,
      script: [...draft.script],
      variables: Object.entries(draft.variables ?? {}).map(([key, value]) => ({ key, value })),
      needs: waitsFor(plan, draft).map((other) => names.get(other.role)!),
      tags: [],
      cache: [...(draft.cache ?? [])],
    },
  }));
}

/** The same project's jobs of the closest earlier stage that has some: a broken format stops the tests. */
function waitsFor(plan: ProjectPlan, draft: Draft): Draft[] {
  const earlier = PREDICTED_STAGES.slice(0, PREDICTED_STAGES.indexOf(STAGE_OF[draft.role])).reverse();
  return earlier.map((stage) => plan.drafts.filter((other) => STAGE_OF[other.role] === stage)).find((found) => found.length > 0) ?? [];
}

export function predictPipeline(profile: RepositoryProfile | null): Prediction | null {
  if (!profile || profile.projects.length === 0) {
    return null;
  }
  const plans = profile.projects.map(planOf);
  const taken: string[] = [];
  const prefixes: string[] = [];
  const jobs = plans.flatMap((plan) => {
    if (plans.length === 1) {
      return jobsOfPlan(plan, null, taken);
    }
    // Two folders of the same name (apps/web, packages/web) still get jobs of their own: web-…, web-2-….
    const prefix = uniqueName(prefixes, plan.prefix);
    prefixes.push(prefix);
    return jobsOfPlan(plan, prefix, taken);
  });
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

/** The folder a job works in: its leading `cd`, or the root, written alike whether quoted, with `./` or a final `/`. */
const folderOf = (job: BuilderJob) => {
  const dir = commandsOf(job).find((line) => line.startsWith('cd '))?.slice(3).trim() ?? '';
  return dir.replace(/^'(.*)'$/, '$1').replace(/^\.\/+/, '').replace(/\/+$/, '').replace(/^\.$/, '');
};

/** What a command does, its options aside: `cargo clippy --workspace -- -D warnings` is `cargo clippy`. */
const intentOf = (line: string) => {
  const words = line.trim().split(/\s+/);
  const options = words.findIndex((word) => word.startsWith('-'));
  return (options === -1 ? words : words.slice(0, options)).join(' ');
};

/** Each command a job runs, as what it does in which folder. */
const intentsOf = (job: BuilderJob) => commandsOf(job).filter((line) => !line.startsWith('cd ')).map((line) => `${folderOf(job)}|${intentOf(line)}`);

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
