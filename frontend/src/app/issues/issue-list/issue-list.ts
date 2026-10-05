import { Component, OnInit, computed, inject, input, linkedSignal, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router, RouterLink } from '@angular/router';
import { Badge } from '@masmarino/gabarit/badge';
import { Button } from '@masmarino/gabarit/button';
import { GbtDateTimePipe, GbtRelativeTimePipe } from '@masmarino/gabarit/format';
import { Icon } from '@masmarino/gabarit/icon';
import { GbtInput } from '@masmarino/gabarit/input';
import { ListCard, ListCardState } from '@masmarino/gabarit/list-card';
import { ListRow } from '@masmarino/gabarit/list-row';
import { createListToolbarState, ListToolbarSortOption } from '@masmarino/gabarit/list-toolbar';
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
import { UserChip } from '@masmarino/gabarit/user-chip';
import { Issue, IssuesService, isClosed } from '../issues.service';
import { CreatableIssueKind, ISSUE_KIND_OPTIONS, IssueKindPresentation, issueKindPresentation } from '../issue-kind';
import { createIssueFilters } from '../issue-filters';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { MeService } from '../../shell/me.service';
import { StatusPresentation, statusPresentation } from '../../shared/layout/status-badge/status-badge';
import { PageTitleService } from '../../shell/page-title.service';
import { openWhenAsked } from '../../shared/open-when-asked';

type SortKey = 'date' | 'title';
type StateTab = 'open' | 'closed';

/** Client-side pagination: the API returns every issue of the repository at once. */
const ISSUES_PAGE_SIZE = 25;

const SORT_OPTIONS: ListToolbarSortOption<SortKey>[] = [
  { value: 'date', label: 'Date de création' },
  { value: 'title', label: 'Titre' },
];

const DIRECTION_OPTIONS: SegmentedControlOption<'asc' | 'desc'>[] = [
  { value: 'asc', label: 'Croissant' },
  { value: 'desc', label: 'Décroissant' },
];

interface IssueRow {
  issue: Issue;
  link: string[];
  status: StatusPresentation;
  kind: IssueKindPresentation;
  milestoneTitle: string | null;
  closed: boolean;
}

@Component({
  selector: 'fg-issue-list',
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
    UserChip,
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
  templateUrl: './issue-list.html',
  styleUrl: './issue-list.scss',
})
export class IssueList implements OnInit {
  repositoryId = input.required<string>();
  path = input.required<string[]>();

  private pageTitle = inject(PageTitleService);
  private router = inject(Router);
  private issuesService = inject(IssuesService);
  private repoContext = inject(RepositoryContextService);
  private me = inject(MeService);
  private toast = inject(GbtToastService);

  protected issues = signal<Issue[]>([]);
  protected loading = signal(true);
  protected loadFailed = signal(false);
  protected filters = createIssueFilters(() => this.repositoryId(), () => this.load());

  private readonly searchSort = createListToolbarState<SortKey>({ sortOptions: SORT_OPTIONS, defaultSort: 'date' });
  protected search = this.searchSort.search;
  protected sortValue = this.searchSort.sortValue;
  protected direction = this.searchSort.direction;
  protected readonly sortOptions = this.searchSort.sortOptions as SelectOption<SortKey>[];
  protected readonly directionOptions = DIRECTION_OPTIONS;
  protected filteredIssues = this.searchSort.filtered(() => this.issues(), {
    text: (issue) => issue.title,
    sortBy: { title: (issue) => issue.title, date: (issue) => issue.createdAt },
    locale: 'fr',
  });

  protected tab = signal<StateTab>('open');
  private openIssues = computed(() => this.filteredIssues().filter((issue) => !isClosed(issue)));
  private closedIssues = computed(() => this.filteredIssues().filter(isClosed));
  protected tabOptions = computed<SegmentedControlOption<StateTab>[]>(() => [
    { value: 'open', label: `Ouverts (${this.openIssues().length})` },
    { value: 'closed', label: `Fermés (${this.closedIssues().length})` },
  ]);
  protected tabIssues = computed(() => (this.tab() === 'open' ? this.openIssues() : this.closedIssues()));

  protected readonly pageSize = ISSUES_PAGE_SIZE;
  /** Back to page 1 whenever what the list shows changes (search, sort, tab, server filters). */
  protected page = linkedSignal<unknown, number>({
    source: () => [this.search(), this.sortValue(), this.direction(), this.tab(), this.filters.selectedLabelIds(), this.filters.selectedMilestoneId()],
    computation: () => 1,
  });
  /** `page` clamped to the tab's page count. Closing the only issue of the last page shrinks the tab under `page` and may unmount the pager, leaving an empty page. */
  protected currentPage = computed(() => Math.min(this.page(), Math.max(1, Math.ceil(this.tabIssues().length / ISSUES_PAGE_SIZE))));
  protected rows = computed<IssueRow[]>(() => {
    const start = (this.currentPage() - 1) * ISSUES_PAGE_SIZE;
    const milestoneTitles = this.filters.milestoneTitleById();
    return this.tabIssues()
      .slice(start, start + ISSUES_PAGE_SIZE)
      .map((issue) => ({
        issue,
        link: ['/repositories', ...this.path(), '-', 'issues', String(issue.number)],
        status: statusPresentation('issue', issue.status),
        kind: issueKindPresentation(issue.kind),
        milestoneTitle: issue.milestoneId ? (milestoneTitles.get(issue.milestoneId) ?? null) : null,
        closed: isClosed(issue),
      }));
  });
  protected readonly pageLabel = (page: number) => `Page ${page}`;

