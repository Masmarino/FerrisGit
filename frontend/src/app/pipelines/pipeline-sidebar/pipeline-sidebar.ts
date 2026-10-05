import { Component, input } from '@angular/core';
import { RouterLink } from '@angular/router';
import { JobStatus } from '@masmarino/gabarit/job-status';
import { JobSummary } from '../pipelines.service';
import { STATUS_LABELS, StageGroup, jobDurationLabel, jobGlyphStatus, pipelineLink } from '../pipeline-helpers';

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
  protected readonly glyph = jobGlyphStatus;

  protected summaryLink(): string[] {
    return pipelineLink(this.path(), this.pipelineId());
  }

  protected jobLink(jobId: string): string[] {
    return pipelineLink(this.path(), this.pipelineId(), 'jobs', jobId);
  }

  protected duration(job: JobSummary): string {
    return jobDurationLabel(job, this.now());
  }
}
