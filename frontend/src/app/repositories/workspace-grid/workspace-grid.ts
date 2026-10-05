import { HttpErrorResponse } from '@angular/common/http';
import { Component, computed, inject, input, linkedSignal, output, signal } from '@angular/core';
import { RouterLink } from '@angular/router';
import { Alert } from '@masmarino/gabarit/alert';
import { Badge } from '@masmarino/gabarit/badge';
import { Button } from '@masmarino/gabarit/button';
import { ConfirmDangerModal } from '@masmarino/gabarit/confirm-danger-modal';
import { copyToClipboard } from '@masmarino/gabarit/copy-button';
import { EmptyState } from '@masmarino/gabarit/empty-state';
import { formatBytes, GbtDateTimePipe, GbtRelativeTimePipe } from '@masmarino/gabarit/format';
import { Icon } from '@masmarino/gabarit/icon';
import { ListCard } from '@masmarino/gabarit/list-card';
import { ListRow } from '@masmarino/gabarit/list-row';
import { createListToolbarState, ListToolbarSortOption } from '@masmarino/gabarit/list-toolbar';
import { Menu, MenuItem } from '@masmarino/gabarit/menu';
import { Pagination } from '@masmarino/gabarit/pagination';
import { SkeletonList } from '@masmarino/gabarit/skeleton-list';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { GroupsService } from '../../groups/groups.service';
import { MemberRole, Repository, RepositoriesService, RepositoryRole } from '../repositories.service';
import { canMaintain } from '../repository-role';

/** Paginated client-side: the API returns every group and repository at once. */
export const WORKSPACE_PAGE_SIZE = 25;

export interface WorkspaceGroupItem {
  id: string;
  name: string;
  link: string[];
  path: string;
  role?: MemberRole;
  /** Only known for subgroups (`GET /groups/{id}/children`), not for member groups. */
  description?: string;
  createdAt?: string;
}

export type WorkspaceSortKey = 'date' | 'name';

const SORT_OPTIONS: ListToolbarSortOption<WorkspaceSortKey>[] = [
  { value: 'date', label: 'Date de création' },
  { value: 'name', label: 'Nom' },
];

const ROLE_LABELS: Record<RepositoryRole, string> = {
  owner: 'Propriétaire',
  reader: 'Lecteur',
  contributor: 'Contributeur',
  maintainer: 'Mainteneur',
};

interface WorkspaceRow {
  key: string;
  kind: 'group' | 'repository';
  title: string;
  fullPath: string;
  link: string[];
  icon: string;
  kindLabel: string;
  visibility: { label: string; variant: 'info' | 'neutral' } | null;
  description: string;
  createdAt: string | null;
  size: string | null;
  stars: number | null;
  role: string | null;
  menuLabel: string;
  group: WorkspaceGroupItem | null;
  repository: Repository | null;
  hasMenu: boolean;
  canDelete: boolean;
}

/** `acme/backend/infra` under `acme/backend` reads `infra`. */
function relativePath(path: string, basePath: string): string {
  return basePath && path.startsWith(`${basePath}/`) ? path.slice(basePath.length + 1) : path;
}

function groupRow(group: WorkspaceGroupItem, basePath: string): WorkspaceRow {
  const canManage = group.role === 'maintainer';
  return {
    key: `group:${group.id}`,
    kind: 'group',
    title: relativePath(group.path, basePath),
    fullPath: group.path,
    link: group.link,
    icon: 'folder',
    kindLabel: 'Groupe',
    visibility: null,
    description: group.description ?? '',
    createdAt: group.createdAt ?? null,
    size: null,
    stars: null,
    role: group.role ? ROLE_LABELS[group.role] : null,
    menuLabel: `Actions du groupe ${group.path}`,
    group,
    repository: null,
    // Copy path and delete are maintainer actions.
    hasMenu: canManage,
    canDelete: canManage,
  };
}