  protected hasActiveFilters = computed(() => this.search().trim() !== '' || this.filters.hasSelection());
  protected isEmptyRepository = computed(() => !this.loading() && !this.loadFailed() && this.issues().length === 0 && !this.filters.hasSelection());

  protected cardState = computed<ListCardState>(() => (this.loading() ? 'loading' : this.loadFailed() ? 'failed' : this.isEmptyRepository() ? 'empty' : 'ready'));

  protected canWrite = computed(() => {
    const role = this.repoContext.current()?.role;
    return role === 'owner' || role === 'contributor' || role === 'maintainer';
  });

  protected createOpen = signal(false);
  protected creating = signal(false);
  protected newTitle = signal('');
  protected newDescription = signal('');
  protected newKind = signal<CreatableIssueKind>('task');
  protected titleTouched = signal(false);
  protected readonly kindOptions = ISSUE_KIND_OPTIONS;
  protected canSubmit = computed(() => this.newTitle().trim() !== '' && !this.creating());
  protected titleError = computed(() => (this.titleTouched() && this.newTitle().trim() === '' ? 'Le titre est requis' : null));

  constructor() {
    // Newest first: with tabs and pages, oldest-first would bury new work on the last page. Set here, not in
    // `createListToolbarState`, whose `asc` default the other lists keep.
    this.direction.set('desc');
    openWhenAsked('issue', () => this.openCreate());
  }

  ngOnInit(): void {
    this.pageTitle.set('Tickets');
    this.load();
    this.filters.loadOptions();
  }

  protected load(): void {
    this.issuesService.list(this.repositoryId(), this.filters.params()).subscribe({
      next: (issues) => {
        this.issues.set(issues);
        this.loading.set(false);
        this.loadFailed.set(false);
      },
      error: () => {
        this.loading.set(false);
        // Already on the failed card: a second failure gets a toast, since the alert is unchanged and wouldn't be
        // announced again.
        if (this.loadFailed()) {
          this.toast.show('Impossible de charger les tickets. Réessayez plus tard.', 'error');
        }
        this.loadFailed.set(true);
      },
    });
  }

  protected resetFilters(): void {
    this.search.set('');
    this.filters.clear();
  }

  protected openCreate(): void {
    this.createOpen.set(true);
  }

  protected closeCreate(): void {
    this.createOpen.set(false);
    this.resetDraft();
  }

  private resetDraft(): void {
    this.newTitle.set('');
    this.newDescription.set('');
    this.newKind.set('task');
    this.titleTouched.set(false);
  }

  createIssue(): void {
    if (!this.canSubmit()) {
      this.titleTouched.set(true);
      return;
    }
    this.creating.set(true);
    this.issuesService.create(this.repositoryId(), this.newTitle(), this.newDescription(), this.newKind()).subscribe({
      next: () => {
        this.creating.set(false);
        this.closeCreate();
        this.load();
        this.toast.show('Ticket créé.');
      },
      // The dialog stays open with the draft, so nothing typed is lost.
      error: () => {
        this.creating.set(false);
        this.toast.show('Impossible de créer le ticket.', 'error');
      },
    });
  }

  openKanban(): void {
    this.router.navigate(['/repositories', ...this.path(), '-', 'issues', 'board']);
  }

  protected toggleClosed(issue: Issue): void {
    const reopening = isClosed(issue);
    const action = reopening ? this.issuesService.reopen(this.repositoryId(), issue.number) : this.issuesService.close(this.repositoryId(), issue.number);
    action.subscribe({
      next: (updated) => {
        this.replace(updated);
        this.toast.show(reopening ? 'Ticket rouvert.' : 'Ticket fermé.');
      },
      error: () => this.toast.show('Impossible de mettre à jour le ticket. Réessayez plus tard.', 'error'),
    });
  }

  protected assignToMe(issue: Issue): void {
    if (!this.me.id()) {
      return;
    }
    this.issuesService.assign(this.repositoryId(), issue.number, this.me.id()).subscribe({
      next: (updated) => {
        this.replace(updated);
        this.toast.show('Ticket assigné.');
      },
      error: () => this.toast.show('Impossible de vous assigner le ticket. Réessayez plus tard.', 'error'),
    });
  }

  private replace(updated: Issue): void {
    this.issues.update((list) => list.map((i) => (i.id === updated.id ? updated : i)));
  }
}
