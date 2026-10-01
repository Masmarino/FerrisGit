import { BadgeVariant, formatDateTime, formatRelativeTime } from '@masmarino/gabarit';
import { AdminUser } from '../admin-users.service';

export interface Presentation {
  label: string;
  variant: BadgeVariant;
  icon: string;
}

const ACTIVE: Presentation = { label: 'Actif', variant: 'success', icon: 'circle-check' };
export const PENDING: Presentation = { label: 'Invitation en attente', variant: 'warning', icon: 'clock' };
export const EXPIRED: Presentation = { label: 'Invitation expirée', variant: 'error', icon: 'alert-circle' };
const MFA_ON: Presentation = { label: 'Double authentification active', variant: 'success', icon: 'shield-check' };
const MFA_OFF: Presentation = { label: 'Non configurée', variant: 'neutral', icon: 'shield-alert' };
export const SUPER_ADMIN: Presentation = { label: 'Super-administrateur', variant: 'info', icon: 'shield-check' };

export interface RowDate {
  iso: string;
  label: string;
  title: string;
}

export const RELATIVE_OPTIONS = { style: 'short', maxUnit: 'day', absoluteAfterDays: 30 } as const;
export const ABSOLUTE_OPTIONS = { day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' } as const;

export function rowDate(iso: string, now: Date): RowDate {
  const relative = formatRelativeTime(iso, 'fr', now, RELATIVE_OPTIONS);
  return { iso, label: /^\d/.test(relative) ? `le ${relative}` : relative, title: formatDateTime(iso, 'fr', ABSOLUTE_OPTIONS) };
}

export function accountState(user: AdminUser, now: Date): { state: Presentation; expiry: RowDate | null } {
  if (user.state !== 'invited') {
    return { state: ACTIVE, expiry: null };
  }
  const expiresAt = user.invitationExpiresAt;
  if (!expiresAt) {
    return { state: PENDING, expiry: null };
  }
  const expired = new Date(expiresAt).getTime() <= now.getTime();
  const title = formatDateTime(expiresAt, 'fr', ABSOLUTE_OPTIONS);
  return { state: expired ? EXPIRED : PENDING, expiry: { iso: expiresAt, title, label: expired ? `a expiré ${rowDate(expiresAt, now).label}` : `expire le ${title}` } };
}

export const mfaPresentation = (user: AdminUser): Presentation | null => (user.state === 'invited' ? null : user.mfaEnabled ? MFA_ON : MFA_OFF);

export const adminAction = (isAdmin: boolean) =>
  isAdmin ? { label: 'Retirer les droits de super-administrateur', icon: 'shield-off' } : { label: 'Nommer super-administrateur', icon: 'shield-plus' };

export const plural = (count: number, one: string, many: string) => `${count} ${count === 1 ? one : many}`;

export const mfaResetMessage = (username: string, self: boolean) =>
  self
    ? "C'est votre propre compte : vous serez déconnecté aussitôt de tous vos appareils et devrez configurer à nouveau la double authentification à votre prochaine connexion. Votre application d'authentification et vos codes de secours actuels cesseront de fonctionner."
    : `${username} sera déconnecté de tous ses appareils et devra configurer à nouveau la double authentification à sa prochaine connexion. Son application d'authentification et ses codes de secours actuels cesseront de fonctionner.`;

export const passwordResetMessage = (username: string) =>
  `Le mot de passe actuel de ${username} cessera de fonctionner immédiatement, et ${username} sera déconnecté de tous ses appareils. Il recevra par e-mail un lien valable 1 heure pour en choisir un nouveau : il ne pourra pas se connecter avant de l'avoir utilisé.`;

export const demoteMessage = (username: string, self: boolean) =>
  self
    ? "Vous allez vous retirer vous-même des super-administrateurs : vous perdrez aussitôt l'accès à l'administration (utilisateurs, réglages de l'instance, runners). Seul un autre super-administrateur pourra vous rendre ces droits."
    : `${username} ne pourra plus gérer les utilisateurs, les réglages de l'instance ni les runners. Vous pourrez lui rendre ces droits plus tard.`;

export const DEMOTE_HEADING = 'Retirer les droits de super-administrateur';

export const promotedToast = (username: string) => `${username} est maintenant super-administrateur.`;
export const demotedToast = (username: string) => `${username} n'est plus super-administrateur.`;
export const SELF_DEMOTED_TOAST = "Vous n'êtes plus super-administrateur.";
export const PROMOTE_FAILED = "Les droits de super-administrateur n'ont pas pu être accordés. Réessayez plus tard.";
export const DEMOTE_FAILED = "Les droits de super-administrateur n'ont pas pu être retirés. Réessayez plus tard.";
export const LAST_ADMIN_DEMOTE_REFUSED = "Impossible de retirer les droits de super-administrateur : l'instance n'aurait plus aucun super-administrateur actif.";
export const ACCOUNT_GONE = "Ce compte n'existe plus.";

export const apiMessage = (err: { error?: unknown }) => (typeof err.error === 'object' && err.error !== null ? (err.error as { error?: unknown }).error : undefined);

/** The server's 400 for an admin resetting their own password (the pages never offer it; a stale page could). */
export const OWN_PASSWORD_REFUSED = 'use your account settings to change your own password';
/** The server's 400 for a reset on an account still pending activation (the pages never offer it either). */
export const PENDING_ACTIVATION_REFUSED = 'the user has not activated their account yet; resend the invitation instead';
export const LAST_ADMIN_REFUSED = 'cannot remove the last administrator';
/** The server's 409 when deleting the last Maintainer of a group hierarchy. The message is this prefix, the group's path, then {@link LAST_MAINTAINER_SUFFIX}. */
const LAST_MAINTAINER_PREFIX = 'the user is the last maintainer of the group ';
const LAST_MAINTAINER_SUFFIX = '; promote another member first';

export function lastMaintainerGroup(message: unknown): string | null {
  if (typeof message !== 'string' || !message.startsWith(LAST_MAINTAINER_PREFIX)) {
    return null;
  }
  const rest = message.slice(LAST_MAINTAINER_PREFIX.length);
  return rest.endsWith(LAST_MAINTAINER_SUFFIX) ? rest.slice(0, -LAST_MAINTAINER_SUFFIX.length) : rest;
}
