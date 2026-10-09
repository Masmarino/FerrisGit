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
import { ListToolbarSortOption, createListToolbarState } from '@masmarino/gabarit/list-toolbar';
import { Menu, MenuItem } from '@masmarino/gabarit/menu';
import { Modal } from '@masmarino/gabarit/modal';
import { PageHeader } from '@masmarino/gabarit/page-header';
import { PageLayout } from '@masmarino/gabarit/page-layout';
import { Pagination } from '@masmarino/gabarit/pagination';
import { Panel } from '@masmarino/gabarit/panel';
import { SegmentedControl, SegmentedControlOption } from '@masmarino/gabarit/segmented-control';
import { Select, SelectOption } from '@masmarino/gabarit/select';
import { Tag } from '@masmarino/gabarit/tag';
import { Textarea } from '@masmarino/gabarit/textarea';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { BranchInfo, MergeRequestSummary, MergeRequestsService } from '../merge-requests.service';
import { mergeRequestEnd } from '../merge-request-presentation';
import { Label, LabelsService } from '../../labels/labels.service';
import { Milestone, MilestonesService } from '../../milestones/milestones.service';
import { PageTitleService } from '../../shell/page-title.service';
import { injectRepositoryPermissions } from '../../repositories/repository-role';
import { StatusPresentation, statusPresentation } from '../../shared/layout/status-badge/status-badge';
import { openWhenAsked } from '../../shared/open-when-asked';
import { activeLocale, t } from '../../shared/i18n/translator';
import { TranslocoPipe } from '@jsverse/transloco';

type SortKey = 'date' | 'title';
type StateTab = MergeRequestSummary['status'];

/** Paginated client-side: the API returns every merge request at once. */
const MERGE_REQUESTS_PAGE_SIZE = 25;

const sortOptions = (): ListToolbarSortOption<SortKey>[] => [
  { value: 'date', label: t('common.createdAt') },
  { value: 'title', label: t('common.title') },
];

const directionOptions = (): SegmentedControlOption<'asc' | 'desc'>[] => [
  { value: 'asc', label: t('common.ascending') },
  { value: 'desc', label: t('common.descending') },
];

const EMPTY_TAB_MESSAGES: Record<StateTab, { all: string; filtered: string }> = {
  open: { all: 'mergeRequests.emptyTab.open', filtered: 'mergeRequests.emptyTab.openFiltered' },
  merged: { all: 'mergeRequests.emptyTab.merged', filtered: 'mergeRequests.emptyTab.mergedFiltered' },
  closed: { all: 'mergeRequests.emptyTab.closed', filtered: 'mergeRequests.emptyTab.closedFiltered' },
};

interface MergeRequestRow {
  mergeRequest: MergeRequestSummary;
  link: string[];
  status: StatusPresentation;
  milestoneTitle: string | null;
  branchesTitle: string;
  ended: { verb: string; at: string } | null;
  menuLabel: string;
}

@Component({
  selector: 'fg-merge-request-list',
  standalone: true,
  imports: [TranslocoPipe, 
    FormsModule,
    RouterLink,
    GbtDateTimePipe,
    GbtRelativeTimePipe,
    PageLayout,
    PageHeader,
    Panel,
    ListRow,
    Badge,
    Button,
    GbtInput,
    Icon,
    ListCard,
    Menu,
    MenuItem,
    Modal,
    Pagination,
    SegmentedControl,
    Select,
    Tag,
    Textarea,
  ],
  templateUrl: './merge-request-list.html',
  styleUrl: './merge-request-list.scss',
})
export class MergeRequestList implements OnInit {
  repositoryId = input.required<string>();
  path = input.required<string[]>();

  private mergeRequests = inject(MergeRequestsService);
  private labelsService = inject(LabelsService);
  private milestonesService = inject(MilestonesService);
  private pageTitle = inject(PageTitleService);
  private permissions = injectRepositoryPermissions();
  private toast = inject(GbtToastService);

  protected list = signal<MergeRequestSummary[]>([]);
  protected loading = signal(true);
  protected loadFailed = signal(false);
  protected branches = signal<BranchInfo[]>([]);
  protected labels = signal<Label[]>([]);
  protected milestones = signal<Milestone[]>([]);
  protected selectedLabelIds = signal<string[]>([]);
  protected selectedMilestoneId = signal<string | null>(null);
  protected canWrite = this.permissions.canWrite;
  /** The server only lets the owner and Maintainers merge, so a Contributor gets no menu entry. */
  protected canMerge = this.permissions.canMaintain;

