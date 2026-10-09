import { HttpErrorResponse } from '@angular/common/http';
import { Component, TemplateRef, computed, inject, input, OnInit, signal, viewChild } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { RouterLink } from '@angular/router';
import { Alert } from '@masmarino/gabarit/alert';
import { Badge } from '@masmarino/gabarit/badge';
import { Button } from '@masmarino/gabarit/button';
import { DescriptionList, DescriptionListEntry } from '@masmarino/gabarit/description-list';
import { Icon } from '@masmarino/gabarit/icon';
import { GbtInput } from '@masmarino/gabarit/input';
import { Modal } from '@masmarino/gabarit/modal';
import { PageHeader } from '@masmarino/gabarit/page-header';
import { PageLayout } from '@masmarino/gabarit/page-layout';
import { Panel } from '@masmarino/gabarit/panel';
import { Skeleton } from '@masmarino/gabarit/skeleton';
import { Textarea } from '@masmarino/gabarit/textarea';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { UserChip } from '@masmarino/gabarit/user-chip';
import { Group, GROUP_ROLE_LABELS, groupCreationError, GroupMember, GroupsService, isGroupRole } from '../groups.service';
import { RepositoriesService, Repository } from '../../repositories/repositories.service';
import { PageTitleService } from '../../shell/page-title.service';
import { CreateRepositoryModal } from '../../repositories/create-repository-modal/create-repository-modal';
import { WorkspaceGrid, WorkspaceGroupItem } from '../../repositories/workspace-grid/workspace-grid';
import { WorkspaceGridFilters } from '../../repositories/workspace-grid/workspace-grid-filters';
import { TranslocoPipe } from '@jsverse/transloco';
import { t, tn } from '../../shared/i18n/translator';

const MEMBER_PREVIEW = 5;

@Component({
  selector: 'fg-group-detail',
  standalone: true,
  imports: [TranslocoPipe, 
    FormsModule,
    RouterLink,
    Alert,
    Badge,
    Button,
    DescriptionList,
    GbtInput,
    Icon,
    Modal,
    Skeleton,
    Textarea,
    PageHeader,
    PageLayout,
    Panel,
    UserChip,
    CreateRepositoryModal,
    WorkspaceGrid,
    WorkspaceGridFilters,
  ],
  templateUrl: './group-detail.html',
  styleUrl: './group-detail.scss',
})
export class GroupDetail implements OnInit {
  groupId = input.required<string>();
  role = input<string | null>(null);
  path = input.required<string[]>();

  private groups = inject(GroupsService);
  private repositories = inject(RepositoriesService);
  private pageTitle = inject(PageTitleService);
  private toast = inject(GbtToastService);

  protected children = signal<Group[]>([]);
  protected repos = signal<Repository[]>([]);
  protected loading = signal(true);
  protected loadFailed = signal(false);
  protected createRepoOpen = signal(false);
  private pendingRequests = 0;

  protected name = computed(() => this.path().at(-1) ?? '');
  protected fullPath = computed(() => this.path().join('/'));
  protected roleLabel = computed(() => {
    const role = this.role();
    return isGroupRole(role) ? GROUP_ROLE_LABELS[role] : null;
  });

  private pathTemplate = viewChild.required<TemplateRef<unknown>>('pathTemplate');
  private roleTemplate = viewChild.required<TemplateRef<unknown>>('roleTemplate');
  protected facts = computed<DescriptionListEntry[]>(() => {
    const entries: DescriptionListEntry[] = [{ term: t('common.path'), value: this.pathTemplate() }];
    if (this.roleLabel()) entries.push({ term: t('common.yourRole'), value: this.roleTemplate() });
    return entries;
  });
  protected membersLink = computed(() => ['/groups', this.groupId(), 'members']);

  // A group's maintainer maintains its whole subtree, so the subgroup rows get the same role.
  protected workspaceGroups = computed<WorkspaceGroupItem[]>(() => {
    const role = this.role();
    return this.children().map((g) => ({
      id: g.id,
      name: g.name,
      link: this.childLink(g.name),
      path: [...this.path(), g.name].join('/'),
      role: isGroupRole(role) ? role : undefined,
      description: g.description,
      createdAt: g.createdAt,
    }));
  });
  protected summary = computed(() => `${tn('groups.subgroups', this.children().length)} · ${tn('common.repositoryCount', this.repos().length)}`);
  protected showFilters = computed(() => !this.loading() && !this.loadFailed() && this.children().length + this.repos().length > 0);

  protected members = signal<GroupMember[]>([]);
  protected membersState = signal<'loading' | 'loaded' | 'failed'>('loading');
  protected memberPreview = computed(() => this.members().slice(0, MEMBER_PREVIEW));
  protected readonly memberSkeletons = ['60%', '45%', '52%'];

  protected createGroupOpen = signal(false);
  protected newGroupName = signal('');
  protected newGroupDescription = signal('');
  protected creatingGroup = signal(false);
  protected createGroupError = signal<string | null>(null);

  protected canWrite(): boolean {
    return this.role() === 'maintainer';
  }

  protected childLink(name: string): string[] {
    return ['/repositories', ...this.path(), name];
  }

  protected roleName(role: string): string {
    return isGroupRole(role) ? GROUP_ROLE_LABELS[role] : role;
  }

  ngOnInit(): void {
    this.pageTitle.set(this.name());
    this.refresh();
    if (this.role()) {
      this.loadMembers();
    }
  }

  protected refresh(): void {
    this.loading.set(true);
    this.loadFailed.set(false);
    this.pendingRequests = 2;
    const requestSettled = () => {
      this.pendingRequests--;
      if (this.pendingRequests === 0) {
        this.loading.set(false);
      }
    };
    this.groups.listChildren(this.groupId()).subscribe({
      next: (c) => {
        this.children.set(c);
        requestSettled();
      },
      error: () => {
        this.loadFailed.set(true);
        this.toast.show(t('groups.detail.loadFailed'), 'error');
        requestSettled();
      },
    });
    this.repositories.listForGroup(this.groupId()).subscribe({
      next: (r) => {
        this.repos.set(r);
        requestSettled();
      },
      error: () => {
        this.loadFailed.set(true);
        this.toast.show(t('groups.detail.loadFailed'), 'error');
        requestSettled();
      },
    });
  }

  private loadMembers(): void {
    this.groups.listMembers(this.groupId()).subscribe({
      next: (members) => {
        this.members.set(members);
        this.membersState.set('loaded');
      },
      error: () => this.membersState.set('failed'),
    });
  }

  protected openCreateGroup(): void {
    this.createGroupOpen.set(true);
  }

  protected closeCreateGroup(): void {
    this.createGroupOpen.set(false);
    this.newGroupName.set('');
    this.newGroupDescription.set('');
    this.createGroupError.set(null);
  }

  createSubgroup(): void {
    const name = this.newGroupName().trim();
    if (!name || this.creatingGroup()) {
      return;
    }
    this.creatingGroup.set(true);
    this.createGroupError.set(null);
    this.groups.createSubgroup(this.groupId(), name, this.newGroupDescription().trim()).subscribe({
      next: () => {
        this.creatingGroup.set(false);
        this.closeCreateGroup();
        this.toast.show(t('groups.detail.subgroupCreated'));
        this.refresh();
      },
      error: (err: HttpErrorResponse) => {
        this.creatingGroup.set(false);
        this.createGroupError.set(groupCreationError(err.status, 'subgroup'));
      },
    });
  }

  onRepoCreated(): void {
    this.createRepoOpen.set(false);
    this.refresh();
  }
}
