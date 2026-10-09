import { PackageManager, PythonTool } from './pipeline-definitions.service';

/**
 * How each language is checked, tested and built: the images and the commands. The tiles use them with their defaults,
 * and the proposal for a repository with what the repository says (its toolchain, package manager and scripts), so that
 * both always run a project the same way.
 */

/**
 * A cache key, and the variables that send a tool's downloads into it (Kubernetes mounts it under /ferrisgit-cache).
 */
export interface CacheRecipe {
  cache: string[];
  variables: Record<string, string>;
}

export const RUST = {
  image: (toolchain?: string | null) => (toolchain ? `rust:${toolchain}` : 'rust:1'),
  cache: { cache: ['cargo-home', 'cargo-target'], variables: { CARGO_HOME: '/ferrisgit-cache/cargo-home', CARGO_TARGET_DIR: '/ferrisgit-cache/cargo-target' } } satisfies CacheRecipe,
  /** The rust image has neither rustfmt nor Clippy, so each job installs the one it runs. */
  format: () => ['rustup component add rustfmt', 'cargo fmt --all -- --check'],
  clippy: (workspace = false) => ['rustup component add clippy', `cargo clippy${workspace ? ' --workspace' : ''} --all-targets -- -D warnings`],
  test: (workspace = false) => [`cargo test${workspace ? ' --workspace' : ''} --all-targets`],
  build: () => ['cargo build --release'],
};

/** Where each package manager keeps what it downloads. */
const PACKAGE_CACHES: Record<PackageManager, CacheRecipe> = {
  npm: { cache: ['npm'], variables: { npm_config_cache: '/ferrisgit-cache/npm' } },
  pnpm: { cache: ['pnpm-store'], variables: { npm_config_store_dir: '/ferrisgit-cache/pnpm-store' } },
  yarnClassic: { cache: ['yarn'], variables: { YARN_CACHE_FOLDER: '/ferrisgit-cache/yarn' } },
  yarn: { cache: ['yarn'], variables: { YARN_CACHE_FOLDER: '/ferrisgit-cache/yarn' } },
  bun: { cache: ['bun'], variables: { BUN_INSTALL_CACHE_DIR: '/ferrisgit-cache/bun' } },
};

export const NODE = {
  image: (manager: PackageManager = 'npm', version?: string | null) => (manager === 'bun' ? 'oven/bun:1' : `node:${version ?? '22'}`),
  cache: (manager: PackageManager = 'npm'): CacheRecipe => PACKAGE_CACHES[manager],
  /** The frozen install of each package manager: the lockfile is the reference, and a job never updates it. */
  install: (manager: PackageManager = 'npm', locked = true): string[] =>
    ({
      npm: [locked ? 'npm ci' : 'npm install'],
      pnpm: ['corepack enable', 'pnpm install --frozen-lockfile'],
      yarnClassic: ['yarn install --frozen-lockfile'],
      yarn: ['corepack enable', 'yarn install --immutable'],
      bun: ['bun install --frozen-lockfile'],
    })[manager],
  /** Runs a script of package.json. `args` are passed to the script itself (npm needs a `--` before them). */
  run: (script: string, manager: PackageManager = 'npm', args = ''): string => {
    const extra = args === '' ? '' : manager === 'npm' ? ` -- ${args}` : ` ${args}`;
    if (manager === 'yarnClassic' || manager === 'yarn') {
      return `yarn ${script}${extra}`;
    }
    return script === 'test' && manager !== 'bun' ? `${manager} test${extra}` : `${manager} run ${script}${extra}`;
  },
};

export const GO = {
  image: (version?: string | null) => `golang:${version ?? '1.23'}`,
  format: () => ['test -z "$(gofmt -l .)"'],
  vet: () => ['go vet ./...'],
  test: () => ['go test ./...'],
  build: () => ['go build ./...'],
};

export const PYTHON = {
  image: (version?: string | null) => `python:${version ?? '3.13'}`,
  /** Installs the project and its dependencies. With pip, from requirements.txt when there is one. */
  setup: (tool: PythonTool = 'pip', requirements = true): string[] =>
    ({ uv: ['pip install uv', 'uv sync --frozen'], poetry: ['pip install poetry', 'poetry install'], pip: [requirements ? 'pip install -r requirements.txt' : 'pip install -e .'] })[tool],
  /** A command run in the project's own environment. */
  exec: (command: string, tool: PythonTool = 'pip') => `${{ uv: 'uv run ', poetry: 'poetry run ', pip: '' }[tool]}${command}`,
};
