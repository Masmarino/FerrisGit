import { computed, inject } from '@angular/core';
import { RepositoryRole } from './repositories.service';
import { RepositoryContextService } from './repository-context.service';

/** Everyone but a Reader can write (comment, open issues). */
export function canWrite(role: RepositoryRole | null): boolean {
  return role === 'owner' || role === 'contributor' || role === 'maintainer';
}

/** Merging, deleting, managing releases: owner and Maintainers only. */
export function canMaintain(role: RepositoryRole | null): boolean {
  return role === 'owner' || role === 'maintainer';
}

/** Permissions on the current repository, both false until its role loads. Needs an injection context. */
export function injectRepositoryPermissions() {
  const context = inject(RepositoryContextService);
  const role = computed(() => context.current()?.role ?? null);
  return { canWrite: computed(() => canWrite(role())), canMaintain: computed(() => canMaintain(role())) };
}
