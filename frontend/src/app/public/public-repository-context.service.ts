import { Injectable, signal } from '@angular/core';
import { RepositoryContext, RepositoryContextService } from '../repositories/repository-context.service';

/** The repository context of the public pages: a visitor has no role, so nothing role-gated ever shows and no role is fetched. */
@Injectable()
export class PublicRepositoryContextService implements Pick<RepositoryContextService, 'current' | 'enter' | 'leave'> {
  readonly current = signal<RepositoryContext | null>(null);

  enter(repositoryId: string, path: string[], ancestors: { label: string; link: string[] }[], groupId: string | null): void {
    if (this.current()?.repositoryId !== repositoryId) {
      this.current.set({ repositoryId, path, role: null, ancestors, groupId });
    }
  }

  leave(): void {
    this.current.set(null);
  }
}
