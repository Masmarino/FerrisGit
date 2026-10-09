import { ChangeDetectionStrategy, Component, computed, DestroyRef, inject, OnDestroy, OnInit, signal } from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { HttpErrorResponse } from '@angular/common/http';
import { ActivatedRoute, RouterLink, UrlSegment } from '@angular/router';
import { BehaviorSubject, catchError, combineLatest, map, of, switchMap } from 'rxjs';
import { Alert } from '@masmarino/gabarit/alert';
import { Breadcrumb } from '@masmarino/gabarit/breadcrumb';
import { Button } from '@masmarino/gabarit/button';
import { NavTab, NavTabs } from '@masmarino/gabarit/nav-tabs';
import { Spinner } from '@masmarino/gabarit/spinner';
import { RepositoriesService, ResolvedPath } from '../../repositories/repositories.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { RepositoryTreeView } from '../../repositories/repository-tree-view/repository-tree-view';
import { RepositoryBlobView } from '../../repositories/repository-blob-view/repository-blob-view';
import { RepositoryCommitList } from '../../repositories/repository-commit-list/repository-commit-list';
import { ReleaseList } from '../../releases/release-list/release-list';
import { ReleaseDetail } from '../../releases/release-detail/release-detail';
import { PageTitleService } from '../../shell/page-title.service';
import { PublicNotFound } from '../public-not-found/public-not-found';
import { TranslocoPipe } from '@jsverse/transloco';

export type PublicSection = 'overview' | 'commits' | 'releases';

interface RepositoryBase {
  repositoryId: string;
  path: string[];
}

export type PublicView =
  | { kind: 'loading' }
  | { kind: 'notFound' }
  | { kind: 'rateLimited' }
  | { kind: 'error' }
  | ({ kind: 'repository' } & RepositoryBase)
  | ({ kind: 'tree'; ref: string; treePath: string[] } & RepositoryBase)
  | ({ kind: 'blob'; ref: string; blobPath: string[] } & RepositoryBase)
  | ({ kind: 'commits'; ref: string } & RepositoryBase)
  | ({ kind: 'releases' } & RepositoryBase)
  | ({ kind: 'releaseDetail'; tagName: string } & RepositoryBase);

/** `repositories/<path…>/-/<sub-page…>`; the route takes the `repositories` prefix too. */
export function parsePublicUrl(urlSegments: UrlSegment[]): { pathSegments: string[]; subPage: string[] } {
  const segments = urlSegments.map((s) => s.path).slice(1);
  const marker = segments.indexOf('-');
  return marker === -1 ? { pathSegments: segments, subPage: [] } : { pathSegments: segments.slice(0, marker), subPage: segments.slice(marker + 1) };
}

export function sectionOf(subPage: string[]): PublicSection | null {
  switch (subPage[0]) {
    case undefined:
    case 'tree':
    case 'blob':
      return 'overview';
    case 'commits':
      return 'commits';
    case 'releases':
      return 'releases';
    default:
      return null;
  }
}

function toView(resolved: ResolvedPath, subPage: string[], path: string[]): PublicView {
  // Groups have no public page.
  if (resolved.type === 'group') {
    return { kind: 'notFound' };
  }
  const base = { repositoryId: resolved.repositoryId, path };
  switch (subPage[0]) {
    case undefined:
      return { kind: 'repository', ...base };
    case 'tree':
      return { kind: 'tree', ref: subPage[1] ?? 'HEAD', treePath: subPage.slice(2), ...base };
    case 'blob':
      return subPage[1] && subPage.length > 2 ? { kind: 'blob', ref: subPage[1], blobPath: subPage.slice(2), ...base } : { kind: 'notFound' };
    case 'commits':
      return { kind: 'commits', ref: subPage[1] ?? 'HEAD', ...base };
    case 'releases':
      return subPage[1] ? { kind: 'releaseDetail', tagName: subPage[1], ...base } : { kind: 'releases', ...base };
    default:
      return { kind: 'notFound' };
  }
}

function failedView(error: unknown): PublicView {
  if (error instanceof HttpErrorResponse && error.status === 429) return { kind: 'rateLimited' };
  if (error instanceof HttpErrorResponse && (error.status === 404 || error.status === 400)) return { kind: 'notFound' };
  return { kind: 'error' };
}

/**
 * The read-only pages of a public repository, at the same URLs as the signed-in ones. A small public cousin of
 * `RepositoryPathResolver`: only the readable sub-pages, with a tab bar instead of the shell's sidebar.
 */
@Component({
  selector: 'fg-public-repository-page',
  standalone: true,
  imports: [TranslocoPipe, RouterLink, Alert, Breadcrumb, Button, NavTab, NavTabs, Spinner, RepositoryTreeView, RepositoryBlobView, RepositoryCommitList, ReleaseList, ReleaseDetail, PublicNotFound],
  templateUrl: './public-repository-page.html',
  styleUrl: './public-repository-page.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class PublicRepositoryPage implements OnInit, OnDestroy {
  private route = inject(ActivatedRoute);
  private repositories = inject(RepositoriesService);
  private context = inject(RepositoryContextService);
  private destroyRef = inject(DestroyRef);
  protected pageTitle = inject(PageTitleService);

  protected readonly rootPath: string[] = [];
  protected view = signal<PublicView>({ kind: 'loading' });
  protected section = signal<PublicSection | null>(null);
  private retry$ = new BehaviorSubject<void>(undefined);

  /** Breadcrumb and tabs follow the repository context, so they stay put while a sub-page loads. */
  protected header = computed(() => {
    const ctx = this.context.current();
    if (!ctx) return null;
    const root = ['/repositories', ...ctx.path];
    return {
      owners: ctx.path.slice(0, -1),
      name: ctx.path[ctx.path.length - 1],
      overviewLink: root,
      commitsLink: [...root, '-', 'commits'],
      releasesLink: [...root, '-', 'releases'],
    };
  });

  ngOnInit(): void {
    combineLatest([this.route.url, this.retry$])
      .pipe(
        map(([segments]) => parsePublicUrl(segments)),
        switchMap(({ pathSegments, subPage }) => {
          this.section.set(sectionOf(subPage));
          this.view.set({ kind: 'loading' });
          this.pageTitle.set('');
          return this.repositories.resolve(pathSegments).pipe(
            map((resolved) => toView(resolved, subPage, pathSegments)),
            catchError((error: unknown) => of(failedView(error))),
          );
        }),
        takeUntilDestroyed(this.destroyRef),
      )
      .subscribe((view) => {
        if ('repositoryId' in view) {
          this.context.enter(view.repositoryId, view.path, [], null);
        } else {
          this.context.leave();
        }
        this.view.set(view);
      });
  }

  ngOnDestroy(): void {
    this.context.leave();
  }

  protected retry(): void {
    this.retry$.next();
  }
}
