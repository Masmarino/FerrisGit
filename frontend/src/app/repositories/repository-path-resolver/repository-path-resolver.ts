import { Component, DestroyRef, OnDestroy, OnInit, inject, signal } from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { catchError, map, of, switchMap } from 'rxjs';
import { ActivatedRoute, UrlSegment } from '@angular/router';
import { ResolvedPath, RepositoriesService } from '../repositories.service';
import { RepositoryContextService } from '../repository-context.service';
import { RepositoryDetail } from '../repository-detail/repository-detail';
import { GroupDetail } from '../../groups/group-detail/group-detail';
import { PipelineList } from '../../pipelines/pipeline-list/pipeline-list';
import { PipelineDetail } from '../../pipelines/pipeline-detail/pipeline-detail';
import { MergeRequestList } from '../../merge-requests/merge-request-list/merge-request-list';
import { MergeRequestDetail } from '../../merge-requests/merge-request-detail/merge-request-detail';
import { IssueList } from '../../issues/issue-list/issue-list';
import { IssueKanban } from '../../issues/issue-kanban/issue-kanban';
import { IssueDetail } from '../../issues/issue-detail/issue-detail';
import { RepositorySettings } from '../repository-settings/repository-settings';
import { ReleaseList } from '../../releases/release-list/release-list';
import { ReleaseDetail } from '../../releases/release-detail/release-detail';
import { WikiPageList } from '../../wiki/wiki-page-list/wiki-page-list';
import { WikiPageEditor } from '../../wiki/wiki-page-editor/wiki-page-editor';
import { WikiPageDetail } from '../../wiki/wiki-page-detail/wiki-page-detail';
import { RepositoryTreeView } from '../repository-tree-view/repository-tree-view';
import { RepositoryBlobView } from '../repository-blob-view/repository-blob-view';
import { EmptyState, Spinner } from '@masmarino/gabarit';

interface BreadcrumbAncestor {
  label: string;
  link: string[];
}

interface RepositoryViewBase {
  repositoryId: string;
  path: string[];
  ancestors: BreadcrumbAncestor[];
  groupId: string | null;
}

type View =
  | { kind: 'loading' }
  | { kind: 'notFound' }
  | { kind: 'group'; groupId: string; role: 'reader' | 'contributor' | 'maintainer' | null; path: string[] }
  | ({ kind: 'repository' } & RepositoryViewBase)
  | ({ kind: 'pipelines' } & RepositoryViewBase)
  | ({ kind: 'pipelineDetail'; id: string; jobId: string | null } & RepositoryViewBase)
  | ({ kind: 'mergeRequests' } & RepositoryViewBase)
  | ({ kind: 'mergeRequestDetail'; id: string } & RepositoryViewBase)
  | ({ kind: 'issues' } & RepositoryViewBase)
  | ({ kind: 'issueBoard' } & RepositoryViewBase)
  | ({ kind: 'issueDetail'; number: number } & RepositoryViewBase)
  | ({ kind: 'settings' } & RepositoryViewBase)
  | ({ kind: 'releases' } & RepositoryViewBase)
  | ({ kind: 'releaseDetail'; tagName: string } & RepositoryViewBase)
  | ({ kind: 'wikiPages' } & RepositoryViewBase)
  | ({ kind: 'wikiPageEditor'; slug: string | null } & RepositoryViewBase)
  | ({ kind: 'wikiPageDetail'; slug: string; showHistory: boolean } & RepositoryViewBase)
  | ({ kind: 'tree'; ref: string; treePath: string[] } & RepositoryViewBase)
  | ({ kind: 'blob'; ref: string; blobPath: string[] } & RepositoryViewBase);

@Component({
  selector: 'fg-repository-path-resolver',
  standalone: true,
  imports: [
    RepositoryDetail,
    GroupDetail,
    PipelineList,
    PipelineDetail,
    MergeRequestList,
    MergeRequestDetail,
    IssueList,
    IssueKanban,
    IssueDetail,
    RepositorySettings,
    ReleaseList,
    ReleaseDetail,
    WikiPageList,
    WikiPageEditor,
    WikiPageDetail,
    RepositoryTreeView,
    RepositoryBlobView,
    EmptyState,
    Spinner,
  ],
  templateUrl: './repository-path-resolver.html',
  styleUrl: './repository-path-resolver.scss',
})
export class RepositoryPathResolver implements OnInit, OnDestroy {
  private route = inject(ActivatedRoute);
  private repositories = inject(RepositoriesService);
  private destroyRef = inject(DestroyRef);
  private context = inject(RepositoryContextService);

  protected view = signal<View>({ kind: 'loading' });

