import { afterNextRender, Component, computed, DOCUMENT, inject, Injector, input, OnInit, signal, TemplateRef, viewChild } from '@angular/core';
import { ActivatedRoute, RouterLink } from '@angular/router';
import { HttpErrorResponse } from '@angular/common/http';
import { Alert } from '@masmarino/gabarit/alert';
import { Badge } from '@masmarino/gabarit/badge';
import { Breadcrumb } from '@masmarino/gabarit/breadcrumb';
import { Button } from '@masmarino/gabarit/button';
import { Card, CardHeader } from '@masmarino/gabarit/card';
import { CopyField } from '@masmarino/gabarit/copy-field';
import { DescriptionList, DescriptionListEntry } from '@masmarino/gabarit/description-list';
import { EmptyState } from '@masmarino/gabarit/empty-state';
import { formatBytes, GbtDateTimePipe, GbtRelativeTimePipe } from '@masmarino/gabarit/format';
import { Icon } from '@masmarino/gabarit/icon';
import { PageLayout } from '@masmarino/gabarit/page-layout';
import { Panel } from '@masmarino/gabarit/panel';
import { Skeleton } from '@masmarino/gabarit/skeleton';
import { Spinner } from '@masmarino/gabarit/spinner';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { UserChip } from '@masmarino/gabarit/user-chip';
import { CommitInfo, RepositoriesService, Repository, StarResponse, TreeEntry } from '../repositories.service';
import { CLONE_PANEL_ID, RepositoryHeader, revealClonePanel } from '../repository-header/repository-header';
import { MarkdownView } from '../../shared/markdown-view/markdown-view';
import { ContributorAvatars } from '../contributor-avatars/contributor-avatars';
import { LanguageBar } from '../language-bar/language-bar';
import { commitTitle, shortSha } from '../commit-format';
import { pathBreadcrumb, repositoryLink, sortEntries } from '../repository-links';
import { activeLocale, t } from '../../shared/i18n/translator';
import { TranslocoPipe } from '@jsverse/transloco';

interface TreeRow {
  entry: TreeEntry;
  depth: number;
  parentPath: string[];
  /** Built once per row so the template binds a stable array. */
  link: string[];
}

const longDate = () => new Intl.DateTimeFormat(activeLocale(), { dateStyle: 'long' });

function without<T>(set: ReadonlySet<T>, value: T): Set<T> {
  const next = new Set(set);
  next.delete(value);
  return next;
}

@Component({
  selector: 'fg-repository-tree-view',
  standalone: true,
  imports: [TranslocoPipe, 
    RouterLink,
    Alert,
    Badge,
    Breadcrumb,
    Button,
    Card,
    CardHeader,
    DescriptionList,
    EmptyState,
    Icon,
    Skeleton,
    Spinner,
    RepositoryHeader,
    MarkdownView,
    ContributorAvatars,
    LanguageBar,
    PageLayout,
    Panel,
    CopyField,
    UserChip,
    GbtRelativeTimePipe,
    GbtDateTimePipe,
  ],
  templateUrl: './repository-tree-view.html',
  styleUrl: './repository-tree-view.scss',
})
export class RepositoryTreeView implements OnInit {
  repositoryId = input.required<string>();
  ref = input.required<string>();
  treePath = input.required<string[]>();

  private repositories = inject(RepositoriesService);
  private toast = inject(GbtToastService);
  private route = inject(ActivatedRoute);
  private document = inject(DOCUMENT);
  private injector = inject(Injector);

  protected readonly clonePanelId = CLONE_PANEL_ID;
  protected readonly skeletonRows = ['38%', '24%', '31%', '18%', '27%', '22%'];

  protected repo = signal<Repository | null>(null);
  protected entries = signal<TreeEntry[] | null>(null);
  protected readme = signal<string | null>(null);
  protected commits = signal<CommitInfo[]>([]);
  protected isEmptyRepository = signal(false);
  protected isNotFound = signal(false);
  protected loadFailed = signal(false);

  protected cloneUrl = computed(() => {
    const r = this.repo();
    return r ? this.repositories.cloneUrl(r.path) : '';
  });

  /** The clone URL cut after each `/`, so commands can wrap between segments on a phone. */
  protected cloneUrlSegments = computed(() => this.cloneUrl().split(/(?<=[^/:]\/)/));

  protected rootLink = computed(() => {
    const r = this.repo();
    return r ? ['/repositories', ...r.path] : null;
  });

  protected breadcrumbAncestors = computed(() => pathBreadcrumb(this.repo(), this.ref(), this.treePath()));

  protected currentFolderName = computed(() => this.treePath().at(-1) ?? '');

  /** At the root the ref's head, else the newest last-commit among the folder's entries (no extra request). */
  protected latestCommit = computed<CommitInfo | null>(() => {
    if (this.treePath().length === 0) {
      return this.commits()[0] ?? null;
    }
    let newest: CommitInfo | null = null;
    for (const entry of this.entries() ?? []) {
      const commit = entry.lastCommit;
      if (commit && (!newest || new Date(commit.committedAt) > new Date(newest.committedAt))) {
        newest = commit;
      }
    }
    return newest;
  });

  protected createdLabel = computed(() => {
    const createdAt = this.repo()?.createdAt;
    if (!createdAt) return '';
    const date = new Date(createdAt);
    return Number.isNaN(date.getTime()) ? createdAt : longDate().format(date);
  });

