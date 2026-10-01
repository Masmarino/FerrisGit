import { Component, input } from '@angular/core';
import { RouterLink } from '@angular/router';
import { JobStatus } from '@masmarino/gabarit';
import { JobSummary } from '../pipelines.service';
import { STATUS_LABELS, StageGroup, durationLabel } from '../pipeline-helpers';

@Component({
  selector: 'fg-pipeline-sidebar',
  standalone: true,
  imports: [RouterLink, JobStatus],
  templateUrl: './pipeline-sidebar.html',
  styleUrl: './pipeline-sidebar.scss',
})
export class PipelineSidebar {
  path = input.required<string[]>();
  pipelineId = input.required<string>();
  groups = input.required<StageGroup[]>();
  selectedJobId = input<string | null>(null);
  now = input.required<number>();

  protected readonly statusLabels = STATUS_LABELS;

  protected summaryLink(): string[] {
    return ['/repositories', ...this.path(), '-', 'pipelines', this.pipelineId()];
  }

  protected jobLink(jobId: string): string[] {
    return [...this.summaryLink(), 'jobs', jobId];
  }

  protected duration(job: JobSummary): string {
    return durationLabel(job.startedAt, job.finishedAt, this.now());
  }
}
