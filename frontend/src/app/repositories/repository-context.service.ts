import { inject, Injectable, signal } from '@angular/core';
import { RepositoriesService } from './repositories.service';
import { PageTitleService } from '../shell/page-title.service';

export interface RepositoryContext {
  repositoryId: string;
  path: string[];
  /** `null` until the role fetch resolves. Templates treat it as no elevated permissions, so a role-gated item does not flash into view. */
  role: 'owner' | 'reader' | 'contributor' | 'maintainer' | null;
  /** Groups above and including the repository's own group, root-first; empty for a personal repository. */
  ancestors: { label: string; link: string[] }[];
  /** The repository's own direct group id (`null` for a personal repository), to fetch the breadcrumb switcher's siblings. */
  groupId: string | null;
}

@Injectable({ providedIn: 'root' })
export class RepositoryContextService {
  private repositories = inject(RepositoriesService);
  private pageTitle = inject(PageTitleService);

  readonly current = signal<RepositoryContext | null>(null);

  enter(repositoryId: string, path: string[], ancestors: { label: string; link: string[] }[], groupId: string | null): void {
    if (this.current()?.repositoryId === repositoryId) {
      return;
    }
    // A stale subpage title must not show in the new repository's breadcrumb before its own page sets one.
    this.pageTitle.set('');
    this.current.set({ repositoryId, path, role: null, ancestors, groupId });
    this.repositories.getById(repositoryId).subscribe({
      next: (repo) => {
        if (this.current()?.repositoryId === repositoryId) {
          this.current.update((ctx) => (ctx ? { ...ctx, role: repo.role } : ctx));
        }
      },
      // Without an error callback, RxJS reports the failure as uncaught. `role` just stays `null`.
      error: () => {},
    });
  }

  leave(): void {
    this.current.set(null);
  }
}
