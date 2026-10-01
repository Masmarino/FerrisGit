import { Component, OnInit, computed, inject, input, linkedSignal, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { RouterLink } from '@angular/router';
import {
  Badge,
  Button,
  createListToolbarState,
  GbtDateTimePipe,
  GbtInput,
  GbtRelativeTimePipe,
  GbtToastService,
  Icon,
  ListCard,
  ListCardState,
  ListRow,
  ListToolbarSortOption,
  PageHeader,
  PageLayout,
  Pagination,
  SegmentedControl,
  SegmentedControlOption,
  Select,
  SelectOption,
} from '@masmarino/gabarit';
import { PipelineSummary, PipelinesService } from '../pipelines.service';
import { formatDuration } from '../pipeline-helpers';
import { PageTitleService } from '../../shell/page-title.service';
import { StatusBadge, StatusPresentation, statusPresentation } from '../../shared/layout/status-badge/status-badge';

type StatusTab = 'all' | 'active' | 'success' | 'failed';

/** The API returns every pipeline at once: pagination is client-side. */
const PIPELINES_PAGE_SIZE = 25;

const SORT_OPTIONS: ListToolbarSortOption<'date'>[] = [{ value: 'date', label: 'Date' }];

const ORDER_OPTIONS: SelectOption<'asc' | 'desc'>[] = [
  { value: 'desc', label: 'Plus récents' },
  { value: 'asc', label: 'Plus anciens' },
];

/** A queued pipeline is "en cours" too. */
const TAB_STATUSES: Record<Exclude<StatusTab, 'all'>, PipelineSummary['status'][]> = {
  active: ['pending', 'running'],
  success: ['success'],
  failed: ['failed'],
};

const TAB_ADJECTIVES: Record<StatusTab, string> = { all: '', active: ' en cours', success: ' réussi', failed: ' échoué' };

const TERMINAL_STATUSES: ReadonlySet<PipelineSummary['status']> = new Set(['success', 'failed', 'canceled']);

interface PipelineRow {
  pipeline: PipelineSummary;
  link: string[];
  status: StatusPresentation;
  shortId: string;
  shortSha: string;
  tooltip: string;
  duration: string | null;
}

/** Running durations are as of the last load (the list does not tick). Pipelines finished before migration 0003 have no `finishedAt`, so they show no duration rather than a wrong one. */
function durationText(pipeline: PipelineSummary, now: number): string | null {
  const since = formatDuration(now - Date.parse(pipeline.createdAt));
  if (pipeline.status === 'running') {
    return `en cours depuis ${since}`;
  }
  if (pipeline.status === 'pending') {
    return `en attente depuis ${since}`;
  }
  if (!TERMINAL_STATUSES.has(pipeline.status) || pipeline.finishedAt === null) {
    return null;
  }
  return `durée ${formatDuration(Date.parse(pipeline.finishedAt) - Date.parse(pipeline.createdAt))}`;
}

@Component({
  selector: 'fg-pipeline-list',
  standalone: true,
  imports: [
    FormsModule,
    RouterLink,
    GbtDateTimePipe,
    GbtRelativeTimePipe,
    PageLayout,
    PageHeader,
    ListRow,
    StatusBadge,
    Button,
    GbtInput,
    Icon,
    ListCard,
    Pagination,
    SegmentedControl,
    Select,
    Badge,
  ],
  templateUrl: './pipeline-list.html',
  styleUrl: './pipeline-list.scss',
})
export class PipelineList implements OnInit {
  repositoryId = input.required<string>();
  path = input.required<string[]>();

  private pipelines = inject(PipelinesService);
  private pageTitle = inject(PageTitleService);
  private toast = inject(GbtToastService);

  protected list = signal<PipelineSummary[]>([]);
  protected loading = signal(true);
  protected loadFailed = signal(false);
  protected refreshing = signal(false);
  private loadedAt = signal(Date.now());

  private readonly searchSort = createListToolbarState<'date'>({ sortOptions: SORT_OPTIONS, defaultSort: 'date' });
  protected search = this.searchSort.search;
  protected sortValue = this.searchSort.sortValue;
  protected direction = this.searchSort.direction;
  protected readonly orderOptions = ORDER_OPTIONS;
  protected filteredList = this.searchSort.filtered(() => this.list(), {
    // A SHA matches from its start, a commit message anywhere: `text`'s plain substring matching cannot express both.
    matches: (pipeline, query) => pipeline.commitSha.toLowerCase().startsWith(query) || (pipeline.commitMessage?.toLowerCase().includes(query) ?? false),
    sortBy: { date: (pipeline) => pipeline.createdAt },
    locale: 'fr',
  });

  protected tab = signal<StatusTab>('all');
  private byTab = computed(() => {
    const all = this.filteredList();
    const only = (statuses: PipelineSummary['status'][]) => all.filter((pipeline) => statuses.includes(pipeline.status));
    return { all, active: only(TAB_STATUSES.active), success: only(TAB_STATUSES.success), failed: only(TAB_STATUSES.failed) } satisfies Record<StatusTab, PipelineSummary[]>;
  });
  protected tabOptions = computed<SegmentedControlOption<StatusTab>[]>(() => {
    const groups = this.byTab();
    return [
      { value: 'all', label: `Tous (${groups.all.length})` },
      { value: 'active', label: `En cours (${groups.active.length})` },
      { value: 'success', label: `Réussis (${groups.success.length})` },
      { value: 'failed', label: `Échoués (${groups.failed.length})` },
    ];
  });
  protected tabList = computed(() => this.byTab()[this.tab()]);
  protected emptyTabMessage = computed(() => `Aucun pipeline${TAB_ADJECTIVES[this.tab()]}${this.search().trim() !== '' ? ' ne correspond à cette recherche' : ''}`);

  protected readonly pageSize = PIPELINES_PAGE_SIZE;
  protected page = linkedSignal<unknown, number>({
    source: () => [this.search(), this.direction(), this.tab()],
    computation: () => 1,
  });
  /** `page`, clamped to the tab's page count. A refresh can shrink the tab below `page`, and once the pager unmounts nothing else would bring it back. */
  protected currentPage = computed(() => Math.min(this.page(), Math.max(1, Math.ceil(this.tabList().length / PIPELINES_PAGE_SIZE))));
  protected rows = computed<PipelineRow[]>(() => {
    const start = (this.currentPage() - 1) * PIPELINES_PAGE_SIZE;
    const now = this.loadedAt();
    return this.tabList()
      .slice(start, start + PIPELINES_PAGE_SIZE)
      .map((pipeline) => {
        const shortId = pipeline.id.slice(0, 8);
        return {
          pipeline,
          link: ['/repositories', ...this.path(), '-', 'pipelines', pipeline.id],
          status: statusPresentation('pipeline', pipeline.status),
          shortId,
          shortSha: pipeline.commitSha.slice(0, 8),
          tooltip: pipeline.commitMessage ?? `Pipeline #${shortId}`,
          duration: durationText(pipeline, now),
        };
      });
  });
  protected readonly pageLabel = (page: number) => `Page ${page}`;

  protected isEmptyRepository = computed(() => !this.loading() && !this.loadFailed() && this.list().length === 0);

  protected cardState = computed<ListCardState>(() => (this.loading() ? 'loading' : this.loadFailed() ? 'failed' : this.isEmptyRepository() ? 'empty' : 'ready'));

  constructor() {
    // Newest first: with tabs and pages, oldest-first would push the latest runs to the last page.
    this.direction.set('desc');
  }

  ngOnInit(): void {
    this.pageTitle.set('Pipelines');
    this.load();
  }

  protected refresh(): void {
    this.refreshing.set(true);
    this.load();
  }

  private load(): void {
    this.pipelines.listForRepository(this.repositoryId()).subscribe({
      next: (list) => {
        this.loadedAt.set(Date.now());
        this.list.set(list);
        this.loading.set(false);
        this.loadFailed.set(false);
        this.refreshing.set(false);
      },
      error: () => {
        this.loading.set(false);
        // The failed card is already showing and its alert won't announce again, so a second failure gets a toast.
        if (this.loadFailed()) {
          this.toast.show('Impossible de charger les pipelines. Réessayez plus tard.', 'error');
        }
        this.loadFailed.set(true);
        this.refreshing.set(false);
      },
    });
  }
}
