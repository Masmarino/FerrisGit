import { Component, DestroyRef, OnInit, WritableSignal, computed, inject, signal } from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { ActivatedRoute, Router, RouterLink } from '@angular/router';
import { Observable, forkJoin } from 'rxjs';
import { Button } from '@masmarino/gabarit/button';
import { Icon } from '@masmarino/gabarit/icon';
import { PageHeader } from '@masmarino/gabarit/page-header';
import { PageLayout } from '@masmarino/gabarit/page-layout';
import { Panel } from '@masmarino/gabarit/panel';
import { SegmentedControl, SegmentedControlOption } from '@masmarino/gabarit/segmented-control';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { GroupMembership, GroupsService } from '../../groups/groups.service';
import { Repository, RepositoriesService } from '../repositories.service';
import { CreateGroupModal } from '../../groups/create-group-modal/create-group-modal';
import { CreateRepositoryModal } from '../create-repository-modal/create-repository-modal';
import { PageTitleService } from '../../shell/page-title.service';
import { openWhenAsked } from '../../shared/open-when-asked';
import { WorkspaceGrid, WorkspaceGroupItem } from '../workspace-grid/workspace-grid';
import { WorkspaceGridFilters } from '../workspace-grid/workspace-grid-filters';

type TabId = 'all' | 'groups' | 'mine' | 'starred';

const TAB_TEXTS: Record<TabId, { label: string; searchLabel: string; emptyHeading: string; emptyMessage: string }> = {
  all: {
    label: 'Tous',
    searchLabel: 'Rechercher un dépôt ou un groupe',
    emptyHeading: "Aucun dépôt ni groupe pour l'instant",
    emptyMessage: 'Créez votre premier dépôt pour commencer.',
  },
  mine: { label: 'Mes dépôts', searchLabel: 'Rechercher un dépôt', emptyHeading: "Vous ne possédez aucun dépôt pour l'instant", emptyMessage: '' },
  starred: { label: 'Favoris', searchLabel: 'Rechercher un dépôt', emptyHeading: "Aucun favori pour l'instant", emptyMessage: 'Étoilez un dépôt pour le retrouver ici.' },
  groups: { label: 'Groupes', searchLabel: 'Rechercher un groupe', emptyHeading: "Vous n'appartenez à aucun groupe pour l'instant", emptyMessage: '' },
};

const TAB_ORDER: TabId[] = ['all', 'mine', 'starred', 'groups'];

const GROUP_SHORTCUTS = 6;

const LOAD_REPOSITORIES_ERROR = 'Impossible de charger les dépôts. Réessayez plus tard.';

const NO_GROUPS: WorkspaceGroupItem[] = [];
const NO_REPOSITORIES: Repository[] = [];

function toWorkspaceGroupItem(g: GroupMembership): WorkspaceGroupItem {
  return { id: g.id, name: g.path.split('/').at(-1) ?? g.path, link: ['/repositories', ...g.path.split('/')], path: g.path, role: g.role };
}

@Component({
  selector: 'fg-workspace-page',
  standalone: true,
  imports: [RouterLink, Button, Icon, SegmentedControl, PageHeader, PageLayout, Panel, CreateGroupModal, CreateRepositoryModal, WorkspaceGrid, WorkspaceGridFilters],
  templateUrl: './workspace-page.html',
  styleUrl: './workspace-page.scss',
})
export class WorkspacePage implements OnInit {
  private repositories = inject(RepositoriesService);
  private groups = inject(GroupsService);
  private pageTitle = inject(PageTitleService);
  private route = inject(ActivatedRoute);
  private router = inject(Router);
  private toast = inject(GbtToastService);
  private destroyRef = inject(DestroyRef);

  protected createModalOpen = signal(false);
  protected createGroupOpen = signal(false);

  constructor() {
    openWhenAsked('repository', () => this.createModalOpen.set(true));
    openWhenAsked('group', () => this.createGroupOpen.set(true));
  }
  protected activeTab = signal<TabId>('all');

  protected allRepos = signal<Repository[]>([]);
  protected allGroups = signal<WorkspaceGroupItem[]>([]);
  protected allLoading = signal(false);

  protected groupsList = signal<WorkspaceGroupItem[]>([]);
  protected groupsLoading = signal(false);

  protected mineRepos = signal<Repository[]>([]);
  protected mineLoading = signal(false);

  protected starredRepos = signal<Repository[]>([]);
  protected starredLoading = signal(false);

  private loaded = signal<ReadonlySet<TabId>>(new Set());
  private failedTabs = signal<ReadonlySet<TabId>>(new Set());
  private loadedTabs = new Set<TabId>();

  protected texts = computed(() => TAB_TEXTS[this.activeTab()]);
  protected activeGroups = computed(() => {
    switch (this.activeTab()) {
      case 'all':
        return this.allGroups();
      case 'groups':
        return this.groupsList();
      default:
        return NO_GROUPS;
    }
  });
  protected activeRepositories = computed(() => {
    switch (this.activeTab()) {
      case 'all':
        return this.allRepos();
      case 'mine':
        return this.mineRepos();
      case 'starred':
        return this.starredRepos();
      default:
        return NO_REPOSITORIES;
    }
  });
  protected activeLoading = computed(() => {
    switch (this.activeTab()) {
      case 'all':
        return this.allLoading();
      case 'groups':
        return this.groupsLoading();
      case 'mine':
        return this.mineLoading();
      case 'starred':
        return this.starredLoading();
    }
  });
  protected activeFailed = computed(() => !this.activeLoading() && this.failedTabs().has(this.activeTab()));

