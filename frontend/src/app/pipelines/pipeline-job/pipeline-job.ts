import { Component, ElementRef, afterRenderEffect, computed, input, linkedSignal, output, viewChild } from '@angular/core';
import { JobStatus } from '@masmarino/gabarit/job-status';
import { Stepper } from '@masmarino/gabarit/stepper';
import { JobSummary } from '../pipelines.service';
import { STATUS_LABELS, StageGroup, isNearBottom, jobDurationLabel, jobGlyphStatus, stageIndexOf, stageSteps } from '../pipeline-helpers';
import { activeLocale, t } from '../../shared/i18n/translator';
import { TranslocoPipe } from '@jsverse/transloco';

const purgeDate = () => new Intl.DateTimeFormat(activeLocale(), { day: 'numeric', month: 'long', year: 'numeric' });

@Component({
  selector: 'fg-pipeline-job',
  standalone: true,
  imports: [TranslocoPipe, Stepper, JobStatus],
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
  protected glyph = computed(() => jobGlyphStatus(this.job().status));
  protected duration = computed(() => jobDurationLabel(this.job(), this.now()));
  protected logPlaceholder = computed(() => {
    const { status, logsPurgedAt } = this.job();
    if (logsPurgedAt) {
      return t('pipelines.logsPurged', { date: purgeDate().format(new Date(logsPurgedAt)) });
    }
    return status === 'skipped' ? t('pipelines.notStarted') : t('pipelines.noLogsYet');
  });

  protected steps = computed(() => stageSteps(this.groups()));
  protected selectedIndex = computed(() => stageIndexOf(this.groups(), this.job().stage));

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