  ngOnInit(): void {
    // `route.url` is a `BehaviorSubject` that replays its value synchronously, so this subscription covers the
    // initial load and every in-place navigation. Also resolving `route.snapshot.url` would fire a duplicate request.
    this.route.url
      .pipe(
        // `switchMap` cancels a stale `resolve()` so a slow earlier response can never overwrite a later view.
        switchMap((urlSegments) => {
          const { pathSegments, subPageSegments } = this.parseUrl(urlSegments);
          // Moving between a pipeline's summary and one of its jobs keeps the `fg-pipeline-detail` shell alive
          // (sidebar, polling): it reacts to a changing `jobId`, so skip the loading reset. Every other view reads
          // its inputs in `ngOnInit` and needs the reset to be recreated.
          const previous = this.view();
          if (
            previous.kind === 'pipelineDetail' &&
            subPageSegments[0] === 'pipelines' &&
            subPageSegments[1] === previous.id &&
            pathSegments.length === previous.path.length &&
            pathSegments.every((segment, index) => segment === previous.path[index])
          ) {
            return of<View>({
              kind: 'pipelineDetail',
              id: previous.id,
              jobId: this.pipelineJobId(subPageSegments),
              repositoryId: previous.repositoryId,
              path: previous.path,
              ancestors: previous.ancestors,
              groupId: previous.groupId,
            });
          }
          this.view.set({ kind: 'loading' });
          return this.repositories.resolve(pathSegments).pipe(
            map((resolved) => this.toView(resolved, subPageSegments, pathSegments)),
            catchError(() => {
              // The repository or group itself does not exist, so no sidebar is valid any more. Clear any previous context.
              this.context.leave();
              return of<View>({ kind: 'notFound' });
            }),
          );
        }),
        takeUntilDestroyed(this.destroyRef),
      )
      .subscribe((view) => {
        this.view.set(view);
        if (view.kind === 'group') {
          this.context.leave();
        } else if (view.kind !== 'loading' && view.kind !== 'notFound') {
          this.context.enter(view.repositoryId, view.path, view.ancestors, view.groupId);
        }
      });
  }

  ngOnDestroy(): void {
    this.context.leave();
  }

  // The `repositories/**` route also consumes the literal `repositories` prefix in `ActivatedRoute.url`:
  // drop it before `resolve()` and before the `path` passed to children.
  private parseUrl(urlSegments: UrlSegment[]): { pathSegments: string[]; subPageSegments: string[] } {
    const segments = urlSegments.map((s) => s.path).slice(1);
    const markerIndex = segments.indexOf('-');
    const pathSegments = markerIndex === -1 ? segments : segments.slice(0, markerIndex);
    const subPageSegments = markerIndex === -1 ? [] : segments.slice(markerIndex + 1);
    return { pathSegments, subPageSegments };
  }

  private pipelineJobId(subPage: string[]): string | null {
    return subPage[2] === 'jobs' && subPage[3] ? subPage[3] : null;
  }

  private toView(resolved: ResolvedPath, subPage: string[], path: string[]): View {
    if (resolved.type === 'group') {
      return { kind: 'group', groupId: resolved.groupId, role: resolved.role, path };
    }
    const repositoryId = resolved.repositoryId;
    const chain = resolved.type === 'groupRepository' ? resolved.chain : [];
    const ancestors: BreadcrumbAncestor[] = chain.map((entry, index) => ({
      label: entry.name,
      link: ['/repositories', ...chain.slice(0, index + 1).map((e) => e.name)],
    }));
    const groupId = chain.length > 0 ? chain[chain.length - 1].id : null;
    const base = { repositoryId, path, ancestors, groupId };
    if (subPage.length === 0) {
      return { kind: 'repository', ...base };
    }
    switch (subPage[0]) {
      case 'pipelines':
        if (!subPage[1]) {
          return { kind: 'pipelines', ...base };
        }
        return { kind: 'pipelineDetail', id: subPage[1], jobId: this.pipelineJobId(subPage), ...base };
      case 'merge-requests':
        return subPage[1]
          ? { kind: 'mergeRequestDetail', id: subPage[1], ...base }
          : { kind: 'mergeRequests', ...base };
      case 'issues':
        if (subPage[1] === 'board') {
          return { kind: 'issueBoard', ...base };
        }
        return subPage[1]
          ? { kind: 'issueDetail', number: Number(subPage[1]), ...base }
          : { kind: 'issues', ...base };
      case 'settings':
        return { kind: 'settings', ...base };
      case 'releases':
        return subPage[1] ? { kind: 'releaseDetail', tagName: subPage[1], ...base } : { kind: 'releases', ...base };
      case 'tree':
        return { kind: 'tree', ref: subPage[1] ?? 'HEAD', treePath: subPage.slice(2), ...base };
      case 'blob':
        return subPage[1] && subPage.length > 2
          ? { kind: 'blob', ref: subPage[1], blobPath: subPage.slice(2), ...base }
          : { kind: 'notFound' };
      case 'wiki': {
        if (subPage.length === 1) {
          return { kind: 'wikiPages', ...base };
        }
        if (subPage[1] === 'new') {
          return { kind: 'wikiPageEditor', slug: null, ...base };
        }
        const slug = subPage[1];
        if (subPage[2] === 'edit') {
          return { kind: 'wikiPageEditor', slug, ...base };
        }
        return { kind: 'wikiPageDetail', slug, showHistory: subPage[2] === 'history', ...base };
      }
      default:
        return { kind: 'notFound' };
    }
  }
}
