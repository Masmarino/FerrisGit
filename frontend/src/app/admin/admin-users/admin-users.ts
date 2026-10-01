import { afterNextRender, Component, computed, ElementRef, inject, Injector, OnInit, signal, viewChild } from '@angular/core';
import { Router, RouterLink } from '@angular/router';
import {
  Badge,
  Button,
  ConfirmDangerModal,
  createListToolbarState,
  GbtToastService,
  ListCard,
  ListCardState,
  ListRow,
  ListToolbar,
  ListToolbarSortOption,
  Menu,
  MenuItem,
  PageHeader,
  PageLayout,
  Panel,
  SegmentedControl,
  SegmentedControlOption,
  Spinner,
  UserChip,
} from '@masmarino/gabarit';
import { AdminUser, AdminUsersService } from '../admin-users.service';
import { AuthService } from '../../auth/auth.service';
import { MeService } from '../../shell/me.service';
import { PageTitleService } from '../../shell/page-title.service';
import { LinkMailFailed, MailedLinkKind } from './link-mail-failed/link-mail-failed';
import { InviteUserModal } from './invite-user-modal/invite-user-modal';
import {
  ACCOUNT_GONE,
  accountState,
  adminAction,
  apiMessage,
  DEMOTE_FAILED,
  DEMOTE_HEADING,
  demotedToast,
  demoteMessage,
  LAST_ADMIN_DEMOTE_REFUSED,
  mfaPresentation,
  mfaResetMessage,
  OWN_PASSWORD_REFUSED,
  passwordResetMessage,
  PENDING_ACTIVATION_REFUSED,
  plural,
  Presentation,
  PROMOTE_FAILED,
  promotedToast,
  RowDate,
  rowDate,
  SELF_DEMOTED_TOAST,
  SUPER_ADMIN,
} from './admin-user-presentation';

type UsersFilter = 'all' | 'pending';
type SortKey = 'createdAt' | 'username';
type LoadState = 'loading' | 'loaded' | 'failed';

const SORT_OPTIONS: ListToolbarSortOption<SortKey>[] = [
  { value: 'createdAt', label: 'Date de création' },
  { value: 'username', label: "Nom d'utilisateur" },
];

interface UserRow {
  user: AdminUser;
  isSelf: boolean;
  state: Presentation;
  mfa: Presentation | null;
  created: RowDate;
  expiry: RowDate | null;
  canResend: boolean;
  canReset: boolean;
  /** Not offered for the signed-in administrator's own account (the server refuses it). */
  canResetPassword: boolean;
  adminAction: { label: string; icon: string };
  menuLabel: string;
  /** Null for the signed-in administrator's own account. */
  link: string[] | null;
  busy: string | null;
}

interface MailFailure {
  kind: MailedLinkKind;
  username: string;
  url?: string;
  emailError?: string;
}

/**
 * Resetting the signed-in administrator's own two-factor authentication ends their session (the server
 * invalidates every session of the account), so the page warns beforehand and signs them out afterwards.
 * Their own password is never offered for a reset (the server refuses it).
 */
@Component({
  selector: 'fg-admin-users',
  standalone: true,
  imports: [
    RouterLink,
    PageHeader,
    PageLayout,
    Panel,
    ListRow,
    UserChip,
    ConfirmDangerModal,
    LinkMailFailed,
    InviteUserModal,
    Badge,
    Button,
    ListCard,
    ListToolbar,
    Menu,
    MenuItem,
    SegmentedControl,
    Spinner,
  ],
  templateUrl: './admin-users.html',
  styleUrl: './admin-users.scss',
})
export class AdminUsers implements OnInit {
  private users = inject(AdminUsersService);
  private pageTitle = inject(PageTitleService);
  private toast = inject(GbtToastService);
  private auth = inject(AuthService);
  private router = inject(Router);
  private me = inject(MeService);
  private host = inject<ElementRef<HTMLElement>>(ElementRef);
  private injector = inject(Injector);

  protected list = signal<AdminUser[]>([]);
  protected loadState = signal<LoadState>('loading');
  protected cardState = computed<ListCardState>(() =>
    this.loadState() === 'loading' ? 'loading' : this.loadState() === 'failed' ? 'failed' : this.list().length === 0 ? 'empty' : 'ready',
  );

