import { Component, ElementRef, afterRenderEffect, computed, input, linkedSignal, output, viewChild } from '@angular/core';
import { JobStatus, Stepper, StepperStep } from '@masmarino/gabarit';
import { JobSummary } from '../pipelines.service';
import { STATUS_LABELS, StageGroup, durationLabel, isNearBottom } from '../pipeline-helpers';

@Component({
  selector: 'fg-pipeline-job',
  standalone: true,
  imports: [Stepper, JobStatus],
  templateUrl: './pipeline-job.html',
  styleUrl: './pipeline-job.scss',
})
export class PipelineJob {
  job = input.required<JobSummary>();
  groups = input.required<StageGroup[]>();
  activeStageIndex = input.required<number>();
  now = input.required<number>();

  stageSelected = output<string>();

  // Following resets to true whenever another job is shown (the component instance is reused across jobs).
  protected readonly follow = linkedSignal<string, boolean>({ source: () => this.job().id, computation: () => true });
  private readonly logElement = viewChild<ElementRef<HTMLElement>>('logs');

  protected statusLabel = computed(() => STATUS_LABELS[this.job().status]);
  protected duration = computed(() => durationLabel(this.job().startedAt, this.job().finishedAt, this.now()));

  protected steps = computed<StepperStep[]>(() =>
    this.groups().map((group) => ({ label: group.name, hasError: group.jobs.some((job) => job.status === 'failed') })),
  );

  protected selectedIndex = computed<number | null>(() => {
    const index = this.groups().findIndex((group) => group.name === this.job().stage);
    return index === -1 ? null : index;
  });

  constructor() {
    afterRenderEffect(() => {
      const { logs, status } = this.job();
      void logs;
      const element = this.logElement()?.nativeElement;
      if (element && status === 'running' && this.follow()) {
        element.scrollTop = element.scrollHeight;
      }
    });
  }

  protected onStepSelected(index: number | null): void {
    const group = index === null ? undefined : this.groups()[index];
    if (group) {
      this.stageSelected.emit(group.name);
    }
  }

  protected onScroll(element: { scrollHeight: number; scrollTop: number; clientHeight: number }): void {
    this.follow.set(isNearBottom(element.scrollHeight, element.scrollTop, element.clientHeight));
  }
}
