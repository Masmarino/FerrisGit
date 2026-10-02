import { Component, OnInit, computed, inject, input, linkedSignal, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { RouterLink } from '@angular/router';
import {
  Badge,
  Button,
  GbtDateTimePipe,
  GbtInput,
  GbtRelativeTimePipe,
  GbtToastService,
  Icon,
  ListCard,
  ListCardState,
  ListRow,
  ListToolbarSortOption,
  Menu,
  MenuItem,
  Modal,
  PageHeader,
  PageLayout,
  Pagination,
  Panel,
  SegmentedControl,
  SegmentedControlOption,
  Select,
  SelectOption,
  Tag,
  Textarea,
  createListToolbarState,
} from '@masmarino/gabarit';
import { BranchInfo, MergeRequestSummary, MergeRequestsService } from '../merge-requests.service';
import { mergeRequestEnd } from '../merge-request-presentation';
import { Label, LabelsService } from '../../labels/labels.service';
import { Milestone, MilestonesService } from '../../milestones/milestones.service';
import { PageTitleService } from '../../shell/page-title.service';
import { injectRepositoryPermissions } from '../../repositories/repository-role';
import { StatusPresentation, statusPresentation } from '../../shared/layout/status-badge/status-badge';

type SortKey = 'date' | 'title';
type StateTab = MergeRequestSummary['status'];

/** Paginated client-side: the API returns every merge request at once. */
const MERGE_REQUESTS_PAGE_SIZE = 25;

const SORT_OPTIONS: ListToolbarSortOption<SortKey>[] = [
  { value: 'date', label: 'Date de création' },
  { value: 'title', label: 'Titre' },
];

const DIRECTION_OPTIONS: SegmentedControlOption<'asc' | 'desc'>[] = [
  { value: 'asc', label: 'Croissant' },
  { value: 'desc', label: 'Décroissant' },
];

const TAB_ADJECTIVES: Record<StateTab, string> = { open: 'ouverte', merged: 'fusionnée', closed: 'fermée' };

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
  imports: [
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
    { value: null, label: 'Tous les milestones' },
    ...this.milestones().map((milestone) => ({ value: milestone.id, label: milestone.title })),
  ]);
  private milestoneTitleById = computed(() => new Map(this.milestones().map((milestone) => [milestone.id, milestone.title])));

  private readonly searchSort = createListToolbarState<SortKey>({ sortOptions: SORT_OPTIONS, defaultSort: 'date' });
  protected search = this.searchSort.search;
  protected sortValue = this.searchSort.sortValue;
  protected direction = this.searchSort.direction;
  protected readonly sortOptions = this.searchSort.sortOptions as SelectOption<SortKey>[];
  protected readonly directionOptions = DIRECTION_OPTIONS;
  protected filteredList = this.searchSort.filtered(() => this.list(), {
    text: (mr) => mr.title,
    sortBy: { title: (mr) => mr.title, date: (mr) => mr.createdAt },
    locale: 'fr',
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
      { value: 'open', label: `Ouvertes (${groups.open.length})` },
      { value: 'merged', label: `Fusionnées (${groups.merged.length})` },
      { value: 'closed', label: `Fermées (${groups.closed.length})` },
    ];
  });
  protected tabList = computed(() => this.byStatus()[this.tab()]);
  protected emptyTabMessage = computed(
    () => `Aucune demande de fusion ${TAB_ADJECTIVES[this.tab()]}${this.hasActiveFilters() ? ' ne correspond à ces filtres' : ''}`,
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
        menuLabel: `Actions de la demande de fusion « ${mergeRequest.title} »`,
      }));
  });
  protected readonly pageLabel = (page: number) => `Page ${page}`;

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
  protected targetError = computed(() => (this.sameBranches() ? 'Identique à la branche source' : null));
  protected titleError = computed(() => (this.titleTouched() && this.title().trim() === '' ? 'Le titre est requis' : null));
  protected canSubmit = computed(
    () => this.sourceBranch() !== '' && this.targetBranch() !== '' && !this.sameBranches() && this.title().trim() !== '' && !this.creating(),
  );

  constructor() {
    // Newest first, otherwise new work lands on the last page. Set here because the other lists keep
    // the toolbar's ascending default.
    this.direction.set('desc');
  }

  ngOnInit(): void {
    this.pageTitle.set('Demandes de fusion');
    this.reload();
    this.mergeRequests.listBranches(this.repositoryId()).subscribe({
      next: (branches) => this.branches.set(branches),
      error: () => this.toast.show('Impossible de charger les branches.', 'error'),
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
          this.toast.show('Impossible de charger les demandes de fusion. Réessayez plus tard.', 'error');
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
          this.toast.show('Demande de fusion créée.');
        },
        // Dialog stays open, nothing typed is lost.
        error: () => {
          this.creating.set(false);
          this.toast.show('Impossible de créer la demande de fusion (branches identiques ou introuvables ?).', 'error');
        },
      });
  }

  protected closeMr(mr: MergeRequestSummary): void {
    this.mergeRequests.close(mr.id).subscribe({
      next: () => {
        this.reload();
        this.toast.show('Demande de fusion fermée.');
      },
      error: () => this.toast.show('Impossible de fermer la demande de fusion. Réessayez plus tard.', 'error'),
    });
  }

  protected mergeMr(mr: MergeRequestSummary): void {
    this.mergeRequests.merge(mr.id).subscribe({
      next: (result) => {
        if (result.outcome === 'conflicting') {
          this.toast.show('Fusion impossible : un conflit a été détecté.', 'error');
          return;
        }
        this.reload();
        this.toast.show('Demande de fusion fusionnée.');
      },
      error: () => this.toast.show('Fusion impossible : vérifiez les approbations requises. Réessayez plus tard.', 'error'),
    });
  }
}
