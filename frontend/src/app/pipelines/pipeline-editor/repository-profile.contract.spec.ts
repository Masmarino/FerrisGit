import contract from './repository-profile.contract.json';
import { DetectedProject, NodeFramework, PackageManager, PythonTool, RepositoryProfile, TestRunner } from './pipeline-definitions.service';

/**
 * The editor's types for the repository profile mirror the server's (repository_profile.rs). The server's tests keep
 * repository-profile.contract.json in step with what it sends, and these tests check the types against that file, so
 * that neither side can change alone.
 */

/** What a field holds, as `typeof` reports it. `?` means it may also be null. */
type Shape = 'string' | 'string?' | 'boolean' | 'string[]' | 'record';
type Shapes<T> = Record<keyof T, Shape>;
type Kind<K extends DetectedProject['kind']> = Extract<DetectedProject, { kind: K }>;

// Adding a field to the types, or removing one, fails to compile here until its shape is given.
const PROFILE: Shapes<RepositoryProfile> = { projects: 'record', dockerfiles: 'string[]', helmCharts: 'string[]' };
const PROJECTS: { [K in DetectedProject['kind']]: Shapes<Kind<K>> } = {
  rust: { kind: 'string', dir: 'string', evidence: 'string[]', workspace: 'boolean', toolchain: 'string?', sqlxOffline: 'boolean', sqlxPostgres: 'boolean' },
  node: { kind: 'string', dir: 'string', evidence: 'string[]', packageManager: 'string', nodeVersion: 'string?', scripts: 'record', framework: 'string?', testRunner: 'string?' },
  go: { kind: 'string', dir: 'string', evidence: 'string[]', goVersion: 'string?' },
  python: { kind: 'string', dir: 'string', evidence: 'string[]', tool: 'string', pythonVersion: 'string?', pytest: 'boolean', ruff: 'boolean' },
};
// The same goes for a value added to or removed from a union.
const PACKAGE_MANAGERS: Record<PackageManager, true> = { npm: true, pnpm: true, yarnClassic: true, yarn: true, bun: true };
const FRAMEWORKS: Record<NodeFramework, true> = { angular: true, react: true, vue: true, svelte: true, next: true };
const TEST_RUNNERS: Record<TestRunner, true> = { vitest: true, jest: true, karma: true, playwright: true };
const PYTHON_TOOLS: Record<PythonTool, true> = { pip: true, poetry: true, uv: true };

function fits(value: unknown, shape: Shape): boolean {
  switch (shape) {
    case 'string?':
      return value === null || typeof value === 'string';
    case 'string[]':
      return Array.isArray(value) && value.every((item) => typeof item === 'string');
    case 'record':
      return typeof value === 'object' && value !== null;
    default:
      return typeof value === shape;
  }
}

/** The fields of `value` that are missing, extra, or of another shape than `shapes` says. */
function mismatches(value: object, shapes: Record<string, Shape>): string[] {
  const fields = new Set([...Object.keys(value), ...Object.keys(shapes)]);
  return [...fields].filter((field) => !(field in shapes) || !(field in value) || !fits((value as Record<string, unknown>)[field], shapes[field]));
}

describe('the repository profile, as the server sends it', () => {
  const projects = contract.profile.projects as Record<string, unknown>[];

  it('has the fields the editor types say, of the same shapes, and no other', () => {
    expect(mismatches(contract.profile, PROFILE)).toEqual([]);
    for (const project of projects) {
      expect(mismatches(project, PROJECTS[project['kind'] as DetectedProject['kind']])).toEqual([]);
    }
  });

  it('has a sample of every kind of project the editor knows', () => {
    expect(projects.map((project) => project['kind']).sort()).toEqual(Object.keys(PROJECTS).sort());
  });

  it('takes the values the editor knows, and no other', () => {
    expect([...contract.packageManagers].sort()).toEqual(Object.keys(PACKAGE_MANAGERS).sort());
    expect([...contract.frameworks].sort()).toEqual(Object.keys(FRAMEWORKS).sort());
    expect([...contract.testRunners].sort()).toEqual(Object.keys(TEST_RUNNERS).sort());
    expect([...contract.pythonTools].sort()).toEqual(Object.keys(PYTHON_TOOLS).sort());
  });
});
