import { afterNextRender, Component, ElementRef, inject, Injector, output, signal, viewChild } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Alert } from '@masmarino/gabarit/alert';
import { Button } from '@masmarino/gabarit/button';
import { GbtInput } from '@masmarino/gabarit/input';
import { Modal } from '@masmarino/gabarit/modal';
import { Switch } from '@masmarino/gabarit/switch';
import { AdminUsersService, InviteResult } from '../../admin-users.service';
import { emailError as checkEmail } from '../../../auth/account-rules';
import { classifyRegisterFailure, REGISTER_INVALID_MESSAGE } from '../../../auth/account-errors';
import { LinkMailFailed } from '../link-mail-failed/link-mail-failed';

const EMAIL_REQUIRED = "Saisissez l'adresse e-mail";
const EMAIL_INVALID = 'Saisissez une adresse e-mail valide, par exemple nom@exemple.fr';
const EMAIL_TAKEN = 'Cette adresse e-mail est déjà utilisée par un compte';
const SEND_FAILED = "L'invitation n'a pas pu être envoyée. Réessayez plus tard.";

/**
 * Rendered under @if by the parent, so each opening starts fresh and a closed dialog never keeps a draft.
 * A refused invitation leaves it open with the draft; an accepted one emits `invited` and shows the result.
 * `close` is the parent's cue to remove it.
 */
@Component({
  selector: 'fg-invite-user-modal',
  standalone: true,
  imports: [FormsModule, Modal, Alert, Button, GbtInput, Switch, LinkMailFailed],
  templateUrl: './invite-user-modal.html',
  styleUrl: './invite-user-modal.scss',
})
export class InviteUserModal {
  private users = inject(AdminUsersService);
  private injector = inject(Injector);

  close = output<void>();
  invited = output<InviteResult>();

  // Public so the spec can call it.
  email = signal('');
  isAdmin = signal(false);
  emailError = signal<string | null>(null);
  formError = signal<string | null>(null);
  sending = signal(false);
  result = signal<InviteResult | null>(null);

  private resultRegion = viewChild<ElementRef<HTMLElement>>('resultRegion');

  onEmailChange(value: string): void {
    this.email.set(value);
    this.emailError.set(null);
  }

  submit(): void {
    if (this.sending()) {
      return;
    }
    const email = this.email().trim();
    const emailProblem = email === '' ? EMAIL_REQUIRED : checkEmail(email);
    this.emailError.set(emailProblem);
    this.formError.set(null);
    if (emailProblem) {
      return;
    }
    this.sending.set(true);
    this.users.invite(email, this.isAdmin()).subscribe({
      next: (result) => {
        this.sending.set(false);
        this.result.set(result);
        this.invited.emit(result);
        // The form that had the focus is gone, so move it to the result, the next thing to read.
        afterNextRender(() => this.resultRegion()?.nativeElement.focus(), { injector: this.injector });
      },
      error: (err: unknown) => {
        this.sending.set(false);
        this.refused(err);
      },
    });
  }

  private refused(err: unknown): void {
    switch (classifyRegisterFailure(err)) {
      case 'email-taken':
        this.emailError.set(EMAIL_TAKEN);
        break;
      case 'email-invalid':
        this.emailError.set(EMAIL_INVALID);
        break;
      case 'invalid':
      case 'disabled':
      case 'password-weak':
      case 'username-taken':
      case 'username-invalid':
      case 'username-reserved':
        this.formError.set(REGISTER_INVALID_MESSAGE);
        break;
      default:
        this.formError.set(SEND_FAILED);
    }
  }
}
