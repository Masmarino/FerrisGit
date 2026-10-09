import { BuilderJob, BuilderState, insertJob, uniqueName } from './pipeline-builder-model';
import { GO, NODE, PYTHON, RUST } from './pipeline-recipes';
import { deployTiles } from './pipeline-tiles-deploy';
import { JobTile, ParamValues, TileCategory, buildTile } from './pipeline-tile-types';
import { perLanguage, t } from '../../shared/i18n/translator';

export type { JobTile, ParamValue, ParamValues, TileBuild, TileCategory, TileParam } from './pipeline-tile-types';

export interface TileCategoryInfo {
  id: TileCategory;
  title: string;
  summary: string;
}

export const tileCategories = perLanguage((): readonly TileCategoryInfo[] => [
  { id: 'compile', title: t('pipelines.categories.compile.title'), summary: t('pipelines.categories.compile.summary') },
  { id: 'test', title: t('pipelines.categories.test.title'), summary: t('pipelines.categories.test.summary') },
  { id: 'quality', title: t('pipelines.categories.quality.title'), summary: t('pipelines.categories.quality.summary') },
  { id: 'package', title: t('pipelines.categories.package.title'), summary: t('pipelines.categories.package.summary') },
  { id: 'deploy', title: t('pipelines.categories.deploy.title'), summary: t('pipelines.categories.deploy.summary') },
  { id: 'custom', title: t('pipelines.categories.custom.title'), summary: t('pipelines.categories.custom.summary') },
]);

const RUST_CACHE = RUST.cache;
const NPM_CACHE = NODE.cache();
const npmScript = (script: string) => [...NODE.install(), NODE.run(script)];