  protected labelOptions = computed<SelectOption<string>[]>(() => this.labels().map((label) => ({ value: label.id, label: label.name, color: label.color })));
  protected milestoneOptions = computed<SelectOption<string | null>[]>(() => [
    { value: null, label: t('issues.allMilestones') },
    ...this.milestones().map((milestone) => ({ value: milestone.id, label: milestone.title })),
  ]);
  private milestoneTitleById = computed(() => new Map(this.milestones().map((milestone) => [milestone.id, milestone.title])));

  private readonly searchSort = createListToolbarState<SortKey>({ sortOptions: sortOptions(), defaultSort: 'date' });
  protected search = this.searchSort.search;
  protected sortValue = this.searchSort.sortValue;
  protected direction = this.searchSort.direction;
  protected readonly sortOptions = this.searchSort.sortOptions as SelectOption<SortKey>[];
  protected readonly directionOptions = directionOptions();
  protected filteredList = this.searchSort.filtered(() => this.list(), {
    text: (mr) => mr.title,
    sortBy: { title: (mr) => mr.title, date: (mr) => mr.createdAt },
    locale: activeLocale(),
  });

  protected tab = signal<StateTab>('open');
  private byStatus = computed(() => {
    const groups: Record<StateTab, MergeRequestSummary[]> = { open: [], merged: [], closed: [] };
    for (const mr of this.filteredList()) {
      groups[mr.status]?.push(mr);
    }
    return groups;
  });
  protected tabOptions = computed<SegmentedControlOption<StateTab>[]>(() => {
    const groups = this.byStatus();
    return [
      { value: 'open', label: t('mergeRequests.openTab', { count: groups.open.length }) },
      { value: 'merged', label: t('mergeRequests.mergedTab', { count: groups.merged.length }) },
      { value: 'closed', label: t('mergeRequests.closedTab', { count: groups.closed.length }) },
    ];
  });
  protected tabList = computed(() => this.byStatus()[this.tab()]);
  protected emptyTabMessage = computed(
    () => t(EMPTY_TAB_MESSAGES[this.tab()][this.hasActiveFilters() ? 'filtered' : 'all']),
  );

  protected readonly pageSize = MERGE_REQUESTS_PAGE_SIZE;
  /** Back to page 1 when what the list shows changes (search, sort, tab, server filters). */
  protected page = linkedSignal<unknown, number>({
    source: () => [this.search(), this.sortValue(), this.direction(), this.tab(), this.selectedLabelIds(), this.selectedMilestoneId()],
    computation: () => 1,
  });
  /** `page` clamped to the tab's page count: closing the only request on the last page shrinks the tab and can unmount the pager, leaving an empty page. */
  protected currentPage = computed(() => Math.min(this.page(), Math.max(1, Math.ceil(this.tabList().length / MERGE_REQUESTS_PAGE_SIZE))));
  protected rows = computed<MergeRequestRow[]>(() => {
    const start = (this.currentPage() - 1) * MERGE_REQUESTS_PAGE_SIZE;
    const milestoneTitles = this.milestoneTitleById();
    return this.tabList()
      .slice(start, start + MERGE_REQUESTS_PAGE_SIZE)
      .map((mergeRequest) => ({
        mergeRequest,
        link: ['/repositories', ...this.path(), '-', 'merge-requests', mergeRequest.id],
        status: statusPresentation('merge-request', mergeRequest.status),
        milestoneTitle: mergeRequest.milestoneId ? (milestoneTitles.get(mergeRequest.milestoneId) ?? null) : null,
        branchesTitle: `${mergeRequest.sourceBranch} → ${mergeRequest.targetBranch}`,
        ended: mergeRequestEnd(mergeRequest),
        menuLabel: t('mergeRequests.actionsFor', { title: mergeRequest.title }),
      }));
  });
  protected readonly pageLabel = (page: number) => t('common.pageNumber', { page });

  protected hasServerFilters = computed(() => this.selectedLabelIds().length > 0 || this.selectedMilestoneId() !== null);
  protected hasActiveFilters = computed(() => this.search().trim() !== '' || this.hasServerFilters());
  protected isEmptyRepository = computed(() => !this.loading() && !this.loadFailed() && this.list().length === 0 && !this.hasServerFilters());

  protected cardState = computed<ListCardState>(() => (this.loading() ? 'loading' : this.loadFailed() ? 'failed' : this.isEmptyRepository() ? 'empty' : 'ready'));

  protected branchOptions = computed<SelectOption<string>[]>(() => this.branches().map((b) => ({ value: b.name, label: b.name })));
  private defaultBranch = computed(() => this.branches().find((b) => b.isDefault)?.name ?? '');

