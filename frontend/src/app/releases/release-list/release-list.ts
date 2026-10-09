import { Component, OnInit, computed, inject, input, linkedSignal, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { RouterLink } from '@angular/router';
import { Alert } from '@masmarino/gabarit/alert';
import { Badge } from '@masmarino/gabarit/badge';
import { Button } from '@masmarino/gabarit/button';
import { Card, CardHeader } from '@masmarino/gabarit/card';
import { EmptyState } from '@masmarino/gabarit/empty-state';
import { GbtDateTimePipe, GbtRelativeTimePipe } from '@masmarino/gabarit/format';
import { GbtInput } from '@masmarino/gabarit/input';
import { ListToolbarSortOption, createListToolbarState } from '@masmarino/gabarit/list-toolbar';
import { PageHeader } from '@masmarino/gabarit/page-header';
import { PageLayout } from '@masmarino/gabarit/page-layout';
import { Pagination } from '@masmarino/gabarit/pagination';
import { Panel } from '@masmarino/gabarit/panel';
import { SegmentedControl, SegmentedControlOption } from '@masmarino/gabarit/segmented-control';
import { Select, SelectOption } from '@masmarino/gabarit/select';
import { Skeleton } from '@masmarino/gabarit/skeleton';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { UserChip } from '@masmarino/gabarit/user-chip';
import { ReleaseStatus, ReleaseSummary, ReleasesService, releaseStatus } from '../releases.service';
import { plainExcerpt } from '../release-excerpt';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { PageTitleService } from '../../shell/page-title.service';
import { CreateReleaseModal } from '../create-release-modal/create-release-modal';
import { StatusBadge } from '../../shared/layout/status-badge/status-badge';
import { activeLocale, t, tn } from '../../shared/i18n/translator';
import { TranslocoPipe } from '@jsverse/transloco';

type SortKey = 'date' | 'title';

/** Client-side pagination: the API returns every release of the repository at once. */
export const RELEASES_PAGE_SIZE = 20;

const sortOptions = (): ListToolbarSortOption<SortKey>[] => [
  { value: 'date', label: t('common.date') },
  { value: 'title', label: t('common.title') },
];

const directionOptions = (): SegmentedControlOption<'asc' | 'desc'>[] => [
  { value: 'asc', label: t('common.ascending') },
  { value: 'desc', label: t('common.descending') },
];

interface ReleaseCard {
  release: ReleaseSummary;
  link: string[];
  status: ReleaseStatus;
  excerpt: string;
  dateVerb: string;
  date: string;
  assets: string | null;
}

function assetLabel(count: number): string | null {
  if (count <= 0) return null;
  return tn('common.fileCount', count);
}

@Component({
  selector: 'fg-release-list',
  standalone: true,
  imports: [TranslocoPipe, 
    FormsModule,
    RouterLink,
    GbtDateTimePipe,
    GbtRelativeTimePipe,
    PageLayout,
    PageHeader,
    Panel,
    StatusBadge,
    UserChip,
    Alert,
    Badge,
    Button,
    Card,
    CardHeader,
    EmptyState,
    GbtInput,
    Pagination,
    SegmentedControl,
    Select,
    Skeleton,
    CreateReleaseModal,
  ],
  templateUrl: './release-list.html',
  styleUrl: './release-list.scss',
})
export class ReleaseList implements OnInit {
  repositoryId = input.required<string>();
  path = input.required<string[]>();

  private releases = inject(ReleasesService);
  private repositoryContext = inject(RepositoryContextService);
  private pageTitle = inject(PageTitleService);
  private toast = inject(GbtToastService);

  protected items = signal<ReleaseSummary[]>([]);
  protected loading = signal(true);
  protected loadFailed = signal(false);
  protected role = computed(() => this.repositoryContext.current()?.role ?? null);
  // Public so the spec can call it.
  canManage = computed(() => this.role() === 'owner' || this.role() === 'maintainer');
  protected createModalOpen = signal(false);

  private readonly searchSort = createListToolbarState<SortKey>({ sortOptions: sortOptions(), defaultSort: 'date' });
  protected search = this.searchSort.search;
  protected sortValue = this.searchSort.sortValue;
  protected direction = this.searchSort.direction;
  protected readonly sortOptions = this.searchSort.sortOptions as SelectOption<SortKey>[];
  protected readonly directionOptions = directionOptions();
  // By the date each card shows: publication once published, creation for a draft.
  protected filteredItems = this.searchSort.filtered(() => this.items(), {
    text: (release) => [release.title, release.tagName],
    sortBy: { title: (release) => release.title, date: (release) => release.publishedAt ?? release.createdAt },
    locale: activeLocale(),
  });

  protected readonly pageSize = RELEASES_PAGE_SIZE;
  protected page = linkedSignal<unknown, number>({
    source: () => [this.search(), this.sortValue(), this.direction()],
    computation: () => 1,
  });
  /** The page actually shown: `page` clamped to the page count, since a refresh can shrink the list under it. */
  protected currentPage = computed(() => Math.min(this.page(), Math.max(1, Math.ceil(this.filteredItems().length / RELEASES_PAGE_SIZE))));
  protected cards = computed<ReleaseCard[]>(() => {
    const start = (this.currentPage() - 1) * RELEASES_PAGE_SIZE;
    return this.filteredItems()
      .slice(start, start + RELEASES_PAGE_SIZE)
      .map((release) => ({
        release,
        link: ['/repositories', ...this.path(), '-', 'releases', release.tagName],
        status: releaseStatus(release),
        excerpt: plainExcerpt(release.notesExcerpt),
        dateVerb: release.publishedAt ? t('releases.publishedVerb') : t('releases.createdVerb'),
        date: release.publishedAt ?? release.createdAt,
        assets: assetLabel(release.assetCount),
      }));
  });
  protected readonly pageLabel = (page: number) => t('common.pageNumber', { page });

  protected isEmptyRepository = computed(() => !this.loading() && !this.loadFailed() && this.items().length === 0);
  protected readonly skeletonCards = ['58%', '44%', '66%'];

  constructor() {
    // Newest first: the release a visitor looks for is almost always the latest.
    this.direction.set('desc');
  }

  ngOnInit(): void {
    this.pageTitle.set(t('nav.releases'));
    this.refresh();
  }

  refresh(): void {
    this.releases.list(this.repositoryId()).subscribe({
      next: (items) => {
        this.items.set(items);
        this.loading.set(false);
        this.loadFailed.set(false);
      },
      error: () => {
        this.loading.set(false);
        this.loadFailed.set(true);
        this.toast.show(t('releases.loadFailedToast'), 'error');
      },
    });
  }

  onCreated(): void {
    this.createModalOpen.set(false);
    this.refresh();
  }
}
