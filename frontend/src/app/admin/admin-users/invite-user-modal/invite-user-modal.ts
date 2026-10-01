import { afterNextRender, Component, ElementRef, inject, Injector, output, signal, viewChild } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Alert, Button, GbtInput, Modal, Switch } from '@masmarino/gabarit';
import { AdminUsersService, InviteResult } from '../../admin-users.service';
import { emailError as checkEmail, USERNAME_ERROR, USERNAME_HINT, usernameError as checkUsername } from '../../../auth/account-rules';
import { classifyRegisterFailure, REGISTER_INVALID_MESSAGE, REGISTER_RESERVED_MESSAGE, REGISTER_TAKEN_MESSAGE } from '../../../auth/account-errors';
import { LinkMailFailed } from '../link-mail-failed/link-mail-failed';

const EMAIL_REQUIRED = "Saisissez l'adresse e-mail";
const EMAIL_INVALID = 'Saisissez une adresse e-mail valide, par exemple nom@exemple.fr';
const SEND_FAILED = "L'invitation n'a pas pu être envoyée. Réessayez plus tard.";

/**
 * The parent renders it under `@if` (rebuilt on each opening, so a closed dialog never keeps a draft). A refused
 * invitation keeps the dialog open with the draft. An accepted one emits `invited` and shows its result. `close` is the
 * parent's cue to remove it.
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

  // Not `protected` because the spec calls it directly (the codebase's convention for members the tests drive).
  username = signal('');
  email = signal('');
  isAdmin = signal(false);
  usernameError = signal<string | null>(null);
  emailError = signal<string | null>(null);
  formError = signal<string | null>(null);
  sending = signal(false);
  result = signal<InviteResult | null>(null);

  protected readonly usernameHint = USERNAME_HINT;
  private resultRegion = viewChild<ElementRef<HTMLElement>>('resultRegion');

  onUsernameChange(value: string): void {
    this.username.set(value);
    this.usernameError.set(null);
  }

  onEmailChange(value: string): void {
    this.email.set(value);
    this.emailError.set(null);
  }

  submit(): void {
    if (this.sending()) {
      return;
    }
    const username = this.username().trim();
    const email = this.email().trim();
    const usernameProblem = checkUsername(username);
    const emailProblem = email === '' ? EMAIL_REQUIRED : checkEmail(email);
    this.usernameError.set(usernameProblem);
    this.emailError.set(emailProblem);
    this.formError.set(null);
    if (usernameProblem || emailProblem) {
      return;
    }
    this.sending.set(true);
    this.users.invite(username, email, this.isAdmin()).subscribe({
      next: (result) => {
        this.sending.set(false);
        this.result.set(result);
        this.invited.emit(result);
        // The form that had the focus is gone, so move the focus to the result, the next thing to read.
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
      case 'username-taken':
      case 'email-taken':
        this.formError.set(REGISTER_TAKEN_MESSAGE);
        break;
      case 'username-invalid':
        this.usernameError.set(USERNAME_ERROR);
        break;
      case 'username-reserved':
        this.usernameError.set(REGISTER_RESERVED_MESSAGE);
        break;
      case 'email-invalid':
        this.emailError.set(EMAIL_INVALID);
        break;
      case 'invalid':
      case 'disabled':
      case 'password-weak':
        this.formError.set(REGISTER_INVALID_MESSAGE);
        break;
      default:
        this.formError.set(SEND_FAILED);
    }
  }
}
