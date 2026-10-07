import { Component, OnDestroy, OnInit, computed, inject, input, signal } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute, Router, RouterLink } from '@angular/router';
import { map } from 'rxjs';
import { Alert } from '@masmarino/gabarit/alert';
import { Badge } from '@masmarino/gabarit/badge';
import { Button } from '@masmarino/gabarit/button';
import { EmptyState } from '@masmarino/gabarit/empty-state';
import { GbtDateTimePipe, GbtRelativeTimePipe } from '@masmarino/gabarit/format';
import { PageHeader } from '@masmarino/gabarit/page-header';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { PipelineDetail as PipelineDetailModel, PipelinesService } from '../pipelines.service';
import { PageTitleService } from '../../shell/page-title.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { canWrite } from '../../repositories/repository-role';
import { activeStageIndex, durationLabel, groupByStage, isTerminal, pipelineLink } from '../pipeline-helpers';
import { PipelineSidebar } from '../pipeline-sidebar/pipeline-sidebar';
import { PipelineSummary } from '../pipeline-summary/pipeline-summary';
import { PipelineJob } from '../pipeline-job/pipeline-job';
import { StatusBadge } from '../../shared/layout/status-badge/status-badge';

const POLL_INTERVAL_MS = 3000;
const TICK_INTERVAL_MS = 1000;

@Component({
  selector: 'fg-pipeline-detail',
  standalone: true,
  imports: [Button, EmptyState, RouterLink, GbtDateTimePipe, GbtRelativeTimePipe, PageHeader, StatusBadge, PipelineSidebar, PipelineSummary, PipelineJob, Badge, Alert],
  templateUrl: './pipeline-detail.html',
  styleUrl: './pipeline-detail.scss',
})
export class PipelineDetail implements OnInit, OnDestroy {
  repositoryId = input.required<string>();
  pipelineId = input.required<string>();
  path = input.required<string[]>();
  jobId = input<string | null>(null);

  private pipelines = inject(PipelinesService);
  private pageTitle = inject(PageTitleService);
  private toast = inject(GbtToastService);
  private repositoryContext = inject(RepositoryContextService);
  private router = inject(Router);
  private route = inject(ActivatedRoute);
  private pollHandle: ReturnType<typeof setInterval> | null = null;
  private tickHandle: ReturnType<typeof setInterval> | null = null;
  private destroyed = false;
  // Captured per in-flight request: a response that arrives after a newer request started (out-of-order delivery, or
  // overlap with a poll tick) is dropped.
  private requestSeq = 0;

  protected pipeline = signal<PipelineDetailModel | null>(null);
  protected now = signal(Date.now());
  protected role = computed(() => this.repositoryContext.current()?.role ?? null);
  protected canCancel = computed(() => {
    const status = this.pipeline()?.status;
    const role = this.role();
    return (status === 'pending' || status === 'running') && canWrite(role);
  });
  protected groups = computed(() => groupByStage(this.pipeline()?.jobs ?? []));
  protected activeStage = computed(() => activeStageIndex(this.groups()));
  protected currentJob = computed(() => this.pipeline()?.jobs.find((job) => job.id === this.jobId()) ?? null);
  protected selectedStage = toSignal(this.route.queryParamMap.pipe(map((params) => params.get('stage'))), { initialValue: null });
  protected pipelineDuration = computed(() => {
    const pipeline = this.pipeline();
    if (!pipeline) {
      return '—';
    }
    // Pipelines finished before migration 0003 have no finishedAt, so counting up to "now" would be wrong.
    if (isTerminal(pipeline.status) && pipeline.finishedAt === null) {
      return '—';
    }
    return durationLabel(pipeline.createdAt, pipeline.finishedAt, this.now());
  });

  ngOnInit(): void {
    this.pageTitle.set('Pipeline');
    this.refresh();
  }

  ngOnDestroy(): void {
    this.destroyed = true;
    this.stopTimers();
  }

  protected summaryLink(): string[] {
    return pipelineLink(this.path(), this.pipelineId());
  }

  protected openJob(jobId: string): void {
    void this.router.navigate([...this.summaryLink(), 'jobs', jobId]);
  }

  protected selectStage(stage: string): void {
    void this.router.navigate(this.summaryLink(), { queryParams: { stage } });
  }

  private stopTimers(): void {
    if (this.pollHandle !== null) {
      clearInterval(this.pollHandle);
      this.pollHandle = null;
    }
    if (this.tickHandle !== null) {
      clearInterval(this.tickHandle);
      this.tickHandle = null;
    }
  }

  private refresh(): void {
    const seq = ++this.requestSeq;
    this.pipelines.detail(this.pipelineId()).subscribe({
      next: (detail) => {
        if (this.destroyed || seq !== this.requestSeq) {
          return;
        }
        this.pipeline.set(detail);
        this.pageTitle.set(`Pipeline #${detail.id.slice(0, 8)}`);
        this.now.set(Date.now());
        const stillRunning = !isTerminal(detail.status);
        if (stillRunning && this.pollHandle === null) {
          this.pollHandle = setInterval(() => this.refresh(), POLL_INTERVAL_MS);
          this.tickHandle = setInterval(() => this.now.set(Date.now()), TICK_INTERVAL_MS);
        } else if (!stillRunning) {
          this.stopTimers();
        }
      },
      error: () => {
        if (!this.destroyed && seq === this.requestSeq) {
          this.toast.show('Impossible de charger cette pipeline. Réessayez plus tard.', 'error');
        }
      },
    });
  }

  cancel(): void {
    this.pipelines.cancel(this.pipelineId()).subscribe({
      next: () => {
        this.toast.show('Pipeline annulée.');
        this.refresh();
      },
      error: () => this.toast.show("Impossible d'annuler la pipeline.", 'error'),
    });
  }
}
