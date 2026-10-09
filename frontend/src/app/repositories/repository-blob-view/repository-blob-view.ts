import { Component, computed, DestroyRef, ElementRef, inject, input, OnInit, signal, viewChild } from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { Router, RouterLink } from '@angular/router';
import { HttpErrorResponse } from '@angular/common/http';
import { catchError, forkJoin, map, of } from 'rxjs';
import { Alert } from '@masmarino/gabarit/alert';
import { Breadcrumb } from '@masmarino/gabarit/breadcrumb';
import { Button } from '@masmarino/gabarit/button';
import { Card, CardHeader } from '@masmarino/gabarit/card';
import { CopyButton, selectContents } from '@masmarino/gabarit/copy-button';
import { EmptyState } from '@masmarino/gabarit/empty-state';
import { formatBytes } from '@masmarino/gabarit/format';
import { Icon } from '@masmarino/gabarit/icon';
import { PageLayout } from '@masmarino/gabarit/page-layout';
import { Panel } from '@masmarino/gabarit/panel';
import { SegmentedControl, SegmentedControlOption } from '@masmarino/gabarit/segmented-control';
import { Skeleton } from '@masmarino/gabarit/skeleton';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { Tree, TreeNode } from '@masmarino/gabarit/tree';
import { RepositoriesService, Repository, TreeEntry } from '../repositories.service';
import { RepositoryHeader } from '../repository-header/repository-header';
import { pathBreadcrumb, repositoryLink, sortEntries } from '../repository-links';
import { MarkdownView } from '../../shared/markdown-view/markdown-view';
import { CodeView, lineCount } from '../../shared/code-view/code-view';
import { activeLocale } from '../../shared/i18n/translator';

type MarkdownMode = 'preview' | 'source';

interface NavigatorEntry {
  path: string[];
  isDir: boolean;
}

interface Navigator {
  items: TreeNode[];
  entries: ReadonlyMap<string, NavigatorEntry>;
}

/** Id suffix of a fake "Chargement…" child, so a folder shows a chevron before its content loads. */
const PLACEHOLDER_SUFFIX = '/\u0000chargement';

const numberFormat = () => new Intl.NumberFormat(activeLocale());

/** The navigator only needs names and kinds, so skip the per-entry last-commit lookup. */
const NAMES_ONLY = { lastCommit: false } as const;

/** A file plus a navigator built from its path's folders; other folders load when expanded. */
@Component({
  selector: 'fg-repository-blob-view',
  standalone: true,
  imports: [RouterLink, Alert, Breadcrumb, Button, Card, CardHeader, CopyButton, EmptyState, Icon, SegmentedControl, Skeleton, Tree, RepositoryHeader, MarkdownView, CodeView, PageLayout, Panel],
  templateUrl: './repository-blob-view.html',
  styleUrl: './repository-blob-view.scss',
})
export class RepositoryBlobView implements OnInit {
  repositoryId = input.required<string>();
  ref = input.required<string>();
  blobPath = input.required<string[]>();

  private repositories = inject(RepositoriesService);
  private router = inject(Router);
  private toast = inject(GbtToastService);
  private destroyRef = inject(DestroyRef);

  protected readonly skeletonLines = ['42%', '68%', '55%', '74%', '38%', '61%', '47%', '30%'];
  protected readonly navSkeletonLines = ['55%', '40%', '62%', '48%', '35%', '58%'];
  protected readonly markdownModes: SegmentedControlOption<MarkdownMode>[] = [
    { value: 'preview', label: 'Aperçu' },
    { value: 'source', label: 'Code source' },
  ];

  protected repo = signal<Repository | null>(null);
  protected content = signal<string | null>(null);
  protected isBinary = signal(false);
  protected size = signal(0);
  protected isLoaded = signal(false);
  protected isNotFound = signal(false);
  protected loadFailed = signal(false);
  protected markdownMode = signal<MarkdownMode>('preview');

  protected fileName = computed(() => this.blobPath()[this.blobPath().length - 1] ?? '');
  protected isMarkdown = computed(() => this.fileName().toLowerCase().endsWith('.md'));
  protected isTooLarge = computed(() => this.content() === null && !this.isBinary() && this.size() > 0);
  protected hasText = computed(() => !!this.content());
  protected showsSource = computed(() => this.hasText() && (!this.isMarkdown() || this.markdownMode() === 'source'));

  protected sizeLabel = computed(() => formatBytes(this.size(), activeLocale(), { binaryUnits: 'legacy' }));
  protected lineCountLabel = computed(() => {
    const content = this.content();
    if (content === null) return '';
    const count = lineCount(content);
    return `${numberFormat().format(count)} ${count > 1 ? 'lignes' : 'ligne'}`;
  });

  protected rootLink = computed(() => {
    const r = this.repo();
    return r ? ['/repositories', ...r.path] : null;
  });

  protected breadcrumbAncestors = computed(() => pathBreadcrumb(this.repo(), this.ref(), this.blobPath()));

  private codeBody = viewChild<ElementRef<HTMLElement>>('codeBody');

  /** Folder listings keyed by path joined with '/' ('' for the root). */
  private levels = signal<ReadonlyMap<string, TreeEntry[]>>(new Map());
  protected navigatorLoaded = signal(false);
  protected expandedIds = signal<string[]>([]);
  private pendingLevels = new Set<string>();
  protected navOpen = signal(false);

