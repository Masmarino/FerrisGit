import { PipelineProblem, PipelineWarning } from './pipeline-definitions.service';

/** A problem or warning in French, and the job it is about (so the editor can mark its card and open it). */
export interface ProblemView {
  message: string;
  job: string | null;
}

const quote = (name: string) => `« ${name} »`;

export function describeProblem(problem: PipelineProblem): ProblemView {
  const job = problem.job ?? null;
  switch (problem.code) {
    case 'invalid_yaml':
      return { message: `Le fichier n'est pas un YAML valide : ${problem.message.replace(/^invalid YAML: /, '')}`, job };
    case 'unknown_stage':
      return { message: `Le job ${quote(problem.job ?? '?')} est dans l'étape ${quote(problem.stage ?? '?')}, qui n'existe pas.`, job };
    case 'unknown_dependency':
      return { message: `Le job ${quote(problem.job ?? '?')} dépend de ${quote(problem.dependency ?? '?')}, qui n'existe pas.`, job };
    case 'needs_must_precede_own_stage':
      return { message: `Le job ${quote(problem.job ?? '?')} dépend de ${quote(problem.dependency ?? '?')}, qui est dans une étape ultérieure.`, job };
    case 'needs_cycle': {
      const cycle = problem.jobs ?? [];
      return { message: `Des dépendances forment une boucle : ${[...cycle, ...cycle.slice(0, 1)].join(' → ')}.`, job: cycle[0] ?? null };
    }
    case 'invalid_cache_key':
      return { message: `Le cache ${quote(problem.key ?? '?')} du job ${quote(problem.job ?? '?')} n'est pas valide : minuscules, chiffres et tirets seulement.`, job };
    default:
      return { message: problem.message, job };
  }
}

export function describeWarning(warning: PipelineWarning): ProblemView {
  const job = warning.job ?? null;
  switch (warning.code) {
    case 'empty_image':
      return { message: `Le job ${quote(warning.job ?? '?')} n'a pas d'image.`, job };
    case 'empty_script':
      return { message: `Le job ${quote(warning.job ?? '?')} n'a aucune commande.`, job };
    case 'duplicate_stage':
      return { message: `L'étape ${quote(warning.stage ?? '?')} est déclarée deux fois.`, job };
    default:
      return { message: warning.code, job };
  }
}
