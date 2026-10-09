import { afterNextRender, Component, computed, DestroyRef, ElementRef, inject, Injector, OnInit, signal, viewChild } from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { ActivatedRoute, Router, RouterLink } from '@angular/router';
import { Alert } from '@masmarino/gabarit/alert';
import { Badge } from '@masmarino/gabarit/badge';
import { Button } from '@masmarino/gabarit/button';
import { Card } from '@masmarino/gabarit/card';
import { ConfirmDangerModal } from '@masmarino/gabarit/confirm-danger-modal';
import { DescriptionList, DescriptionListEntry } from '@masmarino/gabarit/description-list';
import { EmptyState } from '@masmarino/gabarit/empty-state';
import { formatBytes, GbtDateTimePipe, GbtRelativeTimePipe } from '@masmarino/gabarit/format';
import { Icon } from '@masmarino/gabarit/icon';
import { ListCard, ListCardState } from '@masmarino/gabarit/list-card';
import { ListRow } from '@masmarino/gabarit/list-row';
import { Menu, MenuItem } from '@masmarino/gabarit/menu';
import { PageHeader } from '@masmarino/gabarit/page-header';
import { PageLayout } from '@masmarino/gabarit/page-layout';
import { Panel } from '@masmarino/gabarit/panel';
import { Skeleton } from '@masmarino/gabarit/skeleton';
import { Spinner } from '@masmarino/gabarit/spinner';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { AdminUser, AdminUserRepository, AdminUsersService } from '../../admin-users.service';
import { MeService } from '../../../shell/me.service';
import { PageTitleService } from '../../../shell/page-title.service';
import { LinkMailFailed, MailFailure } from '../link-mail-failed/link-mail-failed';
import { rowDate } from '../../row-date';
import {
  accountGone,
  accountState,
  adminAction,
  alreadyActive,
  apiMessage,
  demoteFailed,
  demoteHeading,
  demotedToast,
  demoteMessage,
  invitationResentToast,
  lastAdminDemoteRefused,
  LAST_ADMIN_REFUSED,
  lastMaintainerGroup,
  mfaResetDone,
  mfaResetFailed,
  mfaPresentation,
  mfaResetMessage,
  notActivated,
  passwordResetFailed,
  passwordResetMessage,
  passwordResetToast,
  PENDING_ACTIVATION_REFUSED,
  promoteFailed,
  promotedToast,
  resendFailed,
  SUPER_ADMIN,
} from '../admin-user-presentation';
import { activeLocale, t, tn } from '../../../shared/i18n/translator';
import { TranslocoPipe } from '@jsverse/transloco';

type LoadState = 'loading' | 'loaded' | 'failed';
type View = 'loading' | 'failed' | 'not-found' | 'self' | 'ready';

const USERS_LINK = '/admin/users';
const bytes = (value: number) => formatBytes(value, activeLocale(), { binaryUnits: 'legacy' });

/** Sum of the known sizes, so a lower bound when some couldn't be computed. */
export function totalSize(repositories: AdminUserRepository[]): { label: string; complete: boolean } | null {
  if (repositories.length === 0) {
    return null;
  }
  const known = repositories.filter((repository) => repository.sizeBytes !== null && repository.sizeBytes !== undefined);
  const sum = known.reduce((total, repository) => total + (repository.sizeBytes ?? 0), 0);
  if (known.length === 0) {
    return { label: t('admin.users.detail.unknownSize'), complete: false };
  }
  return known.length === repositories.length ? { label: bytes(sum), complete: true } : { label: t('admin.users.detail.atLeast', { size: bytes(sum) }), complete: false };
}

/** Everything the deletion destroys, listed before the administrator types the username. */
export function deletionMessage(username: string, repositories: AdminUserRepository[]): string {
  const size = totalSize(repositories);
  let personal: string;
  if (!size) {
    personal = t('admin.users.detail.noPersonal', { name: username });
  } else if (repositories.length === 1) {
    personal = t('admin.users.detail.onePersonal', { size: size.label });
  } else {
    const weight = size.label === t('admin.users.detail.unknownSize') ? size.label : t('admin.users.detail.inTotal', { size: size.label });
    personal = t('admin.users.detail.manyPersonal', { count: repositories.length, weight });
  }
  return [
    t('admin.users.detail.accountDeleted', { name: username }),
    personal,
    t('admin.users.detail.groupKept'),
    t('admin.users.detail.contributionsKept'),
    t('admin.users.detail.contributionsDeleted'),
    t('admin.users.detail.irreversible'),
  ].join('\n');
}

