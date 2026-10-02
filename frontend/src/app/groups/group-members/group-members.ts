import { Component, computed, inject, OnInit, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { ActivatedRoute, RouterLink } from '@angular/router';
import {
  Alert,
  Badge,
  Button,
  Card,
  ConfirmDangerModal,
  DescriptionList,
  DescriptionListEntry,
  EmptyState,
  GbtDateTimePipe,
  GbtInput,
  GbtRelativeTimePipe,
  GbtToastService,
  ListRow,
  PageHeader,
  PageLayout,
  Panel,
  Select,
  SelectOption,
  Skeleton,
  SkeletonList,
  UserChip,
} from '@masmarino/gabarit';
import { GROUP_ROLE_LABELS, GroupMember, GroupMembership, GroupRole, GroupsService } from '../groups.service';
import { PageTitleService } from '../../shell/page-title.service';

const ROLE_LEGEND: DescriptionListEntry[] = [
  { term: GROUP_ROLE_LABELS.reader, value: 'Consulte le groupe et ses dépôts.' },
  { term: GROUP_ROLE_LABELS.contributor, value: 'Pousse des branches, ouvre des tickets et des demandes de fusion.' },
  { term: GROUP_ROLE_LABELS.maintainer, value: 'Administre aussi le groupe : membres, sous-groupes et dépôts.' },
];

@Component({
  selector: 'fg-group-members',
  standalone: true,
  imports: [
    FormsModule,
    RouterLink,
    Alert,
    Badge,
    Button,
    DescriptionList,
    EmptyState,
    GbtInput,
    Select,
    Skeleton,
    SkeletonList,
    ConfirmDangerModal,
    ListRow,
    PageHeader,
    PageLayout,
    Panel,
    Card,
    UserChip,
    GbtDateTimePipe,
    GbtRelativeTimePipe,
  ],
  templateUrl: './group-members.html',
  styleUrl: './group-members.scss',
})
export class GroupMembers implements OnInit {
  private route = inject(ActivatedRoute);
  private groups = inject(GroupsService);
  private pageTitle = inject(PageTitleService);
  private toast = inject(GbtToastService);

  private groupId = '';

  protected members = signal<GroupMember[]>([]);
  protected listState = signal<'loading' | 'loaded' | 'failed'>('loading');
  protected membership = signal<GroupMembership | null>(null);
  /** The add card and row controls wait for 'known' so they don't pop in late. On 'failed' they show anyway, since the server enforces the role. */
  protected membershipState = signal<'loading' | 'known' | 'failed'>('loading');
  protected canManage = computed(() => this.membership()?.role === 'maintainer');
  protected showControls = computed(() => this.membershipState() === 'failed' || this.canManage());
  protected rolePending = computed(() => this.membershipState() === 'loading');
  protected backLink = computed(() => {
    const membership = this.membership();
    return membership ? ['/repositories', ...membership.path.split('/')] : ['/repositories'];
  });
  protected backQuery = computed(() => (this.membership() ? null : { tab: 'groups' }));

  protected newMemberUsername = signal('');
  protected newMemberRole = signal<GroupRole>('contributor');
  protected memberPendingRemoval = signal<GroupMember | null>(null);
  protected removing = signal(false);
  /** Dropping the entry changes the binding back, so `NgModel` writes the previous role into the select instead of leaving a refused one. */
  protected pendingRoles = signal<ReadonlyMap<string, GroupRole>>(new Map());
  protected readonly roleOptions: SelectOption<GroupRole>[] = Object.entries(GROUP_ROLE_LABELS).map(([value, label]) => ({ value: value as GroupRole, label }));

  ngOnInit(): void {
    this.groupId = this.route.snapshot.paramMap.get('id') ?? '';
    this.pageTitle.set('Membres du groupe');
    this.groups.listMember().subscribe({
      next: (memberships) => {
        this.membership.set(memberships.find((m) => m.id === this.groupId) ?? null);
        this.membershipState.set('known');
      },
      error: () => {
        this.membershipState.set('failed');
        this.toast.show("Impossible de vérifier votre rôle dans ce groupe. Les actions restent proposées, le serveur vérifie vos droits.", 'error');
      },
    });
    this.refresh();
  }

  protected readonly roleLegend = ROLE_LEGEND;

  protected roleLabel(role: GroupRole): string {
    return GROUP_ROLE_LABELS[role];
  }

  refresh(): void {
    this.groups.listMembers(this.groupId).subscribe({
      next: (m) => {
        this.members.set(m);
        this.listState.set('loaded');
      },
      error: () => {
        if (this.listState() !== 'loaded') {
          this.listState.set('failed');
        }
        this.toast.show('Impossible de charger les membres. Réessayez plus tard.', 'error');
      },
    });
  }

  protected retry(): void {
    this.listState.set('loading');
    this.refresh();
  }

  addMember(): void {
    const username = this.newMemberUsername().trim();
    if (!username) {
      return;
    }
    this.groups.addMember(this.groupId, username, this.newMemberRole()).subscribe({
      next: () => {
        this.newMemberUsername.set('');
        this.toast.show('Membre ajouté.');
        this.refresh();
      },
      error: () => this.toast.show("Impossible d'ajouter ce membre (nom d'utilisateur inconnu, ou vous n'êtes pas mainteneur de ce groupe).", 'error'),
    });
  }

  protected confirmRemoveMember(member: GroupMember): void {
    this.memberPendingRemoval.set(member);
  }

  removeMember(username: string): void {
    if (this.removing()) {
      return;
    }
    this.removing.set(true);
    this.groups.removeMember(this.groupId, username).subscribe({
      next: () => {
        this.removing.set(false);
        this.memberPendingRemoval.set(null);
        this.toast.show('Membre retiré.');
        this.refresh();
      },
      // The confirmation covers the page, so an error must close it first to be visible.
      error: () => {
        this.removing.set(false);
        this.memberPendingRemoval.set(null);
        this.toast.show("Impossible de retirer ce membre (vous n'êtes peut-être pas mainteneur de ce groupe).", 'error');
      },
    });
  }

  changeMemberRole(member: GroupMember, role: GroupRole): void {
    const { userId, username } = member;
    this.setPendingRole(userId, role);
    this.groups.setMemberRole(this.groupId, username, role).subscribe({
      next: () => {
        this.members.update((list) => list.map((m) => (m.userId === userId ? { ...m, role } : m)));
        this.setPendingRole(userId, null);
        this.toast.show('Rôle mis à jour.');
      },
      error: () => {
        this.setPendingRole(userId, null);
        this.toast.show("Impossible de modifier le rôle de ce membre (vous n'êtes peut-être pas mainteneur de ce groupe).", 'error');
      },
    });
  }

  private setPendingRole(userId: string, role: GroupRole | null): void {
    this.pendingRoles.update((current) => {
      const next = new Map(current);
      if (role === null) {
        next.delete(userId);
      } else {
        next.set(userId, role);
      }
      return next;
    });
  }
}
