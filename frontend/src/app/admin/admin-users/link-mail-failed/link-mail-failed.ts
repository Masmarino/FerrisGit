import { booleanAttribute, Component, computed, ElementRef, input, output, viewChild } from '@angular/core';
import { Alert } from '@masmarino/gabarit/alert';
import { CopyField } from '@masmarino/gabarit/copy-field';
import { TranslocoPipe } from '@jsverse/transloco';
import { t } from '../../../shared/i18n/translator';

export type MailedLinkKind = 'invitation' | 'password-reset';

/** What the alert needs when the mail failed: the link, if one was issued, and why it failed. */
export interface MailFailure {
  kind: MailedLinkKind;
  username: string;
  url?: string;
  emailError?: string;
}

interface Wording {
  lifetime: string;
  reissue: string;
  copyLabel: string;
  noLink: (username: string) => string;
}

const wordingFor = (kind: MailedLinkKind): Wording =>
  kind === 'invitation'
    ? {
        lifetime: t('admin.users.mailFailed.invitationLifetime'),
        reissue: t('admin.users.resendInvitation'),
        copyLabel: t('admin.users.mailFailed.invitationCopy'),
        noLink: (username) => t('admin.users.mailFailed.invitationNoLink', { name: username }),
      }
    : {
        lifetime: t('admin.users.mailFailed.resetLifetime'),
        reissue: t('admin.users.resetPassword'),
        copyLabel: t('admin.users.mailFailed.resetCopy'),
        noLink: (username) => t('admin.users.mailFailed.resetNoLink', { name: username }),
      };

/** The link is the only way to reach the user, and the administrator sees it this once. */
@Component({
  selector: 'fg-link-mail-failed',
  standalone: true,
  imports: [TranslocoPipe, Alert, CopyField],
  templateUrl: './link-mail-failed.html',
  styleUrl: './link-mail-failed.scss',
})
export class LinkMailFailed {
  username = input.required<string>();
  kind = input<MailedLinkKind>('invitation');
  /** Missing only if the server sent no link; the alert then points to the row action that issues one. */
  url = input<string | undefined>(undefined);
  emailError = input<string | undefined>(undefined);
  dismissible = input(false, { transform: booleanAttribute });

  dismissed = output<void>();

  protected wording = computed(() => wordingFor(this.kind()));

  private region = viewChild<ElementRef<HTMLElement>>('region');

  /** The alert shows up far from its row, so the page moves focus to it. */
  focus(): void {
    this.region()?.nativeElement.focus();
  }
}
