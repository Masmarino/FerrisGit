import { ChangeDetectionStrategy, Component, computed, DestroyRef, inject, linkedSignal, OnInit } from '@angular/core';
import { takeUntilDestroyed, toSignal } from '@angular/core/rxjs-interop';
import { HttpErrorResponse } from '@angular/common/http';
import { ActivatedRoute, ParamMap, Router, RouterLink } from '@angular/router';
import { FormsModule } from '@angular/forms';
import { BehaviorSubject, catchError, combineLatest, debounceTime, distinctUntilChanged, map, Observable, of, startWith, Subject, switchMap } from 'rxjs';
import { Alert } from '@masmarino/gabarit/alert';
import { Button } from '@masmarino/gabarit/button';
import { Card } from '@masmarino/gabarit/card';
import { EmptyState } from '@masmarino/gabarit/empty-state';
import { GbtDateTimePipe, GbtRelativeTimePipe } from '@masmarino/gabarit/format';
import { Icon } from '@masmarino/gabarit/icon';
import { GbtInput } from '@masmarino/gabarit/input';
import { SegmentedControl, SegmentedControlOption } from '@masmarino/gabarit/segmented-control';
import { Skeleton } from '@masmarino/gabarit/skeleton';
import { AuthService } from '../../auth/auth.service';
import { PageTitleService } from '../../shell/page-title.service';
import { PublicConfigService } from '../public-config.service';
import { PUBLIC_CATALOG_MAX_QUERY_LENGTH as MAX_QUERY_LENGTH, PUBLIC_CATALOG_PAGE_SIZE, PublicCatalogPage, PublicCatalogQuery, PublicCatalogSort, PublicRepositoriesService, PublicRepositorySummary } from '../public-repositories.service';
import { loginLink } from '../../auth/login-link';
import { TranslocoPipe } from '@jsverse/transloco';
import { t, tn } from '../../shared/i18n/translator';

export const SEARCH_DEBOUNCE_MS = 300;
const SORTS: PublicCatalogSort[] = ['stars', 'name', 'created'];
const DEFAULT_SORT: PublicCatalogSort = 'stars';

const sortOptions = (): SegmentedControlOption<PublicCatalogSort>[] => [
  { value: 'stars', label: t('public.explore.sortStars') },
  { value: 'name', label: t('public.explore.sortName') },
  { value: 'created', label: t('public.explore.sortCreated') },
];

export type ExploreState =
  | { kind: 'loading' }
  | { kind: 'loaded'; result: PublicCatalogPage }
  | { kind: 'error' }
  | { kind: 'rateLimited' }
  | { kind: 'closed' };

interface ResultCard {
  repository: PublicRepositorySummary;
  link: string[];
  fullPath: string;
  starsLabel: string;
}

export function parseCatalogQuery(params: ParamMap): PublicCatalogQuery {
  const q = (params.get('q') ?? '').trim().slice(0, MAX_QUERY_LENGTH);
  const sortParam = params.get('sort') as PublicCatalogSort | null;
  const sort = sortParam && SORTS.includes(sortParam) ? sortParam : DEFAULT_SORT;
  const page = Number(params.get('page'));
  return { q, sort, page: Number.isInteger(page) && page >= 1 ? page : 1 };
}

const sameQuery = (a: PublicCatalogQuery, b: PublicCatalogQuery) => a.q === b.q && a.sort === b.sort && a.page === b.page;

function stateOf(error: unknown): ExploreState {
  if (error instanceof HttpErrorResponse) {
    if (error.status === 429) return { kind: 'rateLimited' };
    // Once public pages are closed, the public API answers 404 on everything.
    if (error.status === 404) return { kind: 'closed' };
  }
  return { kind: 'error' };
}

