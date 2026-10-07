import { BuilderJob, BuilderState, insertJob, uniqueName } from './pipeline-builder-model';
import { DEPLOY_TILES } from './pipeline-tiles-deploy';
import { JobTile, ParamValues, TileCategory, buildTile } from './pipeline-tile-types';

export type { JobTile, ParamValue, ParamValues, TileBuild, TileCategory, TileParam } from './pipeline-tile-types';

export interface TileCategoryInfo {
  id: TileCategory;
  title: string;
  summary: string;
}

export const TILE_CATEGORIES: readonly TileCategoryInfo[] = [
  { id: 'compile', title: 'Compiler', summary: "Produire le programme à partir du code." },
  { id: 'test', title: 'Tester', summary: 'Lancer les tests du projet.' },
  { id: 'quality', title: 'Vérifier le code', summary: 'Format, règles de style, erreurs courantes.' },
  { id: 'package', title: 'Empaqueter', summary: 'Fabriquer une image ou un livrable.' },
  { id: 'deploy', title: 'Déployer et prévenir', summary: 'Envoyer le résultat sur une VM ou un cluster Kubernetes, appeler un service.' },
  { id: 'custom', title: 'Sur mesure', summary: 'Partir de zéro et écrire ses propres commandes.' },
];

const RUST_CACHE = { cache: ['cargo-home', 'cargo-target'], variables: { CARGO_HOME: '/ferrisgit-cache/cargo-home', CARGO_TARGET_DIR: '/ferrisgit-cache/cargo-target' } };
const NPM_CACHE = { cache: ['npm'], variables: { npm_config_cache: '/ferrisgit-cache/npm' } };

const SIMPLE_TILES: readonly JobTile[] = [
  {
    id: 'rust-build',
    category: 'compile',
    title: 'Compiler un projet Rust',
    summary: 'Cargo build en mode release.',
    help: "Utilise l'image officielle rust. Le registre de Cargo et le dossier target sont mis en cache (Kubernetes).",
    icon: 'layers',
    jobName: 'compile',
    stage: 'build',
    image: 'rust:1',
    script: ['cargo build --release'],
    ...RUST_CACHE,
  },
  {
    id: 'node-build',
    category: 'compile',
    title: 'Compiler un projet Node ou Angular',
    summary: 'npm ci puis npm run build.',
    help: "Lance le script « build » de votre package.json. Chaque job refait son npm ci : rien n'est transmis d'un job à l'autre.",
    icon: 'layers',
    jobName: 'build',
    stage: 'build',
    image: 'node:22',
    script: ['npm ci', 'npm run build'],
    ...NPM_CACHE,
  },
  {
    id: 'go-build',
    category: 'compile',
    title: 'Compiler un projet Go',
    summary: 'go build sur tout le module.',
    help: "Utilise l'image officielle golang. Adaptez la version de l'image à celle de votre go.mod.",
    icon: 'layers',
    jobName: 'build',
    stage: 'build',
    image: 'golang:1.23',
    script: ['go build ./...'],
  },
  {
    id: 'rust-test',
    category: 'test',
    title: 'Tester un projet Rust',
    summary: 'Cargo test sur tout le projet.',
    help: 'Lance les tests unitaires et ceux du dossier tests. Placez-le après les vérifications de code pour ne pas tester du code mal formé.',
    icon: 'flask-conical',
    jobName: 'test',
    stage: 'test',
    image: 'rust:1',
    script: ['cargo test --all-targets'],
    ...RUST_CACHE,
  },
  {
    id: 'node-test',
    category: 'test',
    title: 'Tester un projet Node ou Angular',
    summary: 'npm ci puis npm test.',
    help: "Lance le script « test » de votre package.json. Il doit tourner sans interface graphique : s'il lui faut un navigateur, choisissez une image qui en contient un.",
    icon: 'flask-conical',
    jobName: 'unit-tests',
    stage: 'test',
    image: 'node:22',
    script: ['npm ci', 'npm test'],
    ...NPM_CACHE,
  },
  {
    id: 'go-test',
    category: 'test',
    title: 'Tester un projet Go',
    summary: 'go test sur tout le module.',
    help: "Lance tous les tests du module. Ajoutez -race pour chercher les accès concurrents (plus lent).",
    icon: 'flask-conical',
    jobName: 'test',
    stage: 'test',
    image: 'golang:1.23',
    script: ['go test ./...'],
  },
  {
    id: 'python-test',
    category: 'test',
    title: 'Tester un projet Python',
    summary: 'pip install puis pytest.',
    help: "Installe requirements.txt puis lance pytest : ajoutez pytest à vos dépendances si ce n'est pas déjà le cas.",
    icon: 'flask-conical',
    jobName: 'test',
    stage: 'test',
    image: 'python:3.13',
    script: ['pip install -r requirements.txt', 'python -m pytest'],
  },
  {
    id: 'rust-format',
    category: 'quality',
    title: 'Vérifier le format Rust',
    summary: 'Cargo fmt en mode vérification.',
    help: "Échoue si le code n'est pas formaté comme rustfmt le ferait, sans rien modifier. L'image rust n'inclut pas rustfmt : le job l'ajoute.",
    icon: 'shield-check',
    jobName: 'format',
    stage: 'check',
    image: 'rust:1',
    script: ['rustup component add rustfmt', 'cargo fmt --all -- --check'],
  },
  {
    id: 'rust-clippy',
    category: 'quality',
    title: 'Analyser le code Rust (Clippy)',
    summary: 'Clippy, avertissements traités comme des erreurs.',
    help: "Repère les erreurs courantes et les constructions maladroites. L'image rust n'inclut pas Clippy : le job l'ajoute.",
    icon: 'shield-check',
    jobName: 'clippy',
    stage: 'check',
    image: 'rust:1',
    script: ['rustup component add clippy', 'cargo clippy --all-targets -- -D warnings'],
    ...RUST_CACHE,
  },
  {
    id: 'node-lint',
    category: 'quality',
    title: 'Analyser le code Node ou Angular',
    summary: 'npm ci puis npm run lint.',
    help: "Lance le script « lint » de votre package.json (ESLint, par exemple).",
    icon: 'shield-check',
    jobName: 'lint',
    stage: 'check',
    image: 'node:22',
    script: ['npm ci', 'npm run lint'],
    ...NPM_CACHE,
  },
  {
    id: 'http-deploy',
    category: 'deploy',
    title: 'Déployer par un appel HTTP',
    summary: "Appelle l'adresse de déploiement de votre plateforme.",
    help: "Réservé par l'étiquette « deploy » à un runner dédié : sans runner qui la porte, le job reste en attente. Créez les secrets DEPLOY_URL et DEPLOY_TOKEN. Une pipeline part à chaque push : ce job redéploie donc à chaque fois.",
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
    title: 'Prévenir un service (webhook)',
    summary: 'Envoie une requête POST à une adresse.',
    help: "Pratique pour avertir un chat ou un outil de suivi. Créez le secret NOTIFY_URL avec l'adresse complète (elle contient souvent un jeton).",
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
    title: 'Job vide',
    summary: 'Une image légère et une commande à remplacer.',
    help: "Alpine est une petite image Linux avec les commandes de base. Changez l'image et les commandes dans le tiroir du job.",
    icon: 'sparkles',
    jobName: 'job',
    stage: 'build',
    image: 'alpine:3.20',
    script: ['echo "Bonjour"'],
  },
];