  protected filter = signal<UsersFilter>('all');
  private pendingUsers = computed(() => this.list().filter((user) => user.state === 'invited'));
  private filtered = computed(() => (this.filter() === 'pending' ? this.pendingUsers() : this.list()));
  protected searchSort = createListToolbarState<SortKey>({ sortOptions: SORT_OPTIONS, defaultSort: 'createdAt' });
  protected search = this.searchSort.search;
  protected sortValue = this.searchSort.sortValue;
  protected direction = this.searchSort.direction;
  protected readonly sortOptions = SORT_OPTIONS;
  private filteredUsers = this.searchSort.filtered(() => this.filtered(), {
    text: (user) => [user.username, user.email],
    sortBy: { username: (user) => user.username, createdAt: (user) => user.createdAt },
    locale: 'fr',
  });

  /** Shown from the first paint so the layout does not shift when the list arrives. */
  protected toolbarShown = computed(() => this.loadState() !== 'loaded' || this.list().length > 0);
  protected toolbarActive = computed(() => this.loadState() === 'loaded');

  protected filterOptions = computed<SegmentedControlOption<UsersFilter>[]>(() => [
    { value: 'all', label: `Tous (${this.list().length})` },
    { value: 'pending', label: `Invitations en attente (${this.pendingUsers().length})` },
  ]);

  protected summary = computed(() => {
    const total = this.list().length;
    if (total === 0) {
      return null;
    }
    const pending = this.pendingUsers().length;
    const accounts = plural(total, 'compte', 'comptes');
    return pending > 0 ? `${accounts} · ${plural(pending, 'invitation', 'invitations')} en attente` : accounts;
  });

  private busyIds = signal<ReadonlyMap<string, string>>(new Map());

  protected rows = computed<UserRow[]>(() => {
    const now = new Date();
    const selfId = this.me.id();
    const busy = this.busyIds();
    return this.filteredUsers().map((user) => this.toRow(user, user.id === selfId, busy.get(user.id) ?? null, now));
  });

  private toRow(user: AdminUser, isSelf: boolean, busy: string | null, now: Date): UserRow {
    const invited = user.state === 'invited';
    const { state, expiry } = accountState(user, now);
    return {
      user,
      isSelf,
      state,
      mfa: mfaPresentation(user),
      created: rowDate(user.createdAt, now),
      expiry,
      canResend: invited,
      canReset: !invited && user.mfaEnabled,
      canResetPassword: !invited && !isSelf,
      adminAction: adminAction(user.isAdmin),
      menuLabel: `Actions pour ${user.username}`,
      link: isSelf ? null : ['/admin/users', user.id],
      busy,
    };
  }

  protected readonly superAdmin = SUPER_ADMIN;
  protected readonly demoteHeading = DEMOTE_HEADING;

  protected hasNoResults = computed(() => this.rows().length === 0);
  protected noResultsText = computed(() => {
    if (this.search().trim() !== '') {
      return this.filter() === 'pending' ? 'Aucune invitation en attente ne correspond à cette recherche' : 'Aucun utilisateur ne correspond à cette recherche';
    }
    return this.filter() === 'pending' ? 'Aucune invitation en attente' : 'Aucun utilisateur ne correspond à cette recherche';
  });

  protected inviteOpen = signal(false);
  private mailFailedAlert = viewChild(LinkMailFailed);
  protected mailFailure = signal<MailFailure | null>(null);
  protected resetTarget = signal<AdminUser | null>(null);
  protected resetting = signal(false);
  protected resetMessage = computed(() => {
    const target = this.resetTarget();
    if (!target) {
      return '';
    }
    return mfaResetMessage(target.username, target.id === this.me.id());
  });
  protected passwordResetTarget = signal<AdminUser | null>(null);
  protected resettingPassword = signal(false);
  protected passwordResetMessage = computed(() => {
    const target = this.passwordResetTarget();
    return target ? passwordResetMessage(target.username) : '';
  });
  protected demoteTarget = signal<AdminUser | null>(null);
  protected demoting = signal(false);
  protected demoteMessage = computed(() => {
    const target = this.demoteTarget();
    if (!target) {
      return '';
    }
    return demoteMessage(target.username, target.id === this.me.id());
  });

  constructor() {
    // Newest first: a fresh invitation is what an administrator comes back to.
    this.direction.set('desc');
  }

  ngOnInit(): void {
    this.pageTitle.set('Utilisateurs');
    this.refresh();
  }

  refresh(): void {
    this.users.list().subscribe({
      next: (list) => {
        this.list.set(list);
        this.loadState.set('loaded');
      },
      error: () => {
        // A failed reload keeps the list on screen and shows a toast. A failed first load shows the card's alert instead.
        if (this.loadState() !== 'loaded') {
          this.loadState.set('failed');
          return;
        }
        this.toast.show('Impossible de charger les utilisateurs. Réessayez plus tard.', 'error');
      },
    });
  }

