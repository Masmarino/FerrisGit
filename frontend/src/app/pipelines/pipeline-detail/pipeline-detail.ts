import { Component, OnDestroy, OnInit, computed, inject, input, signal } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute, Router, RouterLink } from '@angular/router';
import { map } from 'rxjs';
import { Badge, Button, EmptyState, GbtDateTimePipe, GbtRelativeTimePipe, PageHeader, GbtToastService } from '@masmarino/gabarit';
import { PipelineDetail as PipelineDetailModel, PipelinesService } from '../pipelines.service';
import { PageTitleService } from '../../shell/page-title.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { activeStageIndex, durationLabel, groupByStage } from '../pipeline-helpers';
import { PipelineSidebar } from '../pipeline-sidebar/pipeline-sidebar';
import { PipelineSummary } from '../pipeline-summary/pipeline-summary';
import { PipelineJob } from '../pipeline-job/pipeline-job';
import { StatusBadge } from '../../shared/layout/status-badge/status-badge';

const TERMINAL_STATUSES = new Set(['success', 'failed', 'canceled']);
const POLL_INTERVAL_MS = 3000;
const TICK_INTERVAL_MS = 1000;

@Component({
  selector: 'fg-pipeline-detail',
  standalone: true,
  imports: [Button, EmptyState, RouterLink, GbtDateTimePipe, GbtRelativeTimePipe, PageHeader, StatusBadge, PipelineSidebar, PipelineSummary, PipelineJob, Badge],
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
  // Captured for each in-flight request. A response that arrives after a newer request started (out-of-order delivery,
  // or overlap with a poll tick) is dropped.
  private requestSeq = 0;

  protected pipeline = signal<PipelineDetailModel | null>(null);
  protected now = signal(Date.now());
  protected role = computed(() => this.repositoryContext.current()?.role ?? null);
  protected groups = computed(() => groupByStage(this.pipeline()?.jobs ?? []));
  protected activeStage = computed(() => activeStageIndex(this.groups()));
  protected currentJob = computed(() => this.pipeline()?.jobs.find((job) => job.id === this.jobId()) ?? null);
  protected selectedStage = toSignal(this.route.queryParamMap.pipe(map((params) => params.get('stage'))), { initialValue: null });
  protected pipelineDuration = computed(() => {
    const pipeline = this.pipeline();
    if (!pipeline) {
      return '—';
    }
    // Pipelines finished before migration 0003 have no finishedAt: counting up to "now" would be wrong.
    if (TERMINAL_STATUSES.has(pipeline.status) && pipeline.finishedAt === null) {
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
    return ['/repositories', ...this.path(), '-', 'pipelines', this.pipelineId()];
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
        const stillRunning = !TERMINAL_STATUSES.has(detail.status);
        if (stillRunning && this.pollHandle === null) {
          this.pollHandle = setInterval(() => this.refresh(), POLL_INTERVAL_MS);
          this.tickHandle = setInterval(() => this.now.set(Date.now()), TICK_INTERVAL_MS);
        } else if (!stillRunning) {
          this.stopTimers();
        }
      },
      error: () => {
        if (!this.destroyed && seq === this.requestSeq) {
          this.toast.show('Impossible de charger ce pipeline. Réessayez plus tard.', 'error');
        }
      },
    });
  }

  cancel(): void {
    this.pipelines.cancel(this.pipelineId()).subscribe({
      next: () => {
        this.toast.show('Pipeline annulé.');
        this.refresh();
      },
      error: () => this.toast.show("Impossible d'annuler le pipeline.", 'error'),
    });
  }
}
