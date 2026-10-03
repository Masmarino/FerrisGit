import { afterNextRender, Component, computed, ElementRef, inject, Injector, signal } from '@angular/core';
import { HttpErrorResponse } from '@angular/common/http';
import { FormsModule } from '@angular/forms';
import { Router, RouterLink } from '@angular/router';
import { Alert, AuthFooter, AuthFooterLink, AuthPanel, Button, EmptyState, GbtInput, Skeleton } from '@masmarino/gabarit';
import { AuthLogo } from '../auth-logo/auth-logo';
import { AuthService } from '../auth.service';
import { emailError, USERNAME_ERROR, USERNAME_HINT, usernameError } from '../account-rules';
import { classifyRegisterFailure, REGISTER_INVALID_MESSAGE, REGISTER_RESERVED_MESSAGE, REGISTER_TAKEN_MESSAGE } from '../account-errors';

type Registration = 'loading' | 'open' | 'closed' | 'sent';

const EMAIL_INVALID = 'Saisissez une adresse e-mail valide, par exemple nom@exemple.fr';
const TOO_MANY_ATTEMPTS = 'Trop de tentatives, réessayez dans quelques minutes.';
const MAIL_FAILED = "Le message de confirmation n'a pas pu être envoyé. Réessayez plus tard.";
const FAILED = "L'inscription a échoué, réessayez.";

/**
 * Signing up only asks for a username and an address: the account is created inactive and the password is chosen from
 * the link mailed to that address, which is how the address gets confirmed. Gabarit's own page asks for a password and
 * signs in, so this one is local.
 */
@Component({
  selector: 'fg-register-page',
  standalone: true,
  imports: [FormsModule, RouterLink, AuthPanel, AuthLogo, AuthFooter, AuthFooterLink, Alert, Button, EmptyState, GbtInput, Skeleton],
  templateUrl: './register-page.html',
  styleUrl: './register-page.scss',
})
export class RegisterPage {
  private auth = inject(AuthService);
  private router = inject(Router);
  private host = inject<ElementRef<HTMLElement>>(ElementRef);
  private injector = inject(Injector);

  // Public so the spec can read and drive them.
  registration = signal<Registration>('loading');
  username = signal('');
  email = signal('');
  usernameError = signal<string | null>(null);
  emailError = signal<string | null>(null);
  formError = signal<string | null>(null);
  submitting = signal(false);
  sentTo = signal('');

  // Bound with [id] in the template: a static id would stay on the <gbt-input> host as well as reach the <input>.
  protected readonly usernameId = 'register-username';
  protected readonly emailId = 'register-email';
  protected readonly usernameHint = USERNAME_HINT;
  protected readonly heading = computed(() => (this.registration() === 'open' ? 'Créer un compte' : ''));
  protected readonly intro = computed(() =>
    this.registration() === 'open' ? 'Rejoignez FerrisGit pour héberger vos dépôts, tickets et demandes de fusion.' : '',
  );
  protected readonly sentMessage = computed(
    () => `Un lien de confirmation a été envoyé à ${this.sentTo()}. Il est valable 24 heures : suivez-le pour choisir votre mot de passe.`,
  );

  constructor() {
    this.auth.authConfig().subscribe({
      next: (config) => this.opened(config.registrationEnabled === true),
      // The server decides at submit time anyway, so a failed read shouldn't hide the form.
      error: () => this.opened(true),
    });
  }

  onUsernameChange(value: string): void {
    this.username.set(value);
    this.usernameError.set(null);
  }

  onEmailChange(value: string): void {
    this.email.set(value);
    this.emailError.set(null);
  }

  submit(): void {
    if (this.submitting()) {
      return;
    }
    const username = this.username().trim();
    const email = this.email().trim();
    const usernameProblem = usernameError(username);
    const emailProblem = emailError(email);
    this.usernameError.set(usernameProblem);
    this.emailError.set(emailProblem);
    this.formError.set(null);
    if (usernameProblem || emailProblem) {
      this.focus(usernameProblem ? `#${this.usernameId}` : `#${this.emailId}`);
      return;
    }
    this.submitting.set(true);
    this.auth.register(username, email).subscribe({
      next: () => {
        this.submitting.set(false);
        this.sentTo.set(email);
        this.registration.set('sent');
        this.focus('h1');
      },
      error: (err: unknown) => {
        this.submitting.set(false);
        this.refused(err);
      },
    });
  }

  protected toSignIn(): void {
    this.router.navigateByUrl('/login');
  }

  private opened(open: boolean): void {
    this.registration.set(open ? 'open' : 'closed');
    this.focus(open ? `#${this.usernameId}` : 'h1');
  }

  private refused(err: unknown): void {
    switch (classifyRegisterFailure(err)) {
      case 'disabled':
        this.opened(false);
        return;
      case 'username-taken':
      case 'email-taken':
        this.fail(REGISTER_TAKEN_MESSAGE, `#${this.usernameId}`);
        return;
      case 'username-invalid':
        this.usernameError.set(USERNAME_ERROR);
        this.focus(`#${this.usernameId}`);
        return;
      case 'username-reserved':
        this.usernameError.set(REGISTER_RESERVED_MESSAGE);
        this.focus(`#${this.usernameId}`);
        return;
      case 'email-invalid':
        this.emailError.set(EMAIL_INVALID);
        this.focus(`#${this.emailId}`);
        return;
      case 'invalid':
      case 'password-weak':
        this.fail(REGISTER_INVALID_MESSAGE, `#${this.usernameId}`);
        return;
      case 'rate-limited':
        this.fail(TOO_MANY_ATTEMPTS, `#${this.usernameId}`);
        return;
      default:
        // 503: no mail configured, or the message could not leave. Either way nothing was sent.
        this.fail(err instanceof HttpErrorResponse && err.status === 503 ? MAIL_FAILED : FAILED, `#${this.usernameId}`);
    }
  }

  private fail(message: string, focusSelector: string): void {
    this.formError.set(message);
    this.focus(focusSelector);
  }

  private focus(selector: string): void {
    afterNextRender(() => this.host.nativeElement.querySelector<HTMLElement>(selector)?.focus(), { injector: this.injector });
  }
}