/** The catalog of public repositories. The URL (`q`, `sort`, `page`) is the state, so searches can be shared or bookmarked. */
@Component({
  selector: 'fg-explore-page',
  standalone: true,
  imports: [TranslocoPipe, FormsModule, RouterLink, Alert, Button, Card, EmptyState, GbtInput, Icon, SegmentedControl, Skeleton, GbtRelativeTimePipe, GbtDateTimePipe],
  templateUrl: './explore-page.html',
  styleUrl: './explore-page.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class ExplorePage implements OnInit {
  private route = inject(ActivatedRoute);
  private router = inject(Router);
  private catalog = inject(PublicRepositoriesService);
  private config = inject(PublicConfigService);
  private pageTitle = inject(PageTitleService);
  private destroyRef = inject(DestroyRef);
  protected isAuthenticated = inject(AuthService).isAuthenticated;

  protected readonly sortOptions = sortOptions();
  protected readonly maxQueryLength = MAX_QUERY_LENGTH;
  protected readonly skeletonCards = ['46%', '32%', '58%', '40%'];
  protected readonly login = loginLink('/explore');

  private query$ = this.route.queryParamMap.pipe(map(parseCatalogQuery), distinctUntilChanged(sameQuery));
  protected query = toSignal(this.query$, { requireSync: true });
  /** Follows the URL, and what's typed in between. */
  protected text = linkedSignal(() => this.query().q);

  private typed$ = new Subject<string>();
  private retry$ = new BehaviorSubject<void>(undefined);

  protected state = toSignal(
    combineLatest([this.config.publicPagesEnabled(), this.query$, this.retry$]).pipe(
      switchMap(([enabled, query]): Observable<ExploreState> =>
        enabled === false
          ? of({ kind: 'closed' })
          : this.catalog.search(query).pipe(
              map((result): ExploreState => ({ kind: 'loaded', result })),
              catchError((error: unknown) => of(stateOf(error))),
              startWith<ExploreState>({ kind: 'loading' }),
            ),
      ),
    ),
    { initialValue: { kind: 'loading' } as ExploreState },
  );

  protected result = computed(() => {
    const state = this.state();
    return state.kind === 'loaded' ? state.result : null;
  });
  protected cards = computed<ResultCard[]>(() =>
    (this.result()?.items ?? []).map((repository) => ({
      repository,
      link: ['/repositories', ...repository.path],
      fullPath: repository.path.join(' / '),
      starsLabel: tn('common.stars', repository.stars),
    })),
  );
  protected totalPages = computed(() => {
    const result = this.result();
    return result ? Math.max(1, Math.ceil(result.total / (result.perPage || PUBLIC_CATALOG_PAGE_SIZE))) : 1;
  });
  protected countLabel = computed(() => {
    const total = this.result()?.total;
    if (total === undefined) return '';
    return total === 0 ? t('public.explore.noRepository') : tn('common.repositoryCount', total);
  });
  protected heading = computed(() => {
    const q = this.query().q;
    if (q) return t('public.explore.resultsFor', { query: q });
    return this.query().sort === 'created' ? t('public.explore.recent') : this.query().sort === 'name' ? t('public.explore.all') : t('public.explore.popular');
  });
  protected pageLabel = computed(() => t('common.pageOf', { page: this.query().page, total: this.totalPages() }));

  constructor() {
    this.typed$.pipe(debounceTime(SEARCH_DEBOUNCE_MS), takeUntilDestroyed(this.destroyRef)).subscribe((text) => this.applySearch(text));
  }

  ngOnInit(): void {
    this.pageTitle.set(t('nav.explore'));
  }

  protected onTextChange(value: string): void {
    this.text.set(value);
    this.typed$.next(value);
  }

  /** Enter searches at once, without waiting for the debounce. */
  protected searchNow(): void {
    this.applySearch(this.text());
  }

  private applySearch(value: string): void {
    const q = value.trim().slice(0, MAX_QUERY_LENGTH);
    if (q === this.query().q) return;
    this.navigate({ q: q || null, page: null }, true);
  }

  protected setSort(sort: PublicCatalogSort): void {
    if (sort === this.query().sort) return;
    this.navigate({ sort: sort === DEFAULT_SORT ? null : sort, page: null });
  }

  protected clearSearch(): void {
    this.text.set('');
    this.navigate({ q: null, page: null });
  }

  protected retry(): void {
    this.retry$.next();
  }

  protected pageParams(page: number): { page: number | null } {
    return { page: page <= 1 ? null : page };
  }

  private navigate(queryParams: Record<string, string | number | null>, replaceUrl = false): void {
    void this.router.navigate([], { relativeTo: this.route, queryParams, queryParamsHandling: 'merge', replaceUrl });
  }
}
