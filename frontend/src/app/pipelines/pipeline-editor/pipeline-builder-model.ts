import { DefinitionDto, JobDto } from './pipeline-definitions.service';

/** A variable row. Rows rather than a map, so an empty or repeated key can be edited before it is valid. */
export interface BuilderVariable {
  key: string;
  value: string;
}

export interface BuilderJob {
  name: string;
  stage: string;
  image: string;
  script: string[];
  variables: BuilderVariable[];
  needs: string[];
  tags: string[];
  cache: string[];
}

/** What the builder edits: the stages in order, and the jobs, which keep their order inside a stage. */
export interface BuilderState {
  stages: string[];
  jobs: BuilderJob[];
}

export const NEW_PIPELINE: BuilderState = { stages: ['build', 'test'], jobs: [] };

export const jobsOf = (state: BuilderState, stage: string): BuilderJob[] => state.jobs.filter((job) => job.stage === stage);

/** Jobs grouped by stage in the order of `stages`, those of an unknown stage last, so a lane move only reorders within a stage. */
function grouped(stages: string[], jobs: BuilderJob[]): BuilderJob[] {
  const known = stages.flatMap((stage) => jobs.filter((job) => job.stage === stage));
  return [...known, ...jobs.filter((job) => !stages.includes(job.stage))];
}

export function fromDefinition(definition: DefinitionDto): BuilderState {
  const jobs: BuilderJob[] = Object.entries(definition.jobs).map(([name, job]) => ({
    name,
    stage: job.stage,
    image: job.image,
    script: [...job.script],
    variables: Object.entries(job.variables).map(([key, value]) => ({ key, value })),
    needs: [...job.needs],
    tags: [...job.tags],
    cache: [...job.cache],
  }));
  return { stages: [...definition.stages], jobs: grouped(definition.stages, jobs) };
}

export function toDefinition(state: BuilderState): DefinitionDto {
  const jobs: Record<string, JobDto> = {};
  for (const job of state.jobs) {
    jobs[job.name] = {
      stage: job.stage,
      image: job.image,
      // Blank lines are kept while editing (the cursor needs somewhere to go), and never written.
      script: job.script.filter((line) => line.trim() !== ''),
      // A row with no key means nothing: it is dropped rather than written as an empty name.
      variables: Object.fromEntries(job.variables.filter((row) => row.key.trim() !== '').map((row) => [row.key.trim(), row.value])),
      needs: [...job.needs],
      tags: [...job.tags],
      cache: [...job.cache],
    };
  }
  return { stages: [...state.stages], jobs };
}

/** `base`, then `base-2`, `base-3`... the first that no other name uses. */
export function uniqueName(taken: string[], base: string): string {
  if (!taken.includes(base)) {
    return base;
  }
  let n = 2;
  while (taken.includes(`${base}-${n}`)) {
    n++;
  }
  return `${base}-${n}`;
}

export function addStage(state: BuilderState, name: string): BuilderState {
  const trimmed = name.trim();
  if (trimmed === '' || state.stages.includes(trimmed)) {
    return state;
  }
  return { ...state, stages: [...state.stages, trimmed] };
}

/** The jobs of the stage follow its new name. A name that is empty or already a stage changes nothing. */
export function renameStage(state: BuilderState, from: string, to: string): BuilderState {
  const trimmed = to.trim();
  if (trimmed === '' || trimmed === from || !state.stages.includes(from) || state.stages.includes(trimmed)) {
    return state;
  }
  return {
    stages: state.stages.map((stage) => (stage === from ? trimmed : stage)),
    jobs: state.jobs.map((job) => (job.stage === from ? { ...job, stage: trimmed } : job)),
  };
}

/** Only an empty stage goes: deleting its jobs along with it is not something a click should do. */
export function removeStage(state: BuilderState, stage: string): BuilderState {
  if (jobsOf(state, stage).length > 0) {
    return state;
  }
  return { ...state, stages: state.stages.filter((candidate) => candidate !== stage) };
}

/** Moves a stage to a new place in the order, the jobs staying in it. */
export function moveStage(state: BuilderState, from: number, to: number): BuilderState {
  if (from === to || from < 0 || to < 0 || from >= state.stages.length || to >= state.stages.length) {
    return state;
  }
  const stages = [...state.stages];
  const [stage] = stages.splice(from, 1);
  stages.splice(to, 0, stage);
  return { stages, jobs: grouped(stages, state.jobs) };
}

export function newJob(state: BuilderState, stage: string): BuilderJob {
  return { name: uniqueName(state.jobs.map((job) => job.name), 'job'), stage, image: '', script: [], variables: [], needs: [], tags: [], cache: [] };
}

export function addJob(state: BuilderState, stage: string): BuilderState {
  return insertJob(state, newJob(state, stage));
}

/** A finished job at the end of its stage. */
export function insertJob(state: BuilderState, job: BuilderJob): BuilderState {
  return { ...state, jobs: grouped(state.stages, [...state.jobs, job]) };
}

/** Changes a job. Its new name is followed by the `needs` of the jobs that depend on it; a name that is empty or used by another job is ignored. */
export function updateJob(state: BuilderState, name: string, patch: Partial<BuilderJob>): BuilderState {
  const current = state.jobs.find((job) => job.name === name);
  if (!current) {
    return state;
  }
  const requested = patch.name?.trim();
  const newName = requested !== undefined && requested !== '' && (requested === name || !state.jobs.some((job) => job.name === requested)) ? requested : name;
  const next: BuilderJob = { ...current, ...patch, name: newName };
  const jobs = state.jobs.map((job) => {
    if (job.name === name) {
      return next;
    }
    return newName !== name && job.needs.includes(name) ? { ...job, needs: job.needs.map((need) => (need === name ? newName : need)) } : job;
  });
  return { ...state, jobs: grouped(state.stages, jobs) };
}

/** The job goes, and so does every `needs` that named it. */
export function removeJob(state: BuilderState, name: string): BuilderState {
  return {
    ...state,
    jobs: state.jobs.filter((job) => job.name !== name).map((job) => (job.needs.includes(name) ? { ...job, needs: job.needs.filter((need) => need !== name) } : job)),
  };
}

/**
 * Drops a job into a stage, at `index` among that stage's jobs. A move never touches `needs`: if it leaves a job ahead of
 * what it waits for, the server reports it and the builder shows the problem.
 */
export function moveJob(state: BuilderState, name: string, stage: string, index: number): BuilderState {
  const job = state.jobs.find((candidate) => candidate.name === name);
  if (!job || !state.stages.includes(stage)) {
    return state;
  }
  const others = state.jobs.filter((candidate) => candidate.name !== name);
  const target = others.filter((candidate) => candidate.stage === stage);
  const at = Math.max(0, Math.min(index, target.length));
  const moved = { ...job, stage };
  const before = target[at] ? others.indexOf(target[at]) : others.length;
  const jobs = [...others.slice(0, before), moved, ...others.slice(before)];
  return { ...state, jobs: grouped(state.stages, jobs) };
}

/** The jobs that `name` may wait for: the others, in its own stage or an earlier one. */
export function possibleNeeds(state: BuilderState, name: string): string[] {
  const job = state.jobs.find((candidate) => candidate.name === name);
  if (!job) {
    return [];
  }
  const rank = state.stages.indexOf(job.stage);
  return state.jobs
    .filter((other) => other.name !== name && state.stages.indexOf(other.stage) !== -1 && state.stages.indexOf(other.stage) <= rank)
    .map((other) => other.name);
}
