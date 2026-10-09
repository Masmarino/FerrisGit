import { Component, computed, inject, OnInit, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { ActivatedRoute, RouterLink } from '@angular/router';
import { Alert } from '@masmarino/gabarit/alert';
import { Badge } from '@masmarino/gabarit/badge';
import { Button } from '@masmarino/gabarit/button';
import { Card } from '@masmarino/gabarit/card';
import { ConfirmDangerModal } from '@masmarino/gabarit/confirm-danger-modal';
import { DescriptionList, DescriptionListEntry } from '@masmarino/gabarit/description-list';
import { EmptyState } from '@masmarino/gabarit/empty-state';
import { GbtDateTimePipe, GbtRelativeTimePipe } from '@masmarino/gabarit/format';
import { GbtInput } from '@masmarino/gabarit/input';
import { ListRow } from '@masmarino/gabarit/list-row';
import { PageHeader } from '@masmarino/gabarit/page-header';
import { PageLayout } from '@masmarino/gabarit/page-layout';
import { Panel } from '@masmarino/gabarit/panel';
import { Select, SelectOption } from '@masmarino/gabarit/select';
import { Skeleton } from '@masmarino/gabarit/skeleton';
import { SkeletonList } from '@masmarino/gabarit/skeleton-list';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { UserChip } from '@masmarino/gabarit/user-chip';
import { GROUP_ROLE_LABELS, GroupMember, GroupMembership, GroupRole, GroupsService } from '../groups.service';
import { PageTitleService } from '../../shell/page-title.service';
import { TranslocoPipe } from '@jsverse/transloco';
import { t } from '../../shared/i18n/translator';

const roleLegend = (): DescriptionListEntry[] => [
  { term: GROUP_ROLE_LABELS.reader, value: t('groups.members.readerHelp') },
  { term: GROUP_ROLE_LABELS.contributor, value: t('groups.members.contributorHelp') },
  { term: GROUP_ROLE_LABELS.maintainer, value: t('groups.members.maintainerHelp') },
];

@Component({
  selector: 'fg-group-members',
  standalone: true,
  imports: [TranslocoPipe, 
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
  /** The add card and row controls wait for 'known' so they don't pop in late. On 'failed' they show anyway, the server enforces the role. */
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
  /** Dropping the entry flips the binding back, so `NgModel` rewrites the previous role into the select instead of leaving a refused one. */
  protected pendingRoles = signal<ReadonlyMap<string, GroupRole>>(new Map());
  protected readonly roleOptions: SelectOption<GroupRole>[] = Object.entries(GROUP_ROLE_LABELS).map(([value, label]) => ({ value: value as GroupRole, label }));

  ngOnInit(): void {
    this.groupId = this.route.snapshot.paramMap.get('id') ?? '';
    this.pageTitle.set(t('groups.members.title'));
    this.groups.listMember().subscribe({
      next: (memberships) => {
        this.membership.set(memberships.find((m) => m.id === this.groupId) ?? null);
        this.membershipState.set('known');
      },
      error: () => {
        this.membershipState.set('failed');
        this.toast.show(t('groups.members.roleCheckFailed'), 'error');
      },
    });
    this.refresh();
  }

  protected readonly roleLegend = roleLegend();

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
        this.toast.show(t('groups.members.loadFailed'), 'error');
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
        this.toast.show(t('groups.members.added'));
        this.refresh();
      },
      error: () => this.toast.show(t('groups.members.addFailed'), 'error'),
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
        this.toast.show(t('groups.members.removed'));
        this.refresh();
      },
      // The confirmation covers the page, so close it or the error stays hidden.
      error: () => {
        this.removing.set(false);
        this.memberPendingRemoval.set(null);
        this.toast.show(t('groups.members.removeFailed'), 'error');
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
        this.toast.show(t('groups.members.roleUpdated'));
      },
      error: () => {
        this.setPendingRole(userId, null);
        this.toast.show(t('groups.members.roleFailed'), 'error');
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
