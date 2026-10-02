import { Component, computed, DestroyRef, effect, inject, linkedSignal, OnInit, signal } from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { ActivatedRoute, RouterLink } from '@angular/router';
import { catchError, map, Observable, of, startWith, Subject, switchMap } from 'rxjs';
import {
  Badge,
  Button,
  Card,
  CardHeader,
  EmptyState,
  GbtDateTimePipe,
  GbtToastService,
  Icon,
  ListRow,
  PageHeader,
  PageLayout,
  SegmentedControl,
  SegmentedControlOption,
  Skeleton,
  SkeletonList,
  UserChip,
  formatRelativeTime,
} from '@masmarino/gabarit';
import { SearchIssueResult, SearchMergeRequestResult, SearchRepositoryRef, SearchRepositoryResult, SearchResponse, SearchService, SearchUserResult } from '../search.service';
import { PageTitleService } from '../../shell/page-title.service';
import { issueKindPresentation } from '../../issues/issue-kind';
import { StatusBadge } from '../../shared/layout/status-badge/status-badge';

const RELATIVE_OPTIONS = { style: 'short', maxUnit: 'day', absoluteAfterDays: 30 } as const;

const EMPTY_RESULTS: SearchResponse = { repositories: [], issues: [], mergeRequests: [], users: [] };

/** The API returns at most this many results per category, so a full one reads "8+". */
const RESULTS_PER_CATEGORY = 8;

type CategoryKey = 'repositories' | 'issues' | 'mergeRequests' | 'users';
type Tab = 'all' | CategoryKey;

type SearchState = 'idle' | 'loading' | 'loaded' | 'failed';

interface Category {
  key: CategoryKey;
  label: string;
  icon: string;
}

const CATEGORIES: readonly Category[] = [
  { key: 'repositories', label: 'Dépôts', icon: 'folder-git-2' },
  { key: 'issues', label: 'Tickets', icon: 'circle-dot' },
  { key: 'mergeRequests', label: 'Demandes de fusion', icon: 'git-pull-request' },
  { key: 'users', label: 'Utilisateurs', icon: 'user' },
];

interface CategoryCard extends Category {
  headingId: string;
  counterValue: number;
  capped: boolean;
}

interface RepositoryRow {
  id: string;
  namespace: string;
  name: string;
  path: string;
  link: string[];
  description: string;
  visibilityLabel: string;
  visibilityIcon: string;
  visibilityVariant: 'info' | 'neutral';
}

interface RepositoryItemRow {
  id: string;
  title: string;
  link: string[];
  repositoryPath: string;
  repositoryLink: string[];
  createdAt: string;
  opened: string;
  status: string;
}

interface IssueRow extends RepositoryItemRow {
  number: string;
  kindIcon: string;
  kindLabel: string;
}

interface MergeRequestRow extends RepositoryItemRow {
  sourceBranch: string;
  targetBranch: string;
  branchesTitle: string;
}

function countLabel(length: number): string {
  return length >= RESULTS_PER_CATEGORY ? `${length}+` : String(length);
}

/** Gabarit gap: the counter only writes `N+` above its `max`, so a full category is passed as `max + 1`. */
function counterValue(length: number): number {
  return length >= RESULTS_PER_CATEGORY ? RESULTS_PER_CATEGORY + 1 : length;
}

function repositoryFields(repository: SearchRepositoryRef): Pick<RepositoryItemRow, 'repositoryPath' | 'repositoryLink'> {
  return { repositoryPath: repository.path.join('/'), repositoryLink: ['/repositories', ...repository.path] };
}

function openedWhen(createdAt: string): string {
  const when = formatRelativeTime(createdAt, 'fr', undefined, RELATIVE_OPTIONS);
  return /^\d/.test(when) ? `le ${when}` : when;
}

function repositoryRow(repository: SearchRepositoryResult): RepositoryRow {
  const isPublic = repository.visibility === 'public';
  return {
    id: repository.id,
    namespace: repository.path.slice(0, -1).map((segment) => `${segment}/`).join(''),
    name: repository.path.at(-1) ?? repository.name,
    path: repository.path.join('/'),
    link: ['/repositories', ...repository.path],
    description: repository.description,
    visibilityLabel: isPublic ? 'Public' : 'Privé',
    visibilityIcon: isPublic ? 'globe' : 'lock',
    visibilityVariant: isPublic ? 'info' : 'neutral',
  };
}

function issueRow(issue: SearchIssueResult): IssueRow {
  const kind = issueKindPresentation(issue.kind);
  return {
    id: issue.id,
    number: `#${issue.number}`,
    title: issue.title,
    link: ['/repositories', ...issue.repository.path, '-', 'issues', String(issue.number)],
    ...repositoryFields(issue.repository),
    createdAt: issue.createdAt,
    opened: openedWhen(issue.createdAt),
    status: issue.status,
    kindIcon: kind.icon,
    kindLabel: kind.label,
  };
}

function mergeRequestRow(mergeRequest: SearchMergeRequestResult): MergeRequestRow {
  return {
    id: mergeRequest.id,
    title: mergeRequest.title,
    link: ['/repositories', ...mergeRequest.repository.path, '-', 'merge-requests', mergeRequest.id],
    ...repositoryFields(mergeRequest.repository),
    createdAt: mergeRequest.createdAt,
    opened: openedWhen(mergeRequest.createdAt),
    status: mergeRequest.status,
    sourceBranch: mergeRequest.sourceBranch,
    targetBranch: mergeRequest.targetBranch,
    branchesTitle: `${mergeRequest.sourceBranch} → ${mergeRequest.targetBranch}`,
  };
}

