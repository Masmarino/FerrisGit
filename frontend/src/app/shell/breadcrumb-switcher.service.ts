import { inject, Injectable, signal } from '@angular/core';
import { forkJoin, Observable } from 'rxjs';
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
    this.load(
      `group:${groupId}`,
      forkJoin([this.groupsService.listChildren(groupId), this.repositoriesService.listForGroup(groupId)]),
      ([children, repos]) => ({
        groups: children.map((g) => ({ id: g.id, name: g.name })),
        repositories: repos.map((r) => ({ id: r.id, path: r.path })),
      }),
    );
  }

  loadForOwner(ownerUsername: string): void {
    this.load(`owner:${ownerUsername}`, this.repositoriesService.list(), (repos) => ({
      groups: [],
      repositories: repos.filter((r) => r.path.length === 2 && r.path[0] === ownerUsername).map((r) => ({ id: r.id, path: r.path })),
    }));
  }

  private load<T>(key: string, source: Observable<T>, toSiblings: (loaded: T) => { groups: SwitcherGroup[]; repositories: SwitcherRepository[] }): void {
    if (this.loadedKey === key) {
      return;
    }
    this.loadedKey = key;
    // Cleared synchronously. This singleton lives across navigations, so the switcher must not show the previous target's siblings.
    this.groups.set([]);
    this.repositories.set([]);
    this.loading.set(true);
    source.subscribe({
      next: (loaded) => {
        if (this.loadedKey === key) {
          const siblings = toSiblings(loaded);
          this.groups.set(siblings.groups);
          this.repositories.set(siblings.repositories);
          this.loading.set(false);
        }
      },
      error: () => {
        // A failed load is retried the next time the menu opens.
        if (this.loadedKey === key) {
          this.loadedKey = null;
          this.loading.set(false);
        }
      },
    });
  }
}