export function deletionRefusal(username: string, message: unknown): string | null {
  if (message === LAST_ADMIN_REFUSED) {
    return t('admin.users.detail.lastAdmin', { name: username });
  }
  const group = lastMaintainerGroup(message);
  if (group !== null) {
    return t('admin.users.detail.lastMaintainer', { name: username, group });
  }
  return null;
}

/**
 * No single-user endpoint, so the page reads the capped list (`GET /admin/users`) and picks the account from it.
 * Your own account isn't managed here.
 */
@Component({
  selector: 'fg-admin-user-detail',
  standalone: true,
  imports: [TranslocoPipe, 
    RouterLink,
    GbtDateTimePipe,
    GbtRelativeTimePipe,
    Alert,
    Badge,
    Button,
    Card,
    ConfirmDangerModal,
    DescriptionList,
    EmptyState,
    Icon,
    LinkMailFailed,
    ListCard,
    ListRow,
    Menu,
    MenuItem,
    PageHeader,
    PageLayout,
    Panel,
    Skeleton,
    Spinner,
  ],
  templateUrl: './admin-user-detail.html',
  styleUrl: './admin-user-detail.scss',
})
export class AdminUserDetail implements OnInit {
  private users = inject(AdminUsersService);
  private route = inject(ActivatedRoute);
  private router = inject(Router);
  private me = inject(MeService);
  private pageTitle = inject(PageTitleService);
  private toast = inject(GbtToastService);
  private destroyRef = inject(DestroyRef);
  private host = inject<ElementRef<HTMLElement>>(ElementRef);
  private injector = inject(Injector);

  protected userId = signal('');
  protected user = signal<AdminUser | null>(null);
  protected loadState = signal<LoadState>('loading');
  protected repositories = signal<AdminUserRepository[]>([]);
  protected repositoriesState = signal<LoadState>('loading');

  private meKnown = computed(() => this.me.id() !== '');
  protected view = computed<View>(() => {
    if (this.loadState() === 'loading') return 'loading';
    if (this.loadState() === 'failed') return 'failed';
    const user = this.user();
    if (!user) return 'not-found';
    return user.id === this.me.id() ? 'self' : 'ready';
  });

  protected readonly superAdmin = SUPER_ADMIN;
  protected readonly demoteHeading = demoteHeading();
  protected header = computed(() => {
    const user = this.user();
    if (!user) return null;
    const now = new Date();
    const { state, expiry } = accountState(user, now);
    return { state, expiry, mfa: mfaPresentation(user), created: rowDate(user.createdAt, now) };
  });
  protected actions = computed(() => {
    const user = this.user();
    if (!user || !this.meKnown()) return null;
    const invited = user.state === 'invited';
    return { canResend: invited, canResetPassword: !invited, canResetMfa: !invited && user.mfaEnabled, admin: adminAction(user.isAdmin) };
  });
  protected busy = signal<string | null>(null);

  protected repositoriesCard = computed<ListCardState>(() => {
    const state = this.repositoriesState();
    return state === 'loading' ? 'loading' : state === 'failed' ? 'failed' : this.repositories().length === 0 ? 'empty' : 'ready';
  });
  protected repositoryRows = computed(() =>
    this.repositories().map((repository) => {
      const isPublic = repository.visibility === 'public';
      return {
        repository,
        icon: isPublic ? 'globe' : 'lock',
        kindLabel: isPublic ? t('common.publicRepository') : t('common.privateRepository'),
        visibility: isPublic ? { label: t('common.public'), variant: 'info' as const } : { label: t('common.private'), variant: 'neutral' as const },
        size: repository.sizeBytes === null || repository.sizeBytes === undefined ? null : bytes(repository.sizeBytes),
      };
    }),
  );
  protected repositoriesSummary = computed(() => (this.repositoriesState() === 'loaded' ? tn('admin.users.repositories', this.repositories().length) : null));
  protected repositoryFacts = computed<DescriptionListEntry[]>(() => {
    if (this.repositoriesState() !== 'loaded') return [];
    const size = totalSize(this.repositories());
    return [
      { term: t('common.repositories'), value: String(this.repositories().length) },
      { term: t('common.totalSize'), value: size ? size.label : '—' },
    ];
  });

  private mailFailedAlert = viewChild(LinkMailFailed);
  protected mailFailure = signal<MailFailure | null>(null);
  protected deletionRefused = signal<string | null>(null);

  protected resetOpen = signal(false);
  protected resetting = signal(false);
  protected passwordResetOpen = signal(false);
  protected resettingPassword = signal(false);
  protected demoteOpen = signal(false);
  protected demoting = signal(false);
  protected deleteOpen = signal(false);
  protected deleting = signal(false);