  protected sizeLabel = computed(() => {
    const bytes = this.repo()?.sizeBytes;
    return bytes === undefined || bytes === null ? '' : formatBytes(bytes, activeLocale(), { binaryUnits: 'legacy' });
  });

  private createdTemplate = viewChild.required<TemplateRef<unknown>>('createdTemplate');

  protected facts = computed<DescriptionListEntry[]>(() => {
    const r = this.repo();
    if (!r) return [];
    const entries: DescriptionListEntry[] = [
      { term: t('common.visibility'), value: r.visibility === 'public' ? t('common.public') : t('common.private') },
      { term: t('common.owner'), value: r.owner },
      { term: t('common.createdOnLabel'), value: this.createdTemplate() },
    ];
    if (this.sizeLabel()) entries.push({ term: t('common.size'), value: this.sizeLabel() });
    entries.push({ term: t('repositories.tabs.starred'), value: String(r.starCount ?? 0) });
    return entries;
  });

  protected commitTitle = commitTitle;

  protected shortSha = shortSha;

  // Directories expand in place. Both maps are keyed by the full path joined with '/'.
  private expandedPaths = signal<ReadonlySet<string>>(new Set());
  private childrenCache = signal<ReadonlyMap<string, TreeEntry[]>>(new Map());
  private loadingPaths = signal<ReadonlySet<string>>(new Set());

  protected visibleRows = computed(() => this.buildRows(this.entries() ?? [], this.treePath(), 0));

  private buildRows(entries: TreeEntry[], parentPath: string[], depth: number): TreeRow[] {
    const repoPath = this.repo()?.path ?? [];
    const rows: TreeRow[] = [];
    for (const entry of sortEntries(entries)) {
      const link = repositoryLink(repoPath, entry.isDir ? 'tree' : 'blob', this.ref(), [...parentPath, entry.name]);
      rows.push({ entry, depth, parentPath, link });
      if (entry.isDir) {
        const key = this.pathKey([...parentPath, entry.name]);
        const children = this.expandedPaths().has(key) ? this.childrenCache().get(key) : undefined;
        if (children) {
          rows.push(...this.buildRows(children, [...parentPath, entry.name], depth + 1));
        }
      }
    }
    return rows;
  }

  private pathKey(segments: string[]): string {
    return segments.join('/');
  }

  protected rowKey(row: TreeRow): string {
    return this.pathKey([...row.parentPath, row.entry.name]);
  }

  protected isExpanded(row: TreeRow): boolean {
    return this.expandedPaths().has(this.rowKey(row));
  }

  protected isLoading(row: TreeRow): boolean {
    return this.loadingPaths().has(this.rowKey(row));
  }

  protected toggleDir(row: TreeRow): void {
    const key = this.rowKey(row);
    if (this.expandedPaths().has(key)) {
      this.expandedPaths.update((paths) => without(paths, key));
      return;
    }

    this.expandedPaths.update((paths) => new Set(paths).add(key));
    if (this.childrenCache().has(key)) return;

    this.loadingPaths.update((paths) => new Set(paths).add(key));
    this.repositories.treeAt(this.repositoryId(), this.ref(), [...row.parentPath, row.entry.name]).subscribe({
      next: (children) => {
        this.childrenCache.update((cache) => new Map(cache).set(key, children));
        this.loadingPaths.update((paths) => without(paths, key));
      },
      error: () => {
        this.loadingPaths.update((paths) => without(paths, key));
        this.expandedPaths.update((paths) => without(paths, key));
      },
    });
  }

  protected onStarChange(state: StarResponse): void {
    this.repo.update((r) => (r ? { ...r, starCount: state.starCount, isStarred: state.isStarred } : r));
  }

  ngOnInit(): void {
    this.repositories.getById(this.repositoryId()).subscribe({
      next: (repo) => {
        this.repo.set(repo);
        // Came from the "Cloner" action on another page (#cloner): scroll to the clone box.
        if (this.route.snapshot.fragment === CLONE_PANEL_ID) {
          afterNextRender(() => revealClonePanel(this.document), { injector: this.injector });
        }
      },
      error: () => this.toast.show(t('repositories.loadOneFailed'), 'error'),
    });

    this.repositories.treeAt(this.repositoryId(), this.ref(), this.treePath()).subscribe({
      next: (entries) => this.entries.set(entries),
      error: (err: HttpErrorResponse) => {
        if (err.status !== 404) {
          this.loadFailed.set(true);
          this.toast.show(t('repositories.loadOneFailed'), 'error');
        } else if (this.ref() === 'HEAD' && this.treePath().length === 0) {
          this.isEmptyRepository.set(true);
        } else {
          this.isNotFound.set(true);
        }
      },
    });

    if (this.treePath().length === 0) {
      this.repositories.readmeAt(this.repositoryId(), this.ref()).subscribe({
        next: (readme) => this.readme.set(readme.content ?? null),
        error: () => {}, // Same as no README.
      });

      // Head commit for the banner; if it fails the banner is simply left out.
      this.repositories.commitsById(this.repositoryId(), this.ref()).subscribe({
        next: (commits) => this.commits.set(commits),
        error: () => {},
      });
    }
  }
}
