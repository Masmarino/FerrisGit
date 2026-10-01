import { afterNextRender, Component, computed, DestroyRef, ElementRef, inject, Injector, OnInit, signal, viewChild } from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { ActivatedRoute, Router, RouterLink } from '@angular/router';
import {
  Alert,
  Badge,
  Button,
  Card,
  ConfirmDangerModal,
  DescriptionList,
  DescriptionListEntry,
  EmptyState,
  formatBytes,
  GbtDateTimePipe,
  GbtRelativeTimePipe,
  GbtToastService,
  Icon,
  ListCard,
  ListCardState,
  ListRow,
  Menu,
  MenuItem,
  PageHeader,
  PageLayout,
  Panel,
  Skeleton,
  Spinner,
} from '@masmarino/gabarit';
import { AdminUser, AdminUserRepository, AdminUsersService } from '../../admin-users.service';
import { MeService } from '../../../shell/me.service';
import { PageTitleService } from '../../../shell/page-title.service';
import { LinkMailFailed, MailedLinkKind } from '../link-mail-failed/link-mail-failed';
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
  LAST_ADMIN_REFUSED,
  lastMaintainerGroup,
  mfaPresentation,
  mfaResetMessage,
  passwordResetMessage,
  PENDING_ACTIVATION_REFUSED,
  plural,
  PROMOTE_FAILED,
  promotedToast,
  rowDate,
  SUPER_ADMIN,
} from '../admin-user-presentation';

type LoadState = 'loading' | 'loaded' | 'failed';
type View = 'loading' | 'failed' | 'not-found' | 'self' | 'ready';

interface MailFailure {
  kind: MailedLinkKind;
  username: string;
  url?: string;
  emailError?: string;
}

const USERS_LINK = '/admin/users';
const bytes = (value: number) => formatBytes(value, 'fr', { binaryUnits: 'legacy' });

/** Sum of the known sizes. It is a lower bound when some could not be computed. */
export function totalSize(repositories: AdminUserRepository[]): { label: string; complete: boolean } | null {
  if (repositories.length === 0) {
    return null;
  }
  const known = repositories.filter((repository) => repository.sizeBytes !== null && repository.sizeBytes !== undefined);
  const sum = known.reduce((total, repository) => total + (repository.sizeBytes ?? 0), 0);
  if (known.length === 0) {
    return { label: 'taille inconnue', complete: false };
  }
  return known.length === repositories.length ? { label: bytes(sum), complete: true } : { label: `au moins ${bytes(sum)}`, complete: false };
}

/** Lists everything the deletion destroys, before the administrator types the username. */
export function deletionMessage(username: string, repositories: AdminUserRepository[]): string {
  const size = totalSize(repositories);
  let personal: string;
  if (!size) {
    personal = `${username} ne possède aucun dépôt personnel.`;
  } else if (repositories.length === 1) {
    personal = `Son dépôt personnel (${size.label}) sera définitivement supprimé, fichiers sur le disque compris.`;
  } else {
    const weight = size.label === 'taille inconnue' ? size.label : `${size.label} au total`;
    personal = `Ses ${repositories.length} dépôts personnels (${weight}) seront définitivement supprimés, fichiers sur le disque compris.`;
  }
  return [
    `Le compte de ${username} sera définitivement supprimé.`,
    personal,
    "Les dépôts qu'il a créés dans un groupe seront conservés et vous seront réattribués.",
    'Ses demandes de fusion, ses commentaires sur les demandes de fusion et ses releases seront conservés et affichés comme « Utilisateur supprimé ».',
    "Les tickets et les commentaires de tickets qu'il a rédigés, ses revues et les pipelines qu'il a déclenchés seront supprimés.",
    'Cette action est irréversible.',
  ].join('\n');
}

export function deletionRefusal(username: string, message: unknown): string | null {
  if (message === LAST_ADMIN_REFUSED) {
    return `${username} est le dernier super-administrateur actif de l'instance : son compte ne peut pas être supprimé. Nommez d'abord un autre super-administrateur, puis réessayez.`;
  }
  const group = lastMaintainerGroup(message);
  if (group !== null) {
    return `${username} est le dernier mainteneur du groupe ${group} : sans lui, plus personne ne pourrait gérer ce groupe. Nommez d'abord un autre membre mainteneur de ce groupe, puis réessayez.`;
  }
  return null;
}

