import { JobGraphStatus, JobStatusValue, StepperStep } from '@masmarino/gabarit';
import { JobSummary, PipelineSummary } from './pipelines.service';

export interface StageGroup {
  name: string;
  jobs: JobSummary[];
}

export const STATUS_LABELS: Record<JobSummary['status'], string> = {
  pending: 'En attente',
  running: 'En cours',
  success: 'Réussi',
  failed: 'Échoué',
  canceled: 'Annulé',
  skipped: 'Ignoré',
};

/**
 * Gabarit's job glyphs stop at the five statuses of a run: a skipped job wears the grey cross of a canceled one, and its
 * label ("Ignoré") tells them apart.
 */
export function jobGlyphStatus(status: JobSummary['status']): JobStatusValue {
  return status === 'skipped' ? 'canceled' : status;
}

/**
 * The job graph reads its screen-reader label from the status itself, so it gets the real one (its glyph then falls back
 * to Gabarit's neutral ring): "Annulé" would be wrong for a job nobody cancelled.
 */
export function jobGraphStatus(status: JobSummary['status']): JobGraphStatus {
  return status as JobGraphStatus;
}

/** Jobs are created and listed in stage order (see CreatePipelineUseCase), so first appearance gives the stage order. */
export function groupByStage(jobs: JobSummary[]): StageGroup[] {
  const groups = new Map<string, JobSummary[]>();
  for (const job of jobs) {
    const existing = groups.get(job.stage);
    if (existing) {
      existing.push(job);
    } else {
      groups.set(job.stage, [job]);
    }
  }
  return [...groups].map(([name, stageJobs]) => ({ name, jobs: stageJobs }));
}

/** One stepper step per stage, flagged when one of its jobs failed. */
export function stageSteps(groups: StageGroup[]): StepperStep[] {
  return groups.map((group) => ({ label: group.name, hasError: group.jobs.some((job) => job.status === 'failed') }));
}

/** The position of the stage called `name`, or `null` when there is none (or no name). */
export function stageIndexOf(groups: StageGroup[], name: string | null): number | null {
  const index = groups.findIndex((group) => group.name === name);
  return index === -1 ? null : index;
}

export function isTerminal(status: PipelineSummary['status']): boolean {
  return status === 'success' || status === 'failed' || status === 'canceled';
}

/** The route of a pipeline's page, or of one of its sub-pages (`'jobs', jobId`). */
export function pipelineLink(path: string[], pipelineId: string, ...rest: string[]): string[] {
  return ['/repositories', ...path, '-', 'pipelines', pipelineId, ...rest];
}

export function activeStageIndex(groups: StageGroup[]): number {
  const index = groups.findIndex((group) => !group.jobs.every((job) => job.status === 'success'));
  return index === -1 ? groups.length : index;
}

export function formatDuration(ms: number): string {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  if (seconds < 60) {
    return `${seconds}s`;
  }
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) {
    return `${minutes}m ${seconds % 60}s`;
  }
  return `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
}

export function durationLabel(startedAt: string | null, finishedAt: string | null, now: number): string {
  if (!startedAt) {
    return '—';
  }
  const end = finishedAt ? Date.parse(finishedAt) : now;
  return formatDuration(end - Date.parse(startedAt));
}

/** A skipped job never ran, so it has no duration to show: it says why instead. */
export function jobDurationLabel(job: Pick<JobSummary, 'status' | 'startedAt' | 'finishedAt'>, now: number): string {
  return job.status === 'skipped' ? 'ignoré' : durationLabel(job.startedAt, job.finishedAt, now);
}

export function isNearBottom(scrollHeight: number, scrollTop: number, clientHeight: number, threshold = 24): boolean {
  return scrollHeight - scrollTop - clientHeight <= threshold;
}
