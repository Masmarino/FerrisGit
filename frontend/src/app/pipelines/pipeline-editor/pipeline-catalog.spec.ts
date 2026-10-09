import { BuilderState, NEW_PIPELINE } from './pipeline-builder-model';
import { JOB_TILES, KNOWN_TILE_SECRETS, PIPELINE_TEMPLATES, TILE_CATEGORIES, addTile, jobFromTile, secretsOfTiles, stateFromTemplate, tileById } from './pipeline-catalog';
import { isEnvName } from '../../repositories/ci-variable-name';

describe('pipeline catalog', () => {
  it('has unique tile ids, each in a known category', () => {
    const ids = JOB_TILES.map((tile) => tile.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const tile of JOB_TILES) {
      expect(TILE_CATEGORIES.map((category) => category.id), tile.id).toContain(tile.category);
    }
  });

  it('gives every tile what the server needs to accept a job: an image, a command, valid cache keys and variable names', () => {
    for (const tile of JOB_TILES) {
      expect(tile.image, tile.id).not.toBe('');
      expect(tile.script.length, tile.id).toBeGreaterThan(0);
      for (const key of tile.cache ?? []) {
        expect(key, tile.id).toMatch(/^[a-z0-9-]+$/);
      }
      for (const name of Object.keys(tile.variables ?? {})) {
        expect(isEnvName(name), `${tile.id} ${name}`).toBe(true);
      }
      expect(tile.help.length, tile.id).toBeGreaterThan(20);
    }
  });

  it('names the secrets its commands read, and every one of them is read by a command', () => {
    for (const tile of JOB_TILES.filter((t) => t.secrets)) {
      for (const name of tile.secrets!) {
        expect(tile.script.join('\n'), `${tile.id} ${name}`).toContain(name);
      }
      expect(tile.needsSecrets, tile.id).toBe(true);
    }
    expect(KNOWN_TILE_SECRETS).toEqual(expect.arrayContaining(['DEPLOY_TOKEN', 'REGISTRY_PASSWORD', 'DOCKER_HOST', 'SSH_PRIVATE_KEY', 'SSH_KNOWN_HOSTS', 'KUBE_CONFIG']));
    expect(secretsOfTiles(['http-deploy', 'http-deploy'])).toEqual(['DEPLOY_URL', 'DEPLOY_TOKEN']);
  });

  it('makes a job from a tile, renamed when the name is taken', () => {
    const tile = tileById('rust-test')!;

    expect(jobFromTile(tile, 'test', []).name).toBe('test');
    expect(jobFromTile(tile, 'test', ['test', 'test-2']).name).toBe('test-3');
    expect(jobFromTile(tile, 'test', [])).toMatchObject({ stage: 'test', image: 'rust:1', needs: [], cache: ['cargo-home', 'cargo-target'], variables: [{ key: 'CARGO_HOME', value: '/ferrisgit-cache/cargo-home' }, { key: 'CARGO_TARGET_DIR', value: '/ferrisgit-cache/cargo-target' }] });
  });

  it('does not share its commands with the job: changing one must not change the catalog', () => {
    const job = jobFromTile(tileById('node-test')!, 'test', []);

    job.script.push('extra');

    expect(tileById('node-test')!.script).toEqual(['npm ci', 'npm test']);
  });

  it('drops a tile at the end of the stage it was asked for', () => {
    const state: BuilderState = { stages: ['build', 'test'], jobs: [{ name: 'a', stage: 'build', image: 'x', script: ['y'], variables: [], needs: [], tags: [], cache: [] }] };

    const { state: next, job } = addTile(state, tileById('go-test')!, 'test');

    expect(job.stage).toBe('test');
    expect(next.jobs.map((j) => j.name)).toEqual(['a', 'test']);
    expect(addTile(NEW_PIPELINE, tileById('custom')!, 'build').state.jobs).toHaveLength(1);
  });

  describe('templates', () => {
    it.each(PIPELINE_TEMPLATES.map((template) => [template.id, template] as const))('%s lays out stages and jobs that the server accepts', (_id, template) => {
      const state = stateFromTemplate(template);
      const names = state.jobs.map((job) => job.name);

      expect(new Set(names).size).toBe(names.length);
      for (const job of state.jobs) {
        expect(state.stages, job.name).toContain(job.stage);
        for (const need of job.needs) {
          // A dependency exists, and sits in the same stage or an earlier one.
          const dependency = state.jobs.find((other) => other.name === need);
          expect(dependency, `${job.name} needs ${need}`).toBeDefined();
          expect(state.stages.indexOf(dependency!.stage), `${job.name} needs ${need}`).toBeLessThanOrEqual(state.stages.indexOf(job.stage));
        }
      }
    });

    it('starts the Rust pipeline with format and Clippy side by side, and the tests after both', () => {
      const state = stateFromTemplate(PIPELINE_TEMPLATES.find((t) => t.id === 'rust')!);

      expect(state.stages).toEqual(['check', 'test']);
      expect(state.jobs.map((j) => [j.name, j.stage, j.needs])).toEqual([
        ['format', 'check', []],
        ['clippy', 'check', []],
        ['test', 'test', ['format', 'clippy']],
      ]);
    });
  });
});