/** The catalogue. Within a category the order is the one shown: deployments to a machine or a cluster before the plain HTTP calls. */
export const JOB_TILES: readonly JobTile[] = [
  ...SIMPLE_TILES.filter((tile) => tile.category !== 'deploy' && tile.category !== 'custom'),
  ...DEPLOY_TILES,
  ...SIMPLE_TILES.filter((tile) => tile.category === 'deploy'),
  ...SIMPLE_TILES.filter((tile) => tile.category === 'custom'),
];

export const tileById = (id: string): JobTile | undefined => JOB_TILES.find((tile) => tile.id === id);

/** The job a tile makes, named so as not to collide. `values` answer the tile's questions, when it has some. */
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

/** Drops a tile's job at the end of a stage. */
export function addTile(state: BuilderState, tile: JobTile, stage: string, values: ParamValues = {}): { state: BuilderState; job: BuilderJob } {
  const job = jobFromTile(tile, stage, state.jobs.map((existing) => existing.name), values);
  return { state: insertJob(state, job), job };
}

/** A whole pipeline to start from: its stages and, for each job, the tile it comes from and what it waits for. */
export interface PipelineTemplate {
  id: string;
  title: string;
  summary: string;
  icon: string;
  stages: string[];
  jobs: { tile: string; name?: string; stage: string; needs?: string[] }[];
}

export const PIPELINE_TEMPLATES: readonly PipelineTemplate[] = [
  {
    id: 'rust',
    title: 'Projet Rust',
    summary: 'Format et Clippy en parallèle, puis les tests.',
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
    title: 'Projet Node ou Angular',
    summary: 'Analyse et tests en parallèle, puis le build.',
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
    title: 'Projet Go',
    summary: 'Les tests, puis la compilation.',
    icon: 'layers',
    stages: ['test', 'build'],
    jobs: [
      { tile: 'go-test', stage: 'test' },
      { tile: 'go-build', stage: 'build', needs: ['test'] },
    ],
  },
  {
    id: 'node-docker',
    title: 'Application publiée en image',
    summary: 'Tests Node, puis construction et publication de l’image.',
    icon: 'upload',
    stages: ['test', 'package'],
    jobs: [
      { tile: 'node-test', stage: 'test' },
      { tile: 'docker-build', stage: 'package', needs: ['unit-tests'] },
    ],
  },
  {
    id: 'node-docker-ssh',
    title: 'Image Docker déployée sur une VM',
    summary: 'Tests, image publiée, puis déploiement par SSH.',
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
    title: 'Image Docker déployée sur Kubernetes',
    summary: 'Tests, image publiée, puis mise à jour du Deployment.',
    icon: 'server',
    stages: ['test', 'package', 'deploy'],
    jobs: [
      { tile: 'node-test', stage: 'test' },
      { tile: 'docker-build', stage: 'package', needs: ['unit-tests'] },
      { tile: 'k8s-image', stage: 'deploy', needs: ['image'] },
    ],
  },
];

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

/** The secrets the pipeline's tiles expect, to name them when the repository does not have them. */
export const secretsOfTiles = (ids: readonly string[]): string[] => [...new Set(ids.flatMap((id) => tileById(id)?.secrets ?? []))];

/** Every secret a tile expects: the names worth offering to create when a command reads one that does not exist. */
export const KNOWN_TILE_SECRETS: readonly string[] = secretsOfTiles(JOB_TILES.map((tile) => tile.id));
