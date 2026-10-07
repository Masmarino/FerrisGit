import { Component, OnInit, computed, inject, input, linkedSignal, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { RouterLink } from '@angular/router';
import { Badge } from '@masmarino/gabarit/badge';
import { Button } from '@masmarino/gabarit/button';
import { GbtDateTimePipe, GbtRelativeTimePipe } from '@masmarino/gabarit/format';
import { Icon } from '@masmarino/gabarit/icon';
import { GbtInput } from '@masmarino/gabarit/input';
import { ListCard, ListCardState } from '@masmarino/gabarit/list-card';
import { ListRow } from '@masmarino/gabarit/list-row';
import { createListToolbarState, ListToolbarSortOption } from '@masmarino/gabarit/list-toolbar';
import { PageHeader } from '@masmarino/gabarit/page-header';
import { PageLayout } from '@masmarino/gabarit/page-layout';
import { Pagination } from '@masmarino/gabarit/pagination';
import { SegmentedControl, SegmentedControlOption } from '@masmarino/gabarit/segmented-control';
import { Select, SelectOption } from '@masmarino/gabarit/select';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { PipelineSummary, PipelinesService } from '../pipelines.service';
import { formatDuration, isTerminal, pipelineLink } from '../pipeline-helpers';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { canWrite } from '../../repositories/repository-role';
import { PageTitleService } from '../../shell/page-title.service';
import { StatusBadge, StatusPresentation, statusPresentation } from '../../shared/layout/status-badge/status-badge';

type StatusTab = 'all' | 'active' | 'success' | 'failed';

/** The API returns every pipeline at once, so pagination is client-side. */
const PIPELINES_PAGE_SIZE = 25;

const SORT_OPTIONS: ListToolbarSortOption<'date'>[] = [{ value: 'date', label: 'Date' }];

const ORDER_OPTIONS: SelectOption<'asc' | 'desc'>[] = [
  { value: 'desc', label: 'Plus récentes' },
  { value: 'asc', label: 'Plus anciennes' },
];

/** A queued pipeline counts as "en cours" too. */
const TAB_STATUSES: Record<Exclude<StatusTab, 'all'>, PipelineSummary['status'][]> = {
  active: ['pending', 'running'],
  success: ['success'],
  failed: ['failed'],
};

const TAB_ADJECTIVES: Record<StatusTab, string> = { all: '', active: ' en cours', success: ' réussie', failed: ' échouée' };

interface PipelineRow {
  pipeline: PipelineSummary;
  link: string[];
  status: StatusPresentation;
  shortId: string;
  shortSha: string;
  tooltip: string;
  duration: string | null;
}

/** Running durations are as of the last load (the list doesn't tick). Pipelines finished before migration 0003 have no `finishedAt`, so they show no duration rather than a wrong one. */
function durationText(pipeline: PipelineSummary, now: number): string | null {
  // An invalid pipeline file fails at once and nothing ever ran, so a duration of 0s would be noise.
  if (pipeline.error !== null) {
    return null;
  }
  const since = formatDuration(now - Date.parse(pipeline.createdAt));
  if (pipeline.status === 'running') {
    return `en cours depuis ${since}`;
  }
  if (pipeline.status === 'pending') {
    return `en attente depuis ${since}`;
  }
  if (!isTerminal(pipeline.status) || pipeline.finishedAt === null) {
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
  private repoContext = inject(RepositoryContextService);

  /** Contributors and above can open the editor, which prepares a change to the pipeline file. */
  protected canEdit = computed(() => canWrite(this.repoContext.current()?.role ?? null));
  protected editorLink = computed(() => ['/repositories', ...this.path(), '-', 'pipelines', 'editor']);

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
    // A SHA matches from its start, a commit message anywhere: `text`'s plain substring matching can't express both.
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
      { value: 'all', label: `Toutes (${groups.all.length})` },
      { value: 'active', label: `En cours (${groups.active.length})` },
      { value: 'success', label: `Réussies (${groups.success.length})` },
      { value: 'failed', label: `Échouées (${groups.failed.length})` },
    ];
  });
  protected tabList = computed(() => this.byTab()[this.tab()]);
  protected emptyTabMessage = computed(() => `Aucune pipeline${TAB_ADJECTIVES[this.tab()]}${this.search().trim() !== '' ? ' ne correspond à cette recherche' : ''}`);

  protected readonly pageSize = PIPELINES_PAGE_SIZE;
  protected page = linkedSignal<unknown, number>({
    source: () => [this.search(), this.direction(), this.tab()],
    computation: () => 1,
  });
  /** `page` clamped to the tab's page count. A refresh can shrink the tab below `page`, and once the pager unmounts nothing else would bring it back. */
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
          link: pipelineLink(this.path(), pipeline.id),
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
