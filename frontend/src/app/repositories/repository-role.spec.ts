import { TestBed } from '@angular/core/testing';
import { RepositoryRole } from './repositories.service';
import { RepositoryContextService } from './repository-context.service';
import { canMaintain, canWrite, injectRepositoryPermissions } from './repository-role';

const ROLES: (RepositoryRole | null)[] = ['owner', 'maintainer', 'contributor', 'reader', null];

describe('repository roles', () => {
  it('lets the owner, Maintainers and Contributors write', () => {
    expect(ROLES.filter(canWrite)).toEqual(['owner', 'maintainer', 'contributor']);
  });

  it('lets only the owner and Maintainers maintain', () => {
    expect(ROLES.filter(canMaintain)).toEqual(['owner', 'maintainer']);
  });

  it('follows the role of the repository the user is on', () => {
    const context = TestBed.inject(RepositoryContextService);
    const permissions = TestBed.runInInjectionContext(() => injectRepositoryPermissions());
    const enter = (role: RepositoryRole | null) => context.current.set({ repositoryId: 'repo-1', path: ['acme', 'widget'], role, ancestors: [], groupId: null });

    expect([permissions.canWrite(), permissions.canMaintain()]).toEqual([false, false]);
    enter('contributor');
    expect([permissions.canWrite(), permissions.canMaintain()]).toEqual([true, false]);
    enter('maintainer');
    expect([permissions.canWrite(), permissions.canMaintain()]).toEqual([true, true]);
    enter(null);
    expect([permissions.canWrite(), permissions.canMaintain()]).toEqual([false, false]);
  });
});