  protected createOpen = signal(false);
  protected creating = signal(false);
  protected sourceBranch = signal('');
  /** Follows the default branch once branches arrive, until the user picks one. */
  protected targetBranch = linkedSignal(() => this.defaultBranch());
  protected title = signal('');
  protected description = signal('');
  protected titleTouched = signal(false);
  protected sameBranches = computed(() => this.sourceBranch() !== '' && this.sourceBranch() === this.targetBranch());
  protected targetError = computed(() => (this.sameBranches() ? t('mergeRequests.sameBranch') : null));
  protected titleError = computed(() => (this.titleTouched() && this.title().trim() === '' ? t('issues.titleRequired') : null));
  protected canSubmit = computed(
    () => this.sourceBranch() !== '' && this.targetBranch() !== '' && !this.sameBranches() && this.title().trim() !== '' && !this.creating(),
  );

  constructor() {
    // Newest first, otherwise new work lands on the last page. Set here because the other lists keep
    // the toolbar's ascending default.
    this.direction.set('desc');
    openWhenAsked('merge-request', () => this.openCreate());
  }

  ngOnInit(): void {
    this.pageTitle.set(t('nav.mergeRequests'));
    this.reload();
    this.mergeRequests.listBranches(this.repositoryId()).subscribe({
      next: (branches) => this.branches.set(branches),
      error: () => this.toast.show(t('mergeRequests.branchesFailed'), 'error'),
    });
    this.labelsService.listForRepository(this.repositoryId()).subscribe({ next: (labels) => this.labels.set(labels) });
    this.milestonesService.listForRepository(this.repositoryId()).subscribe({ next: (milestones) => this.milestones.set(milestones) });
  }

  protected reload(): void {
    this.mergeRequests.listForRepository(this.repositoryId(), { labelIds: this.selectedLabelIds(), milestoneId: this.selectedMilestoneId() ?? undefined }).subscribe({
      next: (list) => {
        this.list.set(list);
        this.loading.set(false);
        this.loadFailed.set(false);
      },
      error: () => {
        this.loading.set(false);
        // The failed card is already up and won't announce a second time, so use a toast.
        if (this.loadFailed()) {
          this.toast.show(t('mergeRequests.loadFailedListToast'), 'error');
        }
        this.loadFailed.set(true);
      },
    });
  }

  protected onLabelFilterChange(labelIds: string[]): void {
    this.selectedLabelIds.set(labelIds);
    this.reload();
  }

  protected onMilestoneFilterChange(milestoneId: string | null): void {
    this.selectedMilestoneId.set(milestoneId);
    this.reload();
  }

  protected resetFilters(): void {
    const refetch = this.hasServerFilters();
    this.search.set('');
    this.selectedLabelIds.set([]);
    this.selectedMilestoneId.set(null);
    if (refetch) {
      this.reload();
    }
  }

  protected openCreate(): void {
    this.createOpen.set(true);
  }

  protected closeCreate(): void {
    this.createOpen.set(false);
    this.resetDraft();
  }

  private resetDraft(): void {
    this.sourceBranch.set('');
    this.targetBranch.set(this.defaultBranch());
    this.title.set('');
    this.description.set('');
    this.titleTouched.set(false);
  }

  protected createMergeRequest(): void {
    if (!this.canSubmit()) {
      this.titleTouched.set(true);
      return;
    }
    this.creating.set(true);
    this.mergeRequests
      .create(this.repositoryId(), {
        sourceBranch: this.sourceBranch(),
        targetBranch: this.targetBranch(),
        title: this.title(),
        description: this.description(),
      })
      .subscribe({
        next: () => {
          this.creating.set(false);
          this.closeCreate();
          this.reload();
          this.toast.show(t('mergeRequests.created'));
        },
        // Dialog stays open, nothing typed is lost.
        error: () => {
          this.creating.set(false);
          this.toast.show(t('mergeRequests.createFailed'), 'error');
        },
      });
  }

  protected closeMr(mr: MergeRequestSummary): void {
    this.mergeRequests.close(mr.id).subscribe({
      next: () => {
        this.reload();
        this.toast.show(t('mergeRequests.closedToast'));
      },
      error: () => this.toast.show(t('mergeRequests.closeFailedLater'), 'error'),
    });
  }

  protected mergeMr(mr: MergeRequestSummary): void {
    this.mergeRequests.merge(mr.id).subscribe({
      next: (result) => {
        if (result.outcome === 'conflicting') {
          this.toast.show(t('mergeRequests.mergeConflict'), 'error');
          return;
        }
        this.reload();
        this.toast.show(t('mergeRequests.mergedToast'));
      },
      error: () => this.toast.show(t('mergeRequests.mergeFailed'), 'error'),
    });
  }
}
