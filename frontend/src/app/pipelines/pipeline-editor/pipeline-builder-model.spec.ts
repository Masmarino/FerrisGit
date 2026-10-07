import {
  BuilderJob,
  BuilderState,
  NEW_PIPELINE,
  addJob,
  addStage,
  fromDefinition,
  jobsOf,
  moveJob,
  moveStage,
  possibleNeeds,
  removeJob,
  removeStage,
  renameStage,
  toDefinition,
  uniqueName,
  updateJob,
} from './pipeline-builder-model';
import { DefinitionDto } from './pipeline-definitions.service';

const job = (name: string, stage: string, extra: Partial<BuilderJob> = {}): BuilderJob => ({
  name,
  stage,
  image: 'rust:1',
  script: ['cargo build'],
  variables: [],
  needs: [],
  tags: [],
  cache: [],
  ...extra,
});

const state = (stages: string[], jobs: BuilderJob[]): BuilderState => ({ stages, jobs });
const names = (jobs: BuilderJob[]) => jobs.map((j) => j.name);

describe('pipeline builder model', () => {
  describe('conversion', () => {
    const definition: DefinitionDto = {
      stages: ['build', 'test'],
      jobs: {
        unit: { stage: 'test', image: 'rust:1', script: ['cargo test'], variables: { RUST_LOG: 'debug' }, needs: ['compile'], tags: ['docker'], cache: ['cargo'] },
        compile: { stage: 'build', image: 'rust:1', script: ['cargo build'], variables: {}, needs: [], tags: [], cache: [] },
      },
    };

    it('groups the jobs by stage in the order of the stages', () => {
      const result = fromDefinition(definition);

      expect(result.stages).toEqual(['build', 'test']);
      expect(names(result.jobs)).toEqual(['compile', 'unit']);
    });

    it('turns variables into rows and back', () => {
      const result = fromDefinition(definition);

      expect(result.jobs[1].variables).toEqual([{ key: 'RUST_LOG', value: 'debug' }]);
      expect(toDefinition(result)).toEqual(definition);
    });

    it('drops a variable row with no key instead of writing an empty name', () => {
      const withBlankRow = state(['a'], [job('x', 'a', { variables: [{ key: ' ', value: 'v' }, { key: 'K', value: 'w' }] })]);

      expect(toDefinition(withBlankRow).jobs['x'].variables).toEqual({ K: 'w' });
    });

    it('does not write blank command lines', () => {
      const withBlanks = state(['a'], [job('x', 'a', { script: ['npm ci', '', '  ', 'npm test'] })]);

      expect(toDefinition(withBlanks).jobs['x'].script).toEqual(['npm ci', 'npm test']);
    });

    it('does not share arrays with its input', () => {
      const result = fromDefinition(definition);
      result.jobs[0].script.push('extra');

      expect(definition.jobs['compile'].script).toEqual(['cargo build']);
    });
  });

  describe('stages', () => {
    it('adds a trimmed stage once and ignores an empty or repeated one', () => {
      let result = addStage(NEW_PIPELINE, ' deploy ');
      expect(result.stages).toEqual(['build', 'test', 'deploy']);

      result = addStage(result, 'deploy');
      result = addStage(result, '   ');
      expect(result.stages).toEqual(['build', 'test', 'deploy']);
    });

    it('renames a stage and its jobs follow', () => {
      const start = state(['build', 'test'], [job('compile', 'build'), job('unit', 'test')]);

      const result = renameStage(start, 'build', 'compile-all');

      expect(result.stages).toEqual(['compile-all', 'test']);
      expect(jobsOf(result, 'compile-all').map((j) => j.name)).toEqual(['compile']);
    });

    it('refuses a rename to an empty name or to another stage', () => {
      const start = state(['build', 'test'], [job('compile', 'build')]);

      expect(renameStage(start, 'build', '')).toBe(start);
      expect(renameStage(start, 'build', 'test')).toBe(start);
      expect(renameStage(start, 'nowhere', 'x')).toBe(start);
    });

    it('removes an empty stage and keeps one that still has jobs', () => {
      const start = state(['build', 'test', 'empty'], [job('compile', 'build')]);

      expect(removeStage(start, 'empty').stages).toEqual(['build', 'test']);
      expect(removeStage(start, 'build')).toBe(start);
    });

    it('moves a stage in the order, its jobs staying in it', () => {
      const start = state(['build', 'test', 'deploy'], [job('compile', 'build'), job('unit', 'test'), job('ship', 'deploy')]);

      const result = moveStage(start, 2, 0);

      expect(result.stages).toEqual(['deploy', 'build', 'test']);
      expect(names(result.jobs)).toEqual(['ship', 'compile', 'unit']);
      expect(moveStage(start, 1, 1)).toBe(start);
      expect(moveStage(start, 0, 9)).toBe(start);
    });
  });

  describe('jobs', () => {
    it('numbers a new job after the ones that exist', () => {
      let result = addJob(NEW_PIPELINE, 'build');
      result = addJob(result, 'build');
      result = addJob(result, 'test');

      expect(names(result.jobs)).toEqual(['job', 'job-2', 'job-3']);
      expect(result.jobs[2].stage).toBe('test');
      expect(uniqueName(['a', 'a-2'], 'a')).toBe('a-3');
    });

    it('renames a job and the jobs that wait for it follow', () => {
      const start = state(['build', 'test'], [job('compile', 'build'), job('unit', 'test', { needs: ['compile'] })]);

      const result = updateJob(start, 'compile', { name: ' build-all ' });

      expect(names(result.jobs)).toEqual(['build-all', 'unit']);
      expect(result.jobs[1].needs).toEqual(['build-all']);
    });

    it('keeps the old name when the new one is empty or taken', () => {
      const start = state(['build'], [job('a', 'build'), job('b', 'build')]);

      expect(updateJob(start, 'a', { name: '' }).jobs[0].name).toBe('a');
      expect(updateJob(start, 'a', { name: 'b' }).jobs[0].name).toBe('a');
      expect(updateJob(start, 'a', { name: 'b', image: 'node:22' }).jobs[0].image).toBe('node:22');
    });

    it('changes the other fields without renaming', () => {
      const start = state(['build'], [job('a', 'build')]);

      const result = updateJob(start, 'a', { image: 'node:22', script: ['npm ci', 'npm test'] });

      expect(result.jobs[0]).toMatchObject({ name: 'a', image: 'node:22', script: ['npm ci', 'npm test'] });
    });

    it('removes a job and every needs that named it', () => {
      const start = state(['build', 'test'], [job('compile', 'build'), job('unit', 'test', { needs: ['compile'] })]);

      const result = removeJob(start, 'compile');

      expect(names(result.jobs)).toEqual(['unit']);
      expect(result.jobs[0].needs).toEqual([]);
    });

    it('ignores a change to a job that does not exist', () => {
      const start = state(['build'], [job('a', 'build')]);

      expect(updateJob(start, 'ghost', { image: 'x' })).toBe(start);
    });
  });

  describe('moving a job', () => {
    const start = state(['build', 'test'], [job('a', 'build'), job('b', 'build'), job('c', 'test')]);

    it('reorders inside a stage', () => {
      expect(names(jobsOf(moveJob(start, 'a', 'build', 1), 'build'))).toEqual(['b', 'a']);
      expect(names(jobsOf(moveJob(start, 'b', 'build', 0), 'build'))).toEqual(['b', 'a']);
    });

    it('moves to another stage at the given place', () => {
      const result = moveJob(start, 'a', 'test', 0);

      expect(names(jobsOf(result, 'build'))).toEqual(['b']);
      expect(names(jobsOf(result, 'test'))).toEqual(['a', 'c']);
      expect(result.jobs.find((j) => j.name === 'a')?.stage).toBe('test');
    });

    it('puts a job at the end when the place is past the end, and at the start when before it', () => {
      expect(names(jobsOf(moveJob(start, 'a', 'test', 99), 'test'))).toEqual(['c', 'a']);
      expect(names(jobsOf(moveJob(start, 'c', 'build', -4), 'build'))).toEqual(['c', 'a', 'b']);
    });

    it('moves into an empty stage', () => {
      const withEmpty = state(['build', 'test', 'deploy'], start.jobs);

      expect(names(jobsOf(moveJob(withEmpty, 'c', 'deploy', 0), 'deploy'))).toEqual(['c']);
    });

    it('leaves the dependencies alone, even when the job now comes before what it waits for', () => {
      const dependent = state(['build', 'test'], [job('a', 'build'), job('b', 'test', { needs: ['a'] })]);

      const result = moveJob(dependent, 'b', 'build', 0);

      expect(result.jobs.find((j) => j.name === 'b')?.needs).toEqual(['a']);
    });

    it('ignores an unknown job or stage', () => {
      expect(moveJob(start, 'ghost', 'build', 0)).toBe(start);
      expect(moveJob(start, 'a', 'nowhere', 0)).toBe(start);
    });
  });

  describe('possible dependencies', () => {
    it('offers the other jobs of its own stage and the earlier ones', () => {
      const s = state(['build', 'test', 'deploy'], [job('a', 'build'), job('b', 'build'), job('c', 'test'), job('d', 'deploy')]);

      expect(possibleNeeds(s, 'c')).toEqual(['a', 'b']);
      expect(possibleNeeds(s, 'a')).toEqual(['b']);
      expect(possibleNeeds(s, 'ghost')).toEqual([]);
    });
  });
});
