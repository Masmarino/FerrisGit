import { inject, Injectable, signal } from '@angular/core';
import { RepositoriesService, RepositoryRole } from './repositories.service';
import { PageTitleService } from '../shell/page-title.service';

export interface RepositoryAncestor {
  label: string;
  link: string[];
}

export interface RepositoryContext {
  repositoryId: string;
  path: string[];
  /** `null` until the role loads; templates treat that as no elevated permissions, so gated items never flash in. */
  role: RepositoryRole | null;
  /** Root-first, including the repository's own group; empty for a personal repository. */
  ancestors: RepositoryAncestor[];
  /** Direct parent group, used to fetch the breadcrumb switcher's siblings. */
  groupId: string | null;
}

@Injectable({ providedIn: 'root' })
export class RepositoryContextService {
  private repositories = inject(RepositoriesService);
  private pageTitle = inject(PageTitleService);

  readonly current = signal<RepositoryContext | null>(null);

  enter(repositoryId: string, path: string[], ancestors: RepositoryAncestor[], groupId: string | null): void {
    if (this.current()?.repositoryId === repositoryId) {
      return;
    }
    // Otherwise the previous repository's subpage title lingers in the breadcrumb.
    this.pageTitle.set('');
    this.current.set({ repositoryId, path, role: null, ancestors, groupId });
    this.repositories.getById(repositoryId).subscribe({
      next: (repo) => {
        if (this.current()?.repositoryId === repositoryId) {
          this.current.update((ctx) => (ctx ? { ...ctx, role: repo.role } : ctx));
        }
      },
      // No handler would make RxJS report an uncaught error. The role just stays null.
      error: () => {},
    });
  }

  leave(): void {
    this.current.set(null);
  }
}
