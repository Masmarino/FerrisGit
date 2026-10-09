import { BadgeVariant } from '@masmarino/gabarit/badge';
import { formatDateTime } from '@masmarino/gabarit/format';
import { AdminUser } from '../admin-users.service';
import { ABSOLUTE_OPTIONS, RowDate, rowDate } from '../row-date';
import { activeLocale, t } from '../../shared/i18n/translator';

export interface Presentation {
  label: string;
  variant: BadgeVariant;
  icon: string;
}

const ACTIVE: Presentation = { get label() { return t('admin.users.state.active'); }, variant: 'success', icon: 'circle-check' };
export const PENDING: Presentation = { get label() { return t('admin.users.state.pending'); }, variant: 'warning', icon: 'clock' };
export const EXPIRED: Presentation = { get label() { return t('admin.users.state.expired'); }, variant: 'error', icon: 'alert-circle' };
const MFA_ON: Presentation = { get label() { return t('admin.users.state.mfaOn'); }, variant: 'success', icon: 'shield-check' };
const MFA_OFF: Presentation = { get label() { return t('admin.users.state.mfaOff'); }, variant: 'neutral', icon: 'shield-alert' };
export const SUPER_ADMIN: Presentation = { get label() { return t('common.superAdmin'); }, variant: 'info', icon: 'shield-check' };

export function accountState(user: AdminUser, now: Date): { state: Presentation; expiry: RowDate | null } {
  if (user.state !== 'invited') {
    return { state: ACTIVE, expiry: null };
  }
  const expiresAt = user.invitationExpiresAt;
  if (!expiresAt) {
    return { state: PENDING, expiry: null };
  }
  const expired = new Date(expiresAt).getTime() <= now.getTime();
  const title = formatDateTime(expiresAt, activeLocale(), ABSOLUTE_OPTIONS);
  return { state: expired ? EXPIRED : PENDING, expiry: { iso: expiresAt, title, label: expired ? t('admin.users.expired', { when: rowDate(expiresAt, now).label }) : t('admin.users.expires', { date: title }) } };
}

export const mfaPresentation = (user: AdminUser): Presentation | null => (user.state === 'invited' ? null : user.mfaEnabled ? MFA_ON : MFA_OFF);

export const adminAction = (isAdmin: boolean) =>
  isAdmin ? { label: t('admin.users.demote'), icon: 'shield-off' } : { label: t('admin.users.promote'), icon: 'shield-plus' };

export const mfaResetMessage = (username: string, self: boolean) =>
  self
    ? t('admin.users.mfaResetSelf')
    : t('admin.users.mfaReset', { name: username });

export const passwordResetMessage = (username: string) =>
  t('admin.users.passwordReset', { name: username });

export const demoteMessage = (username: string, self: boolean) =>
  self
    ? t('admin.users.demoteSelf')
    : t('admin.users.demoteOther', { name: username });

export const demoteHeading = (): string => t('admin.users.demote');

export const promotedToast = (username: string) => t('admin.users.promoted', { name: username });
export const demotedToast = (username: string) => t('admin.users.demoted', { name: username });
export const selfDemotedToast = (): string => t('admin.users.selfDemoted');
export const promoteFailed = (): string => t('admin.users.promoteFailed');
export const demoteFailed = (): string => t('admin.users.demoteFailed');
export const lastAdminDemoteRefused = (): string => t('admin.users.lastAdmin');
export const accountGone = (): string => t('admin.users.accountGone');
export const alreadyActive = (): string => t('admin.users.alreadyActive');
export const notActivated = (): string => t('admin.users.notActivated');
export const resendFailed = (): string => t('admin.users.resendFailed');
export const mfaResetDone = (): string => t('admin.users.mfaResetDone');
export const mfaResetFailed = (): string => t('admin.users.mfaResetFailed');
export const passwordResetFailed = (): string => t('admin.users.passwordResetFailed');
export const invitationResentToast = (email: string) => t('admin.users.invitationResent', { email });
export const passwordResetToast = (email: string) => t('admin.users.passwordResetDone', { email });

export const apiMessage = (err: { error?: unknown }) => (typeof err.error === 'object' && err.error !== null ? (err.error as { error?: unknown }).error : undefined);

/** The server's 400 for an admin resetting their own password. The pages never offer it, but a stale page could. */
export const OWN_PASSWORD_REFUSED = 'use your account settings to change your own password';
/** The server's 400 for a reset on an account still pending activation (never offered either). */
export const PENDING_ACTIVATION_REFUSED = 'the user has not activated their account yet; resend the invitation instead';
export const LAST_ADMIN_REFUSED = 'cannot remove the last administrator';
/** The server's 409 for deleting the last Maintainer of a group hierarchy. The message is this prefix, the group's path, then {@link LAST_MAINTAINER_SUFFIX}. */
const LAST_MAINTAINER_PREFIX = 'the user is the last maintainer of the group ';
const LAST_MAINTAINER_SUFFIX = '; promote another member first';

export function lastMaintainerGroup(message: unknown): string | null {
  if (typeof message !== 'string' || !message.startsWith(LAST_MAINTAINER_PREFIX)) {
    return null;
  }
  const rest = message.slice(LAST_MAINTAINER_PREFIX.length);
  return rest.endsWith(LAST_MAINTAINER_SUFFIX) ? rest.slice(0, -LAST_MAINTAINER_SUFFIX.length) : rest;
}