/**
 * There is no single-user endpoint, so the page reads the capped list (`GET /admin/users`) and picks the account from it.
 * The signed-in administrator's own account is not managed here.
 */
@Component({
  selector: 'fg-admin-user-detail',
  standalone: true,
  imports: [
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
  protected readonly demoteHeading = DEMOTE_HEADING;
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
        kindLabel: isPublic ? 'Dépôt public' : 'Dépôt privé',
        visibility: isPublic ? { label: 'Public', variant: 'info' as const } : { label: 'Privé', variant: 'neutral' as const },
        size: repository.sizeBytes === null || repository.sizeBytes === undefined ? null : bytes(repository.sizeBytes),
      };
    }),
  );
  protected repositoriesSummary = computed(() => (this.repositoriesState() === 'loaded' ? plural(this.repositories().length, 'dépôt', 'dépôts') : null));
  protected repositoryFacts = computed<DescriptionListEntry[]>(() => {
    if (this.repositoriesState() !== 'loaded') return [];
    const size = totalSize(this.repositories());
    return [
      { term: 'Dépôts', value: String(this.repositories().length) },
      { term: 'Taille totale', value: size ? size.label : '—' },
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
    this.pageTitle.set('Utilisateur');
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
        this.pageTitle.set(user ? user.username : 'Utilisateur introuvable');
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
    this.toast.show(ACCOUNT_GONE, 'error');
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
    this.busy.set('Envoi en cours');
    this.users.resend(user.id).subscribe({
      next: (result) => {
        this.busy.set(null);
        this.user.set(result.user);
        if (result.emailSent) {
          this.mailFailure.set(null);
          this.toast.show(`Invitation renvoyée à ${result.user.email}.`);
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
          this.toast.show("Ce compte est déjà activé : il n'y a plus d'invitation à renvoyer.", 'error');
          this.load();
        } else {
          this.toast.show("L'invitation n'a pas pu être renvoyée. Réessayez plus tard.", 'error');
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
        this.toast.show('Double authentification réinitialisée.');
        this.focusActions();
      },
      error: (err: { status?: number }) => {
        this.resetting.set(false);
        this.resetOpen.set(false);
        if (err.status === 404) {
          this.accountGone();
          return;
        }
        this.toast.show("La double authentification n'a pas pu être réinitialisée. Réessayez plus tard.", 'error');
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
          this.toast.show(`Mot de passe réinitialisé. Un lien pour en choisir un nouveau a été envoyé à ${user.email}.`);
          this.focusActions();
        } else {
          // The reset went through and the old password no longer works, so this link is the user's only way back in.
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
          this.toast.show("Ce compte n'est pas encore activé : renvoyez-lui plutôt l'invitation.", 'error');
          this.load();
        } else {
          this.toast.show("Le mot de passe n'a pas pu être réinitialisé. Réessayez plus tard.", 'error');
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
    this.busy.set('Enregistrement en cours');
    this.users.setAdmin(user.id, true).subscribe({
      next: () => {
        this.busy.set(null);
        this.patchUser({ isAdmin: true });
        this.toast.show(promotedToast(user.username));
        this.focusActions();
      },
      error: (err: { status?: number }) => {
        this.busy.set(null);
        this.adminChangeFailed(err, PROMOTE_FAILED);
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
        this.adminChangeFailed(err, DEMOTE_FAILED);
      },
    });
  }

  private adminChangeFailed(err: { status?: number }, fallback: string): void {
    if (err.status === 404) {
      this.accountGone();
      return;
    }
    this.focusActions();
    this.toast.show(err.status === 409 ? LAST_ADMIN_DEMOTE_REFUSED : fallback, 'error');
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
        this.toast.show(`Le compte de ${user.username} a été supprimé.`);
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
          // An expected refusal the administrator can act on, so it stays on the page until dismissed instead of in a passing toast.
          this.deletionRefused.set(refusal);
          afterNextRender(() => this.host.nativeElement.querySelector<HTMLElement>('.admin-user-detail__refusal')?.focus(), { injector: this.injector });
        } else if (err.status === 400) {
          this.toast.show('Vous ne pouvez pas supprimer votre propre compte.', 'error');
        } else {
          this.toast.show("Le compte n'a pas pu être supprimé. Réessayez plus tard.", 'error');
        }
      },
    });
  }
}