const simpleTiles = (): readonly JobTile[] => [
  {
    id: 'rust-build',
    category: 'compile',
    title: t('pipelines.tiles.rust-build.title'),
    summary: t('pipelines.tiles.rust-build.summary'),
    help: t('pipelines.tiles.rust-build.help'),
    icon: 'layers',
    jobName: 'compile',
    stage: 'build',
    image: RUST.image(),
    script: RUST.build(),
    needsSource: true,
    ...RUST_CACHE,
  },
  {
    id: 'node-build',
    category: 'compile',
    title: t('pipelines.tiles.node-build.title'),
    summary: t('pipelines.tiles.node-build.summary'),
    help: t('pipelines.tiles.node-build.help'),
    icon: 'layers',
    jobName: 'build',
    stage: 'build',
    image: NODE.image(),
    script: npmScript('build'),
    needsSource: true,
    ...NPM_CACHE,
  },
  {
    id: 'go-build',
    category: 'compile',
    title: t('pipelines.tiles.go-build.title'),
    summary: t('pipelines.tiles.go-build.summary'),
    help: t('pipelines.tiles.go-build.help'),
    icon: 'layers',
    jobName: 'build',
    stage: 'build',
    image: GO.image(),
    script: GO.build(),
    needsSource: true,
  },
  {
    id: 'rust-test',
    category: 'test',
    title: t('pipelines.tiles.rust-test.title'),
    summary: t('pipelines.tiles.rust-test.summary'),
    help: t('pipelines.tiles.rust-test.help'),
    icon: 'flask-conical',
    jobName: 'test',
    stage: 'test',
    image: RUST.image(),
    script: RUST.test(),
    needsSource: true,
    ...RUST_CACHE,
  },
  {
    id: 'node-test',
    category: 'test',
    title: t('pipelines.tiles.node-test.title'),
    summary: t('pipelines.tiles.node-test.summary'),
    help: t('pipelines.tiles.node-test.help'),
    icon: 'flask-conical',
    jobName: 'unit-tests',
    stage: 'test',
    image: NODE.image(),
    script: npmScript('test'),
    needsSource: true,
    ...NPM_CACHE,
  },
  {
    id: 'go-test',
    category: 'test',
    title: t('pipelines.tiles.go-test.title'),
    summary: t('pipelines.tiles.go-test.summary'),
    help: t('pipelines.tiles.go-test.help'),
    icon: 'flask-conical',
    jobName: 'test',
    stage: 'test',
    image: GO.image(),
    script: GO.test(),
    needsSource: true,
  },
  {
    id: 'python-test',
    category: 'test',
    title: t('pipelines.tiles.python-test.title'),
    summary: t('pipelines.tiles.python-test.summary'),
    help: t('pipelines.tiles.python-test.help'),
    icon: 'flask-conical',
    jobName: 'test',
    stage: 'test',
    image: PYTHON.image(),
    script: [...PYTHON.setup(), PYTHON.exec('python -m pytest')],
    needsSource: true,
  },
  {
    id: 'rust-format',
    category: 'quality',
    title: t('pipelines.tiles.rust-format.title'),
    summary: t('pipelines.tiles.rust-format.summary'),
    help: t('pipelines.tiles.rust-format.help'),
    icon: 'shield-check',
    jobName: 'format',
    stage: 'check',
    image: RUST.image(),
    script: RUST.format(),
    needsSource: true,
  },
  {
    id: 'rust-clippy',
    category: 'quality',
    title: t('pipelines.tiles.rust-clippy.title'),
    summary: t('pipelines.tiles.rust-clippy.summary'),
    help: t('pipelines.tiles.rust-clippy.help'),
    icon: 'shield-check',
    jobName: 'clippy',
    stage: 'check',
    image: RUST.image(),
    script: RUST.clippy(),
    needsSource: true,
    ...RUST_CACHE,
  },
  {
    id: 'node-lint',
    category: 'quality',
    title: t('pipelines.tiles.node-lint.title'),
    summary: t('pipelines.tiles.node-lint.summary'),
    help: t('pipelines.tiles.node-lint.help'),
    icon: 'shield-check',
    jobName: 'lint',
    stage: 'check',
    image: NODE.image(),
    script: npmScript('lint'),
    needsSource: true,
    ...NPM_CACHE,
  },
  {
    id: 'http-deploy',
    category: 'deploy',
    title: t('pipelines.tiles.http-deploy.title'),
    summary: t('pipelines.tiles.http-deploy.summary'),
    help: t('pipelines.tiles.http-deploy.help'),
    icon: 'send',
    jobName: 'deploy',
    stage: 'deploy',
    image: 'alpine:3.20',
    script: ['apk add --no-cache curl', `curl -fsS -X POST -H "Authorization: Bearer $DEPLOY_TOKEN" "$DEPLOY_URL"`],
    tags: ['deploy'],
    secrets: ['DEPLOY_URL', 'DEPLOY_TOKEN'],
    needsSecrets: true,
  },
  {
    id: 'http-notify',
    category: 'deploy',
    title: t('pipelines.tiles.http-notify.title'),
    summary: t('pipelines.tiles.http-notify.summary'),
    help: t('pipelines.tiles.http-notify.help'),
    icon: 'send',
    jobName: 'notify',
    stage: 'deploy',
    image: 'alpine:3.20',
    script: ['apk add --no-cache curl', `curl -fsS -X POST -H "Content-Type: application/json" -d '{"text":"Pipeline terminée"}' "$NOTIFY_URL"`],
    secrets: ['NOTIFY_URL'],
    needsSecrets: true,
  },
  {
    id: 'custom',
    category: 'custom',
    title: t('pipelines.tiles.custom.title'),
    summary: t('pipelines.tiles.custom.summary'),
    help: t('pipelines.tiles.custom.help'),
    icon: 'sparkles',
    jobName: 'job',
    stage: 'build',
    image: 'alpine:3.20',
    script: ['echo "Bonjour"'],
  },
];

/**
 * The catalogue. Within a category, the order is the order shown: deployments to a machine or a cluster come before the
 * plain HTTP calls.
 */
export const jobTiles = perLanguage((): readonly JobTile[] => {
  const simple = simpleTiles();
  return [
    ...simple.filter((tile) => tile.category !== 'deploy' && tile.category !== 'custom'),
    ...deployTiles(),
    ...simple.filter((tile) => tile.category === 'deploy'),
    ...simple.filter((tile) => tile.category === 'custom'),
  ];
});

export const tileById = (id: string): JobTile | undefined => jobTiles().find((tile) => tile.id === id);

/** The job a tile makes, with a name that does not collide. `values` answers the tile's questions, when it has some. */
export function jobFromTile(tile: JobTile, stage: string, taken: readonly string[], values: ParamValues = {}): BuilderJob {
  const built = buildTile(tile, values);
  return {
    name: uniqueName([...taken], tile.jobName),
    stage,
    image: built.image,
    script: [...built.script],
    variables: Object.entries(built.variables ?? {}).map(([key, value]) => ({ key, value })),
    needs: [],
    tags: [...(built.tags ?? [])],
    cache: [...(built.cache ?? [])],
  };
}

