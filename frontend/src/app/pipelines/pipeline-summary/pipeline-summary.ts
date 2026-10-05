import { Component, computed, input, output } from '@angular/core';
import { RouterLink } from '@angular/router';
import { JobGraph, JobGraphStage } from '@masmarino/gabarit/job-graph';
import { JobStatus } from '@masmarino/gabarit/job-status';
import { Stepper } from '@masmarino/gabarit/stepper';
import { JobSummary } from '../pipelines.service';
import { STATUS_LABELS, StageGroup, jobDurationLabel, jobGlyphStatus, jobGraphStatus, pipelineLink, stageIndexOf, stageSteps } from '../pipeline-helpers';

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

  // Gabarit types the labels by its five statuses; ours also has `skipped`, which the graph reads at run time.
  protected readonly statusLabels = STATUS_LABELS;
  protected readonly glyph = jobGlyphStatus;

  protected steps = computed(() => stageSteps(this.groups()));
  protected selectedIndex = computed(() => stageIndexOf(this.groups(), this.selectedStage()));

  protected selectedGroup = computed<StageGroup | null>(() => this.groups().find((group) => group.name === this.selectedStage()) ?? null);

  protected graphStages = computed<JobGraphStage[]>(() =>
    this.groups().map((group) => ({
      name: group.name,
      jobs: group.jobs.map((job) => ({
        id: job.id,
        name: job.name,
        status: jobGraphStatus(job.status),
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
    return pipelineLink(this.path(), this.pipelineId(), 'jobs', jobId);
  }

  protected duration(job: JobSummary): string {
    return jobDurationLabel(job, this.now());
  }
}