function repositoryRow(repository: Repository, basePath: string): WorkspaceRow {
  const isPublic = repository.visibility === 'public';
  const path = repository.path.join('/');
  return {
    key: `repository:${repository.id}`,
    kind: 'repository',
    title: relativePath(path, basePath),
    fullPath: path,
    link: ['/repositories', ...repository.path],
    icon: isPublic ? 'globe' : 'lock',
    kindLabel: isPublic ? 'Dépôt public' : 'Dépôt privé',
    visibility: isPublic ? { label: 'Public', variant: 'info' } : { label: 'Privé', variant: 'neutral' },
    description: repository.description,
    createdAt: repository.createdAt,
    size: repository.sizeBytes === undefined ? null : formatBytes(repository.sizeBytes, 'fr', { binaryUnits: 'legacy' }),
    stars: repository.starCount ?? null,
    role: ROLE_LABELS[repository.role],
    menuLabel: `Actions du dépôt ${path}`,
    group: null,
    repository,
    hasMenu: true,
    canDelete: canMaintain(repository.role),
  };
}

/** List card of the repositories page and of a group page. Owns search and sort; the filters component shows them in the aside. */
@Component({
  selector: 'fg-workspace-grid',
  standalone: true,
  imports: [RouterLink, GbtDateTimePipe, GbtRelativeTimePipe, Alert, ListCard, ListRow, Badge, Button, ConfirmDangerModal, EmptyState, Icon, Menu, MenuItem, Pagination, SkeletonList],
  templateUrl: './workspace-grid.html',
  styleUrl: './workspace-grid.scss',
})
export class WorkspaceGrid {
  groups = input<WorkspaceGroupItem[]>([]);
  repositories = input<Repository[]>([]);
  loading = input(false);
  failed = input(false);
  searchLabel = input.required<string>();
  emptyHeading = input.required<string>();
  emptyMessage = input<string>('');
  /** Header heading on a group page; the repositories page projects tabs there instead. */
  heading = input<string | null>(null);
  /** What the list shows (a tab, a group). Changing it goes back to page 1, a plain refresh doesn't. */
  scope = input<string>('');
  /** The group whose page this is; item titles are shown relative to it. */
  basePath = input<string>('');

  changed = output<void>();
  retry = output<void>();

  private repositoriesService = inject(RepositoriesService);
  private groupsService = inject(GroupsService);
  private toast = inject(GbtToastService);

  // Newest first: you're usually looking for the repository you just created.
  private readonly searchSort = createListToolbarState<WorkspaceSortKey>({ sortOptions: SORT_OPTIONS, defaultSort: 'date', defaultDirection: 'desc' });
  readonly search = this.searchSort.search;
  readonly sortValue = this.searchSort.sortValue;
  readonly direction = this.searchSort.direction;
  readonly sortOptions = this.searchSort.sortOptions;

  protected isEmpty = computed(() => this.groups().length === 0 && this.repositories().length === 0);
  private filteredGroups = this.searchSort.filtered(() => this.groups(), {
    // Match on the title as shown, so searching "acme" on the acme page doesn't match every row.
    text: (group) => relativePath(group.path, this.basePath()),
    // compare instead of sortBy: a member group has no createdAt, and descending just reverses the
    // ascending result, so the path fallback has to apply the direction itself.
    compare: (a, b, key) => {
      if (key === 'name') {
        return a.path.localeCompare(b.path);
      }
      if (a.createdAt && b.createdAt) {
        return a.createdAt.localeCompare(b.createdAt);
      }
      return this.direction() === 'desc' ? b.path.localeCompare(a.path) : a.path.localeCompare(b.path);
    },
    locale: 'fr',
  });
  private filteredRepositories = this.searchSort.filtered(() => this.repositories(), {
    text: (repository) => relativePath(repository.path.join('/'), this.basePath()),
    sortBy: { date: (repository) => repository.createdAt, name: (repository) => repository.path.join('/') },
    locale: 'fr',
  });
  private allRows = computed<WorkspaceRow[]>(() => {
    const base = this.basePath();
    return [...this.filteredGroups().map((group) => groupRow(group, base)), ...this.filteredRepositories().map((repository) => repositoryRow(repository, base))];
  });
  protected allRowCount = computed(() => this.allRows().length);
  protected hasNoResults = computed(() => !this.isEmpty() && this.allRows().length === 0);