  protected currentFileId = computed(() => this.blobPath().join('/'));
  private navigator = computed(() => this.buildNavigator());
  protected navigatorItems = computed(() => this.navigator().items);

  ngOnInit(): void {
    this.repositories.getById(this.repositoryId()).subscribe({
      next: (repo) => this.repo.set(repo),
      error: () => this.toast.show('Impossible de charger ce dépôt. Réessayez plus tard.', 'error'),
    });

    this.repositories.blobAt(this.repositoryId(), this.ref(), this.blobPath()).subscribe({
      next: (blob) => {
        this.content.set(blob.content);
        this.isBinary.set(blob.isBinary);
        this.size.set(blob.size);
        this.isLoaded.set(true);
      },
      error: (err: HttpErrorResponse) => {
        if (err.status === 404) {
          this.tryRedirectToTree();
        } else {
          this.loadFailed.set(true);
          this.toast.show('Impossible de charger ce dépôt. Réessayez plus tard.', 'error');
        }
      },
    });

    this.loadNavigator();
  }

  private tryRedirectToTree(): void {
    this.repositories.treeAt(this.repositoryId(), this.ref(), this.blobPath(), NAMES_ONLY).subscribe({
      next: () => {
        this.router.navigate(repositoryLink(this.repo()?.path ?? [], 'tree', this.ref(), this.blobPath()));
      },
      error: () => this.isNotFound.set(true),
    });
  }

  /** A function, so a big file is only read when the button is pressed. */
  protected readonly contentToCopy = (): string => this.content() ?? '';

  /** Copy was refused (no clipboard outside secure contexts): select the code so it can be copied by hand. */
  protected selectCode(): void {
    selectContents(this.codeBody()?.nativeElement.querySelector<HTMLElement>('pre code'));
  }

  private loadNavigator(): void {
    const path = this.blobPath();
    const folders = Array.from({ length: path.length }, (_, depth) => path.slice(0, depth));
    this.expandedIds.set(folders.slice(1).map((folder) => folder.join('/')));
    forkJoin(
      folders.map((folder) =>
        this.repositories.treeAt(this.repositoryId(), this.ref(), folder, NAMES_ONLY).pipe(
          map((entries) => [folder.join('/'), entries] as const),
          catchError(() => of(null)),
        ),
      ),
    )
      .pipe(takeUntilDestroyed(this.destroyRef))
      .subscribe((results) => {
        this.levels.set(new Map(results.filter((result) => result !== null)));
        this.navigatorLoaded.set(true);
      });
  }

  private buildNavigator(): Navigator {
    const levels = this.levels();
    const expanded = new Set(this.expandedIds());
    const filePath = this.blobPath();
    const entries = new Map<string, NavigatorEntry>();

    const buildLevel = (folder: string[]): TreeNode[] => {
      const key = folder.join('/');
      let listing = levels.get(key);
      if (!listing) {
        if (!isOnPath(folder, filePath)) {
          return [{ id: `${key}${PLACEHOLDER_SUFFIX}`, label: 'Chargement…' }];
        }
        const next = filePath[folder.length];
        listing = [{ name: next, isDir: folder.length < filePath.length - 1, lastCommit: null }];
      }
      return sortEntries(listing).map((entry) => {
        const path = [...folder, entry.name];
        const id = path.join('/');
        entries.set(id, { path, isDir: entry.isDir });
        if (!entry.isDir) {
          return { id, label: entry.name, icon: 'file' };
        }
        return { id, label: entry.name, icon: expanded.has(id) ? 'folder-open' : 'folder', children: buildLevel(path) };
      });
    };

    return { items: buildLevel([]), entries };
  }

  protected onExpandedChange(ids: string[]): void {
    this.expandedIds.set(ids);
    for (const id of ids) {
      const entry = this.navigator().entries.get(id);
      if (entry?.isDir && !this.levels().has(id) && !isOnPath(entry.path, this.blobPath())) {
        this.loadLevel(id, entry.path);
      }
    }
  }

  private loadLevel(id: string, path: string[]): void {
    if (this.pendingLevels.has(id)) return;
    this.pendingLevels.add(id);
    this.repositories
      .treeAt(this.repositoryId(), this.ref(), path, NAMES_ONLY)
      .pipe(takeUntilDestroyed(this.destroyRef))
      .subscribe({
        next: (listing) => {
          this.pendingLevels.delete(id);
          this.levels.update((levels) => new Map(levels).set(id, listing));
        },
        error: () => {
          this.pendingLevels.delete(id);
          this.expandedIds.update((ids) => ids.filter((expandedId) => expandedId !== id));
        },
      });
  }

  protected openNode(id: string | null): void {
    const r = this.repo();
    const entry = id === null ? undefined : this.navigator().entries.get(id);
    if (!r || !entry || id === this.currentFileId()) return;
    void this.router.navigate(repositoryLink(r.path, entry.isDir ? 'tree' : 'blob', this.ref(), entry.path));
  }

  protected toggleNav(): void {
    this.navOpen.update((open) => !open);
  }
}

function isOnPath(folder: string[], filePath: string[]): boolean {
  return folder.length < filePath.length && folder.every((segment, i) => segment === filePath[i]);
}