  protected resetMessage = computed(() => mfaResetMessage(this.user()?.username ?? '', false));
  protected passwordResetMessage = computed(() => passwordResetMessage(this.user()?.username ?? ''));
  protected demoteMessage = computed(() => demoteMessage(this.user()?.username ?? '', false));
  protected deleteMessage = computed(() => deletionMessage(this.user()?.username ?? '', this.repositories()));
  protected canDelete = computed(() => this.meKnown() && this.repositoriesState() === 'loaded');

  protected readonly usersLink = USERS_LINK;
  protected readonly skeletonLines = ['70%', '55%', '62%'];

  ngOnInit(): void {
    this.pageTitle.set(t('admin.users.detail.title'));
    this.route.paramMap.pipe(takeUntilDestroyed(this.destroyRef)).subscribe((params) => {
      this.userId.set(params.get('id') ?? '');
      this.user.set(null);
      this.loadState.set('loading');
      this.mailFailure.set(null);
      this.deletionRefused.set(null);
      this.load();
      this.loadRepositories();
    });
  }

  private load(): void {
    const id = this.userId();
    this.users.list().subscribe({
      next: (list) => {
        const user = list.find((item) => item.id === id) ?? null;
        this.user.set(user);
        this.loadState.set('loaded');
        this.pageTitle.set(user ? user.username : t('admin.users.detail.notFound'));
      },
      error: () => this.loadState.set('failed'),
    });
  }

  private loadRepositories(): void {
    this.repositoriesState.set('loading');
    this.users.repositories(this.userId()).subscribe({
      next: (repositories) => {
        this.repositories.set(repositories);
        this.repositoriesState.set('loaded');
      },
      error: () => this.repositoriesState.set('failed'),
    });
  }

  protected retryLoad(): void {
    this.loadState.set('loading');
    this.load();
    if (this.repositoriesState() === 'failed') {
      this.loadRepositories();
    }
  }

  protected retryRepositories(): void {
    this.loadRepositories();
  }

  private accountGone(): void {
    this.toast.show(accountGone(), 'error');
    void this.router.navigateByUrl(USERS_LINK);
  }

  private patchUser(patch: Partial<AdminUser>): void {
    this.user.update((user) => (user ? { ...user, ...patch } : user));
  }

  private focusActions(): void {
    afterNextRender(() => this.host.nativeElement.querySelector<HTMLElement>('.admin-user-detail__menu button[aria-haspopup="menu"]')?.focus(), { injector: this.injector });
  }

  private showMailFailure(failure: MailFailure): void {
    this.mailFailure.set(failure);
    afterNextRender(() => this.mailFailedAlert()?.focus(), { injector: this.injector });
  }

  protected dismissMailFailure(): void {
    this.mailFailure.set(null);
  }

  protected dismissRefusal(): void {
    this.deletionRefused.set(null);
  }

  resend(): void {
    const user = this.user();
    if (!user || this.busy()) return;
    this.busy.set(t('common.sending'));
    this.users.resend(user.id).subscribe({
      next: (result) => {
        this.busy.set(null);
        this.user.set(result.user);
        if (result.emailSent) {
          this.mailFailure.set(null);
          this.toast.show(invitationResentToast(result.user.email));
          this.focusActions();
        } else {
          this.showMailFailure({ kind: 'invitation', username: result.user.username, url: result.activationUrl, emailError: result.emailError });
        }
      },
      error: (err: { status?: number }) => {
        this.busy.set(null);
        if (err.status === 404) {
          this.accountGone();
          return;
        }
        this.focusActions();
        if (err.status === 400) {
          this.toast.show(alreadyActive(), 'error');
          this.load();
        } else {
          this.toast.show(resendFailed(), 'error');
        }
      },
    });
  }

  askReset(): void {
    this.resetOpen.set(true);
  }

  protected cancelReset(): void {
    if (!this.resetting()) {
      this.resetOpen.set(false);
      this.focusActions();
    }
  }

  protected confirmReset(): void {
    const user = this.user();
    if (!user || this.resetting()) return;
    this.resetting.set(true);
    this.users.resetMfa(user.id).subscribe({
      next: () => {
        this.resetting.set(false);
        this.resetOpen.set(false);
        this.patchUser({ mfaEnabled: false });
        this.toast.show(mfaResetDone());
        this.focusActions();
      },
      error: (err: { status?: number }) => {
        this.resetting.set(false);
        this.resetOpen.set(false);
        if (err.status === 404) {
          this.accountGone();
          return;
        }
        this.toast.show(mfaResetFailed(), 'error');
        this.focusActions();
      },
    });
  }