/** French typography: non-breaking spaces inside the « guillemets », so "»" never wraps alone. */
function quoted(query: string): string {
  return `«\u00a0${query}\u00a0»`;
}

@Component({
  selector: 'fg-search-results',
  standalone: true,
  imports: [
    RouterLink,
    Badge,
    Button,
    Card,
    CardHeader,
    EmptyState,
    Icon,
    SegmentedControl,
    Skeleton,
    SkeletonList,
    PageLayout,
    PageHeader,
    ListRow,
    StatusBadge,
    UserChip,
    GbtDateTimePipe,
  ],
  templateUrl: './search-results.html',
  styleUrl: './search-results.scss',
})
export class SearchResults implements OnInit {
  private route = inject(ActivatedRoute);
  private search = inject(SearchService);
  private pageTitle = inject(PageTitleService);
  private destroyRef = inject(DestroyRef);
  private toast = inject(GbtToastService);

  private retries = new Subject<void>();

  protected query = signal('');
  protected results = signal<SearchResponse>(EMPTY_RESULTS);
  protected state = signal<SearchState>('idle');

  ngOnInit(): void {
    // switchMap drops a slower in-flight search so a stale response can't overwrite newer results.
    this.route.queryParamMap
      .pipe(
        map((params) => (params.get('q') ?? '').trim()),
        switchMap((q) => this.retries.pipe(startWith(undefined), map(() => q))),
        switchMap((q) => this.run(q)),
        takeUntilDestroyed(this.destroyRef),
      )
      .subscribe(({ state, results }) => {
        this.results.set(results);
        this.state.set(state);
      });
  }

  private run(q: string): Observable<{ state: SearchState; results: SearchResponse }> {
    this.query.set(q);
    if (!q) {
      return of({ state: 'idle', results: EMPTY_RESULTS });
    }
    // Clear now, or the old results show under the new query while it loads.
    this.results.set(EMPTY_RESULTS);
    this.state.set('loading');
    return this.search.search(q).pipe(
      map((results) => ({ state: 'loaded' as const, results })),
      catchError(() => {
        this.toast.show("Impossible d'effectuer la recherche. Réessayez plus tard.", 'error');
        return of({ state: 'failed' as const, results: EMPTY_RESULTS });
      }),
    );
  }

  protected retry(): void {
    this.retries.next();
  }

  protected title = computed(() => (this.query() ? `Résultats pour ${quoted(this.query())}` : 'Recherche'));

  private shellTitle = effect(() => this.pageTitle.set(this.title()));
  protected quotedQuery = computed(() => quoted(this.query()));

  private total = computed(() => {
    const r = this.results();
    return r.repositories.length + r.issues.length + r.mergeRequests.length + r.users.length;
  });
  private anyCapped = computed(() => CATEGORIES.some((category) => this.results()[category.key].length >= RESULTS_PER_CATEGORY));
  private totalLabel = computed(() => `${this.total()}${this.anyCapped() ? '+' : ''}`);

  protected hasResults = computed(() => this.state() === 'loaded' && this.total() > 0);
  protected isEmpty = computed(() => this.state() === 'loaded' && this.total() === 0);

  protected resultCountLabel = computed(() => `${this.totalLabel()} ${this.total() === 1 ? 'résultat' : 'résultats'}`);

  protected statusMessage = computed(() => {
    switch (this.state()) {
      case 'loading':
        return 'Recherche en cours…';
      case 'failed':
        return 'La recherche a échoué.';
      case 'loaded':
        return this.total() === 0 ? 'Aucun résultat.' : `${this.totalLabel()} ${this.total() === 1 ? 'résultat trouvé' : 'résultats trouvés'}.`;
      default:
        return '';
    }
  });

  protected tab = linkedSignal<SearchResponse, Tab>({ source: this.results, computation: () => 'all' });

  protected tabOptions = computed<SegmentedControlOption<Tab>[]>(() => {
    const r = this.results();
    return [
      { value: 'all', label: `Tout (${this.totalLabel()})` },
      ...CATEGORIES.map((category) => ({
        value: category.key,
        label: `${category.label} (${countLabel(r[category.key].length)})`,
        disabled: r[category.key].length === 0,
      })),
    ];
  });

  private cards = computed<CategoryCard[]>(() => {
    const r = this.results();
    return CATEGORIES.filter((category) => r[category.key].length > 0).map((category) => ({
      ...category,
      headingId: `search-results-${category.key}-heading`,
      counterValue: counterValue(r[category.key].length),
      capped: r[category.key].length >= RESULTS_PER_CATEGORY,
    }));
  });

  protected visibleCards = computed(() => {
    const tab = this.tab();
    return tab === 'all' ? this.cards() : this.cards().filter((card) => card.key === tab);
  });

  protected readonly resultsPerCategory = RESULTS_PER_CATEGORY;

  protected repositoryRows = computed(() => this.results().repositories.map(repositoryRow));
  protected issueRows = computed(() => this.results().issues.map(issueRow));
  protected mergeRequestRows = computed(() => this.results().mergeRequests.map(mergeRequestRow));
  protected userRows = computed<SearchUserResult[]>(() => this.results().users);
}