  /** Known from the tab's own data or from what "Tous" loaded. "Favoris" needs its own request. */
  private counts = computed<Record<TabId, number | null>>(() => {
    const loaded = this.loaded();
    const all = loaded.has('all');
    return {
      all: all ? this.allRepos().length + this.allGroups().length : null,
      mine: loaded.has('mine') ? this.mineRepos().length : all ? this.allRepos().filter((r) => r.role === 'owner').length : null,
      starred: loaded.has('starred') ? this.starredRepos().length : null,
      groups: loaded.has('groups') ? this.groupsList().length : all ? this.allGroups().length : null,
    };
  });
  protected tabOptions = computed<SegmentedControlOption<TabId>[]>(() => {
    const counts = this.counts();
    return TAB_ORDER.map((tab) => ({ value: tab, label: counts[tab] === null ? TAB_TEXTS[tab].label : `${TAB_TEXTS[tab].label} (${counts[tab]})` }));
  });

  protected showFilters = computed(() => !this.activeLoading() && !this.activeFailed() && this.activeGroups().length + this.activeRepositories().length > 0);
  private knownGroups = computed(() => (this.loaded().has('all') ? this.allGroups() : this.loaded().has('groups') ? this.groupsList() : NO_GROUPS));
  protected groupShortcuts = computed(() => [...this.knownGroups()].sort((a, b) => a.path.localeCompare(b.path)).slice(0, GROUP_SHORTCUTS));
  protected groupCount = computed(() => this.knownGroups().length);
  protected readonly groupsTabLink = ['/repositories'];
  protected readonly groupsTabQuery = { tab: 'groups' };

  ngOnInit(): void {
    this.pageTitle.set('Dépôts');
    this.route.queryParamMap.pipe(takeUntilDestroyed(this.destroyRef)).subscribe((params) => {
      const tab = params.get('tab');
      const resolved: TabId = (TAB_ORDER as string[]).includes(tab ?? '') ? (tab as TabId) : 'all';
      this.activeTab.set(resolved);
      this.ensureLoaded(resolved);
    });
  }

  protected selectTab(tab: TabId): void {
    this.router.navigate([], { relativeTo: this.route, queryParams: { tab: tab === 'all' ? null : tab }, queryParamsHandling: 'merge' });
  }

  protected reload(tab: TabId): void {
    this.loadedTabs.delete(tab);
    this.ensureLoaded(tab);
  }

  onRepoCreated(): void {
    this.createModalOpen.set(false);
    this.reloadLoaded();
  }

  onGroupCreated(): void {
    this.createGroupOpen.set(false);
    this.reloadLoaded();
  }

  /** A create or delete makes every loaded tab stale (a deleted repository would 404), so reload each once. */
  protected reloadLoaded(): void {
    const tabs = [...this.loadedTabs];
    this.loadedTabs.clear();
    tabs.forEach((tab) => this.ensureLoaded(tab));
  }

  private settle(tab: TabId, ok: boolean): void {
    this.failedTabs.update((current) => {
      const next = new Set(current);
      if (ok) {
        next.delete(tab);
      } else {
        next.add(tab);
      }
      return next;
    });
    if (ok) {
      this.loaded.update((current) => new Set(current).add(tab));
    }
  }

  private ensureLoaded(tab: TabId): void {
    if (this.loadedTabs.has(tab)) {
      return;
    }
    this.loadedTabs.add(tab);

    switch (tab) {
      case 'all':
        this.fetchTab(
          'all',
          this.allLoading,
          forkJoin({ repos: this.repositories.list(), groups: this.groups.listMember() }),
          ({ repos, groups }) => {
            this.allRepos.set(repos);
            this.allGroups.set(groups.map(toWorkspaceGroupItem));
          },
          LOAD_REPOSITORIES_ERROR,
        );
        break;
      case 'groups':
        this.fetchTab('groups', this.groupsLoading, this.groups.listMember(), (groups) => this.groupsList.set(groups.map(toWorkspaceGroupItem)), 'Impossible de charger les groupes. Réessayez plus tard.');
        break;
      case 'mine':
        this.fetchTab('mine', this.mineLoading, this.repositories.list(), (repos) => this.mineRepos.set(repos.filter((r) => r.role === 'owner')), LOAD_REPOSITORIES_ERROR);
        break;
      case 'starred':
        this.fetchTab('starred', this.starredLoading, this.repositories.list({ starred: true }), (repos) => this.starredRepos.set(repos), LOAD_REPOSITORIES_ERROR);
        break;
    }
  }

  private fetchTab<T>(tab: TabId, loading: WritableSignal<boolean>, request: Observable<T>, store: (data: T) => void, errorMessage: string): void {
    loading.set(true);
    request.subscribe({
      next: (data) => {
        store(data);
        loading.set(false);
        this.settle(tab, true);
      },
      error: () => {
        loading.set(false);
        this.settle(tab, false);
        this.toast.show(errorMessage, 'error');
      },
    });
  }
}
