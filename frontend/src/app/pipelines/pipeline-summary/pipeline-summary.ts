import { Component, computed, input, output } from '@angular/core';
import { RouterLink } from '@angular/router';
import { JobGraph, JobGraphStage, JobGraphStatus, JobStatus, Stepper, StepperStep } from '@masmarino/gabarit';
import { JobSummary } from '../pipelines.service';
import { STATUS_LABELS, StageGroup, durationLabel } from '../pipeline-helpers';

@Component({
  selector: 'fg-pipeline-summary',
  standalone: true,
  imports: [Stepper, JobGraph, RouterLink, JobStatus],
  templateUrl: './pipeline-summary.html',
  styleUrl: './pipeline-summary.scss',
})
export class PipelineSummary {
  groups = input.required<StageGroup[]>();
  activeStageIndex = input.required<number>();
  selectedStage = input<string | null>(null);
  path = input.required<string[]>();
  pipelineId = input.required<string>();
  now = input.required<number>();

  stageSelected = output<string>();
  jobOpened = output<string>();

  protected readonly statusLabels: Record<JobGraphStatus, string> = STATUS_LABELS;

  protected steps = computed<StepperStep[]>(() =>
    this.groups().map((group) => ({ label: group.name, hasError: group.jobs.some((job) => job.status === 'failed') })),
  );

  protected selectedIndex = computed<number | null>(() => {
    const index = this.groups().findIndex((group) => group.name === this.selectedStage());
    return index === -1 ? null : index;
  });

  protected selectedGroup = computed<StageGroup | null>(() => this.groups().find((group) => group.name === this.selectedStage()) ?? null);

  protected graphStages = computed<JobGraphStage[]>(() =>
    this.groups().map((group) => ({
      name: group.name,
      jobs: group.jobs.map((job) => ({
        id: job.id,
        name: job.name,
        status: job.status,
        durationLabel: this.duration(job),
        needs: job.needs,
      })),
    })),
  );

  protected onStepSelected(index: number | null): void {
    const group = index === null ? undefined : this.groups()[index];
    if (group) {
      this.stageSelected.emit(group.name);
    }
  }

  protected jobLink(jobId: string): string[] {
    return ['/repositories', ...this.path(), '-', 'pipelines', this.pipelineId(), 'jobs', jobId];
  }

  protected duration(job: JobSummary): string {
    return durationLabel(job.startedAt, job.finishedAt, this.now());
  }
}
