import { booleanAttribute, Component, computed, ElementRef, input, output, viewChild } from '@angular/core';
import { Alert } from '@masmarino/gabarit/alert';
import { CopyField } from '@masmarino/gabarit/copy-field';

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

const WORDING: Record<MailedLinkKind, Wording> = {
  invitation: {
    lifetime: '24 heures',
    reissue: "Renvoyer l'invitation",
    copyLabel: "Copier le lien d'activation",
    noLink: (username) => `Utilisez « Renvoyer l'invitation » pour obtenir le lien d'activation de ${username}.`,
  },
  'password-reset': {
    lifetime: '1 heure',
    reissue: 'Réinitialiser le mot de passe',
    copyLabel: 'Copier le lien de réinitialisation',
    noLink: (username) => `Utilisez à nouveau « Réinitialiser le mot de passe » pour obtenir un lien pour ${username}.`,
  },
};

/** The link is the only way to reach the user, and the administrator sees it this once. */
@Component({
  selector: 'fg-link-mail-failed',
  standalone: true,
  imports: [Alert, CopyField],
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

  protected wording = computed(() => WORDING[this.kind()]);

  private region = viewChild<ElementRef<HTMLElement>>('region');

  /** The alert shows up far from its row, so the page moves focus to it. */
  focus(): void {
    this.region()?.nativeElement.focus();
  }
}
