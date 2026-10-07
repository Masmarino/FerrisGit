import { BuilderJob, BuilderState, commandsOf, jobsOf } from './pipeline-builder-model';
import { PipelineLink } from './pipeline-links';

/**
 * What the board shows of a pipeline: a stage per column, a card per job, and the ties between them. Pure, so that what
 * a card says follows from the pipeline alone.
 */

export const stageId = (index: number) => `pipeline-stage-${index}`;

export interface CardView {
  job: BuilderJob;
  /** What the server says is wrong with this job (warnings are not counted). */
  problemCount: number;
  menuLabel: string;
  moveTargets: string[];
  /** What the job is for: its last command, the ones before only set it up (`cd web`, `npm ci`). */
  mainCommand: string | null;
  /** How many commands come before it. */
  setupCommands: number;
  /** Its tie to the pointed job: one it waits for, or one that waits for it. */
  relation: 'waited' | 'waiting' | null;
}

export interface LaneView {
  stage: string;
  index: number;
  id: string;
  headingId: string;
  cards: CardView[];
  canMoveBefore: boolean;
  canMoveAfter: boolean;
  /** Only an empty stage can go: deleting its jobs with it is not something a click should do. */
  removable: boolean;
}

/** The tie of `job` to the pointed job, seen from `job`. */
function relationTo(pointed: BuilderJob | null, job: BuilderJob): CardView['relation'] {
  if (!pointed || pointed.name === job.name) {
    return null;
  }
  return pointed.needs.includes(job.name) ? 'waited' : job.needs.includes(pointed.name) ? 'waiting' : null;
}

/**
 * The columns of the board. `problemJobs` names the job of each problem the server reports (one entry per problem);
 * `pointed` is the job under the pointer or the focus.
 */
export function boardLanes(state: BuilderState, problemJobs: readonly (string | null | undefined)[], pointed: string | null): LaneView[] {
  const pointedJob = state.jobs.find((job) => job.name === pointed) ?? null;
  const counts = new Map<string, number>();
  for (const job of problemJobs) {
    if (job) {
      counts.set(job, (counts.get(job) ?? 0) + 1);
    }
  }
  return state.stages.map((stage, index) => {
    const jobs = jobsOf(state, stage);
    return {
      stage,
      index,
      id: stageId(index),
      headingId: `${stageId(index)}-title`,
      cards: jobs.map((job) => {
        const commands = commandsOf(job);
        return {
          job,
          problemCount: counts.get(job.name) ?? 0,
          menuLabel: `Actions du job ${job.name}`,
          moveTargets: state.stages.filter((other) => other !== stage),
          mainCommand: commands.at(-1) ?? null,
          setupCommands: Math.max(0, commands.length - 1),
          relation: relationTo(pointedJob, job),
        };
      }),
      canMoveBefore: index > 0,
      canMoveAfter: index < state.stages.length - 1,
      removable: jobs.length === 0,
    };
  });
}

/**
 * Every `needs` of the board, as on a pipeline's page. A need of a job that does not exist draws nothing; one of a job
 * in a later stage is refused by the server (check_pipeline_definition), and says so. The same stage is allowed.
 */
export function boardLinks(state: BuilderState, pointed: string | null): PipelineLink[] {
  const rank = new Map(state.jobs.map((job) => [job.name, state.stages.indexOf(job.stage)]));
  return state.jobs.flatMap((job) =>
    job.needs
      .filter((need) => rank.has(need))
      .map((need) => ({ from: need, to: job.name, highlighted: pointed === need || pointed === job.name, invalid: rank.get(need)! > rank.get(job.name)! })),
  );
}