  protected retryLoad(): void {
    this.loadState.set('loading');
    this.refresh();
  }

  protected openInvite(): void {
    this.inviteOpen.set(true);
  }

  protected closeInvite(): void {
    this.inviteOpen.set(false);
    // The dialog returns focus to its opener. If that was the empty state's button, which is gone by now, the header button gets it.
    afterNextRender(
      () => {
        const active = document.activeElement;
        if (!active || active === document.body) {
          this.host.nativeElement.querySelector<HTMLElement>('.admin-users__invite button')?.focus();
        }
      },
      { injector: this.injector },
    );
  }

  protected onInvited(): void {
    this.refresh();
  }

  resend(user: AdminUser): void {
    if (this.busyIds().has(user.id)) {
      return;
    }
    this.setBusy(user.id, 'Envoi en cours');
    this.users.resend(user.id).subscribe({
      next: (result) => {
        this.setBusy(user.id, null);
        this.list.update((list) => list.map((item) => (item.id === result.user.id ? result.user : item)));
        if (result.emailSent) {
          this.mailFailure.set(null);
          this.toast.show(`Invitation renvoyée à ${result.user.email}.`);
          this.focusRowAction(user.id);
        } else {
          this.showMailFailure({ kind: 'invitation', username: result.user.username, url: result.activationUrl, emailError: result.emailError });
        }
      },
      error: (err: { status?: number }) => {
        this.setBusy(user.id, null);
        this.focusRowAction(user.id);
        if (err.status === 400) {
          this.toast.show("Ce compte est déjà activé : il n'y a plus d'invitation à renvoyer.", 'error');
          this.refresh();
        } else {
          this.toast.show("L'invitation n'a pas pu être renvoyée. Réessayez plus tard.", 'error');
        }
      },
    });
  }

  protected dismissMailFailure(): void {
    this.mailFailure.set(null);
  }

  private showMailFailure(failure: MailFailure): void {
    this.mailFailure.set(failure);
    // The alert sits above the list, possibly far from the row, so it takes the focus and gets read out.
    afterNextRender(() => this.mailFailedAlert()?.focus(), { injector: this.injector });
  }

  /** Restores focus after render, since the menu closed or was replaced by the spinner and took the focus with it. */
  private focusRowAction(userId: string): void {
    afterNextRender(
      () => {
        const row = Array.from(this.host.nativeElement.querySelectorAll<HTMLElement>('li[data-user-id]')).find((item) => item.dataset['userId'] === userId);
        (row?.querySelector<HTMLElement>('button[aria-haspopup="menu"]') ?? row)?.focus();
      },
      { injector: this.injector },
    );
  }

  private setBusy(id: string, label: string | null): void {
    this.busyIds.update((ids) => {
      const next = new Map(ids);
      if (label) {
        next.set(id, label);
      } else {
        next.delete(id);
      }
      return next;
    });
  }

  askReset(user: AdminUser): void {
    this.resetTarget.set(user);
  }

  protected cancelReset(): void {
    const target = this.resetTarget();
    if (target && !this.resetting()) {
      this.resetTarget.set(null);
      this.focusRowAction(target.id);
    }
  }

  protected confirmReset(): void {
    const target = this.resetTarget();
    if (!target || this.resetting()) {
      return;
    }
    const self = target.id === this.me.id();
    this.resetting.set(true);
    this.users.resetMfa(target.id).subscribe({
      next: () => {
        this.resetting.set(false);
        this.resetTarget.set(null);
        if (self) {
          // The server ended every session of this account, this one included.
          this.auth.logout();
          void this.router.navigateByUrl('/login');
          return;
        }
        this.list.update((list) => list.map((item) => (item.id === target.id ? { ...item, mfaEnabled: false } : item)));
        this.toast.show('Double authentification réinitialisée.');
        this.focusRowAction(target.id);
      },
      error: () => {
        this.resetting.set(false);
        this.resetTarget.set(null);
        this.toast.show("La double authentification n'a pas pu être réinitialisée. Réessayez plus tard.", 'error');
        this.focusRowAction(target.id);
      },
    });
  }

  askPasswordReset(user: AdminUser): void {
    this.passwordResetTarget.set(user);
  }

  protected cancelPasswordReset(): void {
    const target = this.passwordResetTarget();
    if (target && !this.resettingPassword()) {
      this.passwordResetTarget.set(null);
      this.focusRowAction(target.id);
    }
  }

