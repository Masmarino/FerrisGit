import { JobSummary } from './pipelines.service';

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
};

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

export function isNearBottom(scrollHeight: number, scrollTop: number, clientHeight: number, threshold = 24): boolean {
  return scrollHeight - scrollTop - clientHeight <= threshold;
}