  protected readonly pageSize = WORKSPACE_PAGE_SIZE;
  protected page = linkedSignal<unknown, number>({
    source: () => [this.search(), this.sortValue(), this.direction(), this.scope()],
    computation: () => 1,
  });
  /** `page` clamped to the page count, since a refresh can shrink the list. */
  protected currentPage = computed(() => Math.min(this.page(), Math.max(1, Math.ceil(this.allRows().length / WORKSPACE_PAGE_SIZE))));
  protected rows = computed(() => {
    const start = (this.currentPage() - 1) * WORKSPACE_PAGE_SIZE;
    return this.allRows().slice(start, start + WORKSPACE_PAGE_SIZE);
  });
  protected readonly pageLabel = (page: number) => `Page ${page}`;

  protected deleteRepoTarget = signal<Repository | null>(null);
  protected deleteGroupTarget = signal<WorkspaceGroupItem | null>(null);

  protected starsWord(count: number): string {
    return count > 1 ? 'étoiles' : 'étoile';
  }

  protected copyPath(path: string | string[]): Promise<void> {
    return this.copyText(Array.isArray(path) ? path.join('/') : path, 'Chemin copié');
  }

  protected copyCloneUrl(repo: Repository): Promise<void> {
    return this.copyText(this.repositoriesService.cloneUrl(repo.path), 'URL de clonage copiée');
  }

  // A menu item has no button to show a status, hence the toast.
  private async copyText(text: string, done: string): Promise<void> {
    const copied = await copyToClipboard(text);
    if (copied) {
      this.toast.show(done);
    } else {
      this.toast.show('Copie impossible : le presse-papiers est indisponible.', 'error');
    }
  }

  // The menu gives focus back to its trigger on selection, so the dialog opens over the row's kebab
  // and returns focus there when it closes.
  protected confirmDeleteRepo(repo: Repository): void {
    this.deleteRepoTarget.set(repo);
  }

  protected confirmDeleteGroup(group: WorkspaceGroupItem): void {
    this.deleteGroupTarget.set(group);
  }

  protected cancelDelete(): void {
    this.deleteRepoTarget.set(null);
    this.deleteGroupTarget.set(null);
  }

  protected deleteRepoConfirmed(): void {
    const repo = this.deleteRepoTarget();
    if (!repo) {
      return;
    }
    this.repositoriesService.delete(repo.id).subscribe({
      next: () => {
        this.deleteRepoTarget.set(null);
        this.toast.show('Dépôt supprimé');
        this.changed.emit();
      },
      error: () => {
        this.deleteRepoTarget.set(null);
        this.toast.show('Impossible de supprimer le dépôt. Réessayez plus tard.', 'error');
      },
    });
  }

  protected deleteGroupConfirmed(): void {
    const group = this.deleteGroupTarget();
    if (!group) {
      return;
    }
    this.groupsService.delete(group.id).subscribe({
      next: () => {
        this.deleteGroupTarget.set(null);
        this.toast.show('Groupe supprimé');
        this.changed.emit();
      },
      error: (err: HttpErrorResponse) => {
        this.deleteGroupTarget.set(null);
        if (err.status === 409) {
          this.toast.show('Ce groupe contient encore des sous-groupes ou des dépôts. Videz-le avant de le supprimer.', 'error');
        } else {
          this.toast.show('Impossible de supprimer le groupe. Réessayez plus tard.', 'error');
        }
      },
    });
  }
}
