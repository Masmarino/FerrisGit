import { inject, Injectable, signal } from '@angular/core';
import { forkJoin } from 'rxjs';
import { GroupsService } from '../groups/groups.service';
import { RepositoriesService } from '../repositories/repositories.service';

export interface SwitcherGroup {
  id: string;
  name: string;
}

export interface SwitcherRepository {
  id: string;
  path: string[];
}

@Injectable({ providedIn: 'root' })
export class BreadcrumbSwitcherService {
  private groupsService = inject(GroupsService);
  private repositoriesService = inject(RepositoriesService);
  private loadedKey: string | null = null;

  readonly groups = signal<SwitcherGroup[]>([]);
  readonly repositories = signal<SwitcherRepository[]>([]);
  /** True from the moment a new target starts loading until its data arrives. The template then shows a placeholder item rather than an empty `gbt-menu`, whose popup would leave nothing to focus when opened by keyboard. */
  readonly loading = signal(false);

  loadForGroup(groupId: string): void {
    const key = `group:${groupId}`;
    if (this.loadedKey === key) {
      return;
    }
    this.loadedKey = key;
    // Cleared synchronously. This singleton lives across navigations, so the switcher must not show the previous target's siblings.
    this.groups.set([]);
    this.repositories.set([]);
    this.loading.set(true);
    forkJoin([this.groupsService.listChildren(groupId), this.repositoriesService.listForGroup(groupId)]).subscribe({
      next: ([children, repos]) => {
        if (this.loadedKey === key) {
          this.groups.set(children.map((g) => ({ id: g.id, name: g.name })));
          this.repositories.set(repos.map((r) => ({ id: r.id, path: r.path })));
          this.loading.set(false);
        }
      },
      error: () => {
        if (this.loadedKey === key) {
          this.loadedKey = null;
          this.loading.set(false);
        }
      },
    });
  }

  loadForOwner(ownerUsername: string): void {
    const key = `owner:${ownerUsername}`;
    if (this.loadedKey === key) {
      return;
    }
    this.loadedKey = key;
    this.groups.set([]);
    this.repositories.set([]);
    this.loading.set(true);
    this.repositoriesService.list().subscribe({
      next: (repos) => {
        if (this.loadedKey === key) {
          this.repositories.set(
            repos.filter((r) => r.path.length === 2 && r.path[0] === ownerUsername).map((r) => ({ id: r.id, path: r.path })),
          );
          this.loading.set(false);
        }
      },
      error: () => {
        if (this.loadedKey === key) {
          this.loadedKey = null;
          this.loading.set(false);
        }
      },
    });
  }
}
