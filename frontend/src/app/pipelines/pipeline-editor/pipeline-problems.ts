import { PipelineProblem, PipelineWarning } from './pipeline-definitions.service';
import { t } from '../../shared/i18n/translator';

/** A problem or a warning in French, with the job it concerns (so that the editor can mark its card and open it). */
export interface ProblemView {
  message: string;
  job: string | null;
}

const quote = (name: string) => t('common.quoted', { text: name });

export function describeProblem(problem: PipelineProblem): ProblemView {
  const job = problem.job ?? null;
  switch (problem.code) {
    case 'invalid_yaml':
      return { message: t('pipelines.problems.invalidYaml', { detail: problem.message.replace(/^invalid YAML: /, '') }), job };
    case 'unknown_stage':
      return { message: t('pipelines.problems.unknownStage', { job: quote(problem.job ?? '?'), stage: quote(problem.stage ?? '?') }), job };
    case 'unknown_dependency':
      return { message: t('pipelines.problems.unknownDependency', { job: quote(problem.job ?? '?'), dependency: quote(problem.dependency ?? '?') }), job };
    case 'needs_must_precede_own_stage':
      return { message: t('pipelines.problems.laterDependency', { job: quote(problem.job ?? '?'), dependency: quote(problem.dependency ?? '?') }), job };
    case 'needs_cycle': {
      const cycle = problem.jobs ?? [];
      return { message: t('pipelines.problems.cycle', { cycle: [...cycle, ...cycle.slice(0, 1)].join(' → ') }), job: cycle[0] ?? null };
    }
    case 'invalid_cache_key':
      return { message: t('pipelines.problems.invalidCache', { key: quote(problem.key ?? '?'), job: quote(problem.job ?? '?') }), job };
    default:
      return { message: problem.message, job };
  }
}

export function describeWarning(warning: PipelineWarning): ProblemView {
  const job = warning.job ?? null;
  switch (warning.code) {
    case 'empty_image':
      return { message: t('pipelines.problems.emptyImage', { job: quote(warning.job ?? '?') }), job };
    case 'empty_script':
      return { message: t('pipelines.problems.emptyScript', { job: quote(warning.job ?? '?') }), job };
    case 'duplicate_stage':
      return { message: t('pipelines.problems.duplicateStage', { stage: quote(warning.stage ?? '?') }), job };
    default:
      return { message: warning.code, job };
  }
}
