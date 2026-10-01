import { HttpErrorResponse } from '@angular/common/http';
import { Component, TemplateRef, computed, inject, input, OnInit, signal, viewChild } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { RouterLink } from '@angular/router';
import {
  Alert,
  Badge,
  Button,
  DescriptionList,
  DescriptionListEntry,
  GbtInput,
  GbtToastService,
  Icon,
  Modal,
  PageHeader,
  PageLayout,
  Panel,
  Skeleton,
  Textarea,
  UserChip,
} from '@masmarino/gabarit';
import { GroupsService, Group, GroupMember } from '../groups.service';
import { RepositoriesService, Repository } from '../../repositories/repositories.service';
import { PageTitleService } from '../../shell/page-title.service';
import { CreateRepositoryModal } from '../../repositories/create-repository-modal/create-repository-modal';
import { WorkspaceGrid, WorkspaceGroupItem } from '../../repositories/workspace-grid/workspace-grid';
import { WorkspaceGridFilters } from '../../repositories/workspace-grid/workspace-grid-filters';

type Role = 'reader' | 'contributor' | 'maintainer';

const ROLE_LABELS: Record<string, string> = { reader: 'Lecteur', contributor: 'Contributeur', maintainer: 'Mainteneur' };

const MEMBER_PREVIEW = 5;

function plural(count: number, one: string, many: string): string {
  return `${count} ${count > 1 ? many : one}`;
}

@Component({
  selector: 'fg-group-detail',
  standalone: true,
  imports: [
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
  protected roleLabel = computed(() => ROLE_LABELS[this.role() ?? ''] ?? null);

  private pathTemplate = viewChild.required<TemplateRef<unknown>>('pathTemplate');
  private roleTemplate = viewChild.required<TemplateRef<unknown>>('roleTemplate');
  protected facts = computed<DescriptionListEntry[]>(() => {
    const entries: DescriptionListEntry[] = [{ term: 'Chemin', value: this.pathTemplate() }];
    if (this.roleLabel()) entries.push({ term: 'Votre rôle', value: this.roleTemplate() });
    return entries;
  });
  protected membersLink = computed(() => ['/groups', this.groupId(), 'members']);

  // A maintainer of a group maintains its whole subtree: the subgroups' rows get the same role.
  protected workspaceGroups = computed<WorkspaceGroupItem[]>(() =>
    this.children().map((g) => ({
      id: g.id,
      name: g.name,
      link: this.childLink(g.name),
      path: [...this.path(), g.name].join('/'),
      role: this.isRole(this.role()) ? (this.role() as Role) : undefined,
      description: g.description,
      createdAt: g.createdAt,
    })),
  );
  protected summary = computed(() => `${plural(this.children().length, 'sous-groupe', 'sous-groupes')} · ${plural(this.repos().length, 'dépôt', 'dépôts')}`);
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
    return ROLE_LABELS[role] ?? role;
  }

  private isRole(role: string | null): boolean {
    return role === 'reader' || role === 'contributor' || role === 'maintainer';
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
        this.toast.show('Impossible de charger ce groupe.', 'error');
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
        this.toast.show('Impossible de charger ce groupe.', 'error');
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
        this.toast.show('Sous-groupe créé.');
        this.refresh();
      },
      error: (err: HttpErrorResponse) => {
        this.creatingGroup.set(false);
        if (err.status === 400) {
          this.createGroupError.set('Nom invalide : lettres, chiffres, - et _ uniquement');
        } else if (err.status === 409) {
          this.createGroupError.set('Ce nom est déjà utilisé');
        } else {
          this.createGroupError.set('Impossible de créer le sous-groupe. Réessayez plus tard.');
        }
      },
    });
  }

  onRepoCreated(): void {
    this.createRepoOpen.set(false);
    this.refresh();
  }
}
