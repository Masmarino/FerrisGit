import { computed, inject } from '@angular/core';
import { RepositoryRole } from './repositories.service';
import { RepositoryContextService } from './repository-context.service';

/** The owner, Contributors and Maintainers may write (comment, open issues); a Reader only reads. */
export function canWrite(role: RepositoryRole | null): boolean {
  return role === 'owner' || role === 'contributor' || role === 'maintainer';
}

/** The owner and Maintainers administer the repository: merging, deleting, managing releases. */
export function canMaintain(role: RepositoryRole | null): boolean {
  return role === 'owner' || role === 'maintainer';
}

/** Permissions of the repository the user is currently on. Both are `false` until its role has loaded. Call in an injection context. */
export function injectRepositoryPermissions() {
  const context = inject(RepositoryContextService);
  const role = computed(() => context.current()?.role ?? null);
  return { canWrite: computed(() => canWrite(role())), canMaintain: computed(() => canMaintain(role())) };
}