/** Adds a tile's job at the end of a stage. */
export function addTile(state: BuilderState, tile: JobTile, stage: string, values: ParamValues = {}): { state: BuilderState; job: BuilderJob } {
  const job = jobFromTile(tile, stage, state.jobs.map((existing) => existing.name), values);
  return { state: insertJob(state, job), job };
}

/** A whole pipeline to start from: its stages, and for each job the tile it comes from and what it waits for. */
export interface PipelineTemplate {
  id: string;
  title: string;
  summary: string;
  icon: string;
  stages: string[];
  jobs: { tile: string; name?: string; stage: string; needs?: string[] }[];
}

export const pipelineTemplates = perLanguage((): readonly PipelineTemplate[] => [
  {
    id: 'rust',
    title: t('pipelines.templates.rust.title'),
    summary: t('pipelines.templates.rust.summary'),
    icon: 'layers',
    stages: ['check', 'test'],
    jobs: [
      { tile: 'rust-format', stage: 'check' },
      { tile: 'rust-clippy', stage: 'check' },
      { tile: 'rust-test', stage: 'test', needs: ['format', 'clippy'] },
    ],
  },
  {
    id: 'node',
    title: t('pipelines.templates.node.title'),
    summary: t('pipelines.templates.node.summary'),
    icon: 'layers',
    stages: ['check', 'build'],
    jobs: [
      { tile: 'node-lint', stage: 'check' },
      { tile: 'node-test', stage: 'check' },
      { tile: 'node-build', stage: 'build', needs: ['lint', 'unit-tests'] },
    ],
  },
  {
    id: 'go',
    title: t('pipelines.templates.go.title'),
    summary: t('pipelines.templates.go.summary'),
    icon: 'layers',
    stages: ['test', 'build'],
    jobs: [
      { tile: 'go-test', stage: 'test' },
      { tile: 'go-build', stage: 'build', needs: ['test'] },
    ],
  },
  {
    id: 'node-docker',
    title: t('pipelines.templates.node-docker.title'),
    summary: t('pipelines.templates.node-docker.summary'),
    icon: 'upload',
    stages: ['test', 'package'],
    jobs: [
      { tile: 'node-test', stage: 'test' },
      { tile: 'docker-build', stage: 'package', needs: ['unit-tests'] },
    ],
  },
  {
    id: 'node-docker-ssh',
    title: t('pipelines.templates.node-docker-ssh.title'),
    summary: t('pipelines.templates.node-docker-ssh.summary'),
    icon: 'server',
    stages: ['test', 'package', 'deploy'],
    jobs: [
      { tile: 'node-test', stage: 'test' },
      { tile: 'docker-build', stage: 'package', needs: ['unit-tests'] },
      { tile: 'ssh-run', stage: 'deploy', needs: ['image'] },
    ],
  },
  {
    id: 'node-docker-k8s',
    title: t('pipelines.templates.node-docker-k8s.title'),
    summary: t('pipelines.templates.node-docker-k8s.summary'),
    icon: 'server',
    stages: ['test', 'package', 'deploy'],
    jobs: [
      { tile: 'node-test', stage: 'test' },
      { tile: 'docker-build', stage: 'package', needs: ['unit-tests'] },
      { tile: 'k8s-image', stage: 'deploy', needs: ['image'] },
    ],
  },
]);

/** The pipeline a template describes. Jobs keep their tile's name unless the template renames them. */
export function stateFromTemplate(template: PipelineTemplate): BuilderState {
  const jobs: BuilderJob[] = [];
  for (const entry of template.jobs) {
    const tile = tileById(entry.tile);
    if (!tile) {
      continue;
    }
    const job = jobFromTile(tile, entry.stage, jobs.map((existing) => existing.name));
    jobs.push({ ...job, name: entry.name ?? job.name, needs: entry.needs ?? [] });
  }
  return { stages: [...template.stages], jobs };
}

/** The secrets a set of tiles expects, to name them when the repository does not have them. */
export const secretsOfTiles = (ids: readonly string[]): string[] => [...new Set(ids.flatMap((id) => tileById(id)?.secrets ?? []))];

/** Every secret a tile expects: the names worth offering to create when a command reads one that does not exist. */
export const knownTileSecrets = perLanguage((): readonly string[] => secretsOfTiles(jobTiles().map((tile) => tile.id)));