  askPasswordReset(): void {
    this.passwordResetOpen.set(true);
  }

  protected cancelPasswordReset(): void {
    if (!this.resettingPassword()) {
      this.passwordResetOpen.set(false);
      this.focusActions();
    }
  }

  protected confirmPasswordReset(): void {
    const user = this.user();
    if (!user || this.resettingPassword()) return;
    this.resettingPassword.set(true);
    this.users.resetPassword(user.id).subscribe({
      next: (result) => {
        this.resettingPassword.set(false);
        this.passwordResetOpen.set(false);
        if (result.emailSent) {
          this.mailFailure.set(null);
          this.toast.show(passwordResetToast(user.email));
          this.focusActions();
        } else {
          // The old password no longer works, so this link is the user's only way back in.
          this.showMailFailure({ kind: 'password-reset', username: user.username, url: result.resetUrl, emailError: result.emailError });
        }
      },
      error: (err: { status?: number; error?: unknown }) => {
        this.resettingPassword.set(false);
        this.passwordResetOpen.set(false);
        if (err.status === 404) {
          this.accountGone();
          return;
        }
        this.focusActions();
        if (err.status === 400 && apiMessage(err) === PENDING_ACTIVATION_REFUSED) {
          this.toast.show(notActivated(), 'error');
          this.load();
        } else {
          this.toast.show(passwordResetFailed(), 'error');
        }
      },
    });
  }

  toggleAdmin(): void {
    const user = this.user();
    if (!user) return;
    if (user.isAdmin) {
      this.demoteOpen.set(true);
    } else {
      this.promote(user);
    }
  }

  private promote(user: AdminUser): void {
    if (this.busy()) return;
    this.busy.set(t('common.saving'));
    this.users.setAdmin(user.id, true).subscribe({
      next: () => {
        this.busy.set(null);
        this.patchUser({ isAdmin: true });
        this.toast.show(promotedToast(user.username));
        this.focusActions();
      },
      error: (err: { status?: number }) => {
        this.busy.set(null);
        this.adminChangeFailed(err, promoteFailed());
      },
    });
  }

  protected cancelDemote(): void {
    if (!this.demoting()) {
      this.demoteOpen.set(false);
      this.focusActions();
    }
  }

  protected confirmDemote(): void {
    const user = this.user();
    if (!user || this.demoting()) return;
    this.demoting.set(true);
    this.users.setAdmin(user.id, false).subscribe({
      next: () => {
        this.demoting.set(false);
        this.demoteOpen.set(false);
        this.patchUser({ isAdmin: false });
        this.toast.show(demotedToast(user.username));
        this.focusActions();
      },
      error: (err: { status?: number }) => {
        this.demoting.set(false);
        this.demoteOpen.set(false);
        this.adminChangeFailed(err, demoteFailed());
      },
    });
  }

  private adminChangeFailed(err: { status?: number }, fallback: string): void {
    if (err.status === 404) {
      this.accountGone();
      return;
    }
    this.focusActions();
    this.toast.show(err.status === 409 ? lastAdminDemoteRefused() : fallback, 'error');
  }

  askDelete(): void {
    if (!this.canDelete()) return;
    this.deletionRefused.set(null);
    this.deleteOpen.set(true);
  }

  protected cancelDelete(): void {
    if (!this.deleting()) {
      this.deleteOpen.set(false);
    }
  }

  protected confirmDelete(): void {
    const user = this.user();
    if (!user || this.deleting()) return;
    this.deleting.set(true);
    this.users.deleteUser(user.id).subscribe({
      next: () => {
        this.deleting.set(false);
        this.deleteOpen.set(false);
        this.toast.show(t('admin.users.detail.deleted', { name: user.username }));
        void this.router.navigateByUrl(USERS_LINK);
      },
      error: (err: { status?: number; error?: unknown }) => {
        this.deleting.set(false);
        this.deleteOpen.set(false);
        if (err.status === 404) {
          this.accountGone();
          return;
        }
        const refusal = err.status === 409 ? deletionRefusal(user.username, apiMessage(err)) : null;
        if (refusal) {
          // An expected refusal the administrator can act on: keep it on the page until dismissed, not in a passing toast.
          this.deletionRefused.set(refusal);
          afterNextRender(() => this.host.nativeElement.querySelector<HTMLElement>('.admin-user-detail__refusal')?.focus(), { injector: this.injector });
        } else if (err.status === 400) {
          this.toast.show(t('admin.users.detail.cannotDeleteSelf'), 'error');
        } else {
          this.toast.show(t('admin.users.detail.deleteFailed'), 'error');
        }
      },
    });
  }
}