  protected confirmPasswordReset(): void {
    const target = this.passwordResetTarget();
    if (!target || this.resettingPassword()) {
      return;
    }
    this.resettingPassword.set(true);
    this.users.resetPassword(target.id).subscribe({
      next: (result) => {
        this.resettingPassword.set(false);
        this.passwordResetTarget.set(null);
        if (result.emailSent) {
          this.mailFailure.set(null);
          this.toast.show(`Mot de passe réinitialisé. Un lien pour en choisir un nouveau a été envoyé à ${target.email}.`);
          this.focusRowAction(target.id);
        } else {
          // The reset went through and the old password no longer works, so this link is the user's only way back in.
          this.showMailFailure({ kind: 'password-reset', username: target.username, url: result.resetUrl, emailError: result.emailError });
        }
      },
      error: (err: { status?: number; error?: unknown }) => {
        this.resettingPassword.set(false);
        this.passwordResetTarget.set(null);
        this.focusRowAction(target.id);
        if (err.status === 400 && apiMessage(err) === OWN_PASSWORD_REFUSED) {
          this.toast.show('Changez votre propre mot de passe depuis les paramètres de votre compte.', 'error');
        } else if (err.status === 400 && apiMessage(err) === PENDING_ACTIVATION_REFUSED) {
          this.toast.show("Ce compte n'est pas encore activé : renvoyez-lui plutôt l'invitation.", 'error');
          this.refresh();
        } else if (err.status === 400) {
          this.toast.show("Le mot de passe n'a pas pu être réinitialisé. Réessayez plus tard.", 'error');
        } else if (err.status === 404) {
          this.toast.show(ACCOUNT_GONE, 'error');
          this.refresh();
        } else {
          this.toast.show("Le mot de passe n'a pas pu être réinitialisé. Réessayez plus tard.", 'error');
        }
      },
    });
  }

  toggleAdmin(user: AdminUser): void {
    if (user.isAdmin) {
      this.demoteTarget.set(user);
    } else {
      this.promote(user);
    }
  }

  private promote(user: AdminUser): void {
    if (this.busyIds().has(user.id)) {
      return;
    }
    this.setBusy(user.id, 'Enregistrement en cours');
    this.users.setAdmin(user.id, true).subscribe({
      next: () => {
        this.setBusy(user.id, null);
        this.patchAdmin(user.id, true);
        this.toast.show(promotedToast(user.username));
        this.focusRowAction(user.id);
      },
      error: (err: { status?: number }) => {
        this.setBusy(user.id, null);
        this.focusRowAction(user.id);
        this.adminChangeFailed(err, PROMOTE_FAILED);
      },
    });
  }

  protected cancelDemote(): void {
    const target = this.demoteTarget();
    if (target && !this.demoting()) {
      this.demoteTarget.set(null);
      this.focusRowAction(target.id);
    }
  }

  protected confirmDemote(): void {
    const target = this.demoteTarget();
    if (!target || this.demoting()) {
      return;
    }
    const self = target.id === this.me.id();
    this.demoting.set(true);
    this.users.setAdmin(target.id, false).subscribe({
      next: () => {
        this.demoting.set(false);
        this.demoteTarget.set(null);
        if (self) {
          // Drop the shell's admin section immediately instead of waiting for the `/auth/me` refresh.
          this.me.isAdmin.set(false);
          this.me.load();
          this.toast.show(SELF_DEMOTED_TOAST);
          void this.router.navigateByUrl('/home');
          return;
        }
        this.patchAdmin(target.id, false);
        this.toast.show(demotedToast(target.username));
        this.focusRowAction(target.id);
      },
      error: (err: { status?: number }) => {
        this.demoting.set(false);
        this.demoteTarget.set(null);
        this.focusRowAction(target.id);
        this.adminChangeFailed(err, DEMOTE_FAILED);
      },
    });
  }

  private patchAdmin(id: string, isAdmin: boolean): void {
    this.list.update((list) => list.map((item) => (item.id === id ? { ...item, isAdmin } : item)));
  }

  private adminChangeFailed(err: { status?: number }, fallback: string): void {
    if (err.status === 409) {
      this.toast.show(LAST_ADMIN_DEMOTE_REFUSED, 'error');
    } else if (err.status === 404) {
      this.toast.show(ACCOUNT_GONE, 'error');
      this.refresh();
    } else {
      this.toast.show(fallback, 'error');
    }
  }
}
