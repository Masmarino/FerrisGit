import { afterNextRender, Component, computed, ElementRef, inject, Injector, signal } from '@angular/core';
import { HttpErrorResponse } from '@angular/common/http';
import { FormsModule } from '@angular/forms';
import { Router, RouterLink } from '@angular/router';
import { Alert } from '@masmarino/gabarit/alert';
import { AuthFooter, AuthFooterLink, AuthPanel } from '@masmarino/gabarit/auth';
import { Button } from '@masmarino/gabarit/button';
import { EmptyState } from '@masmarino/gabarit/empty-state';
import { GbtInput } from '@masmarino/gabarit/input';
import { Skeleton } from '@masmarino/gabarit/skeleton';
import { AuthLogo } from '../auth-logo/auth-logo';
import { GitField } from '@masmarino/gabarit/git-field';
import { AuthService } from '../auth.service';
import { emailError, usernameError, usernameHint, usernameInvalidMessage } from '../account-rules';
import { classifyRegisterFailure, registerInvalidMessage, registerReservedMessage, registerTakenMessage } from '../account-errors';
import { provideFerrisgitAuth } from '../auth-kit';
import { TranslocoPipe } from '@jsverse/transloco';
import { t } from '../../shared/i18n/translator';

type Registration = 'loading' | 'open' | 'closed' | 'sent';

/**
 * Signing up only asks for a username and an address: the account is created inactive and the password is chosen from
 * the link mailed to that address, which is how the address gets confirmed. Gabarit's own page asks for a password and
 * signs in, so this one is local.
 */
@Component({
  selector: 'fg-register-page',
  standalone: true,
  imports: [TranslocoPipe, FormsModule, RouterLink, AuthPanel, AuthLogo, AuthFooter, AuthFooterLink, Alert, Button, EmptyState, GbtInput, Skeleton, GitField],
  providers: [provideFerrisgitAuth()],
  host: { class: 'fg-auth-page' },
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
  protected readonly usernameHint = usernameHint();
  protected readonly heading = computed(() => (this.registration() === 'open' ? t('auth.createAccount') : ''));
  protected readonly intro = computed(() =>
    this.registration() === 'open' ? t('auth.register.intro') : '',
  );
  protected readonly sentMessage = computed(
    () => t('auth.register.sentMessage', { email: this.sentTo() }),
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
        this.fail(registerTakenMessage(), `#${this.usernameId}`);
        return;
      case 'username-invalid':
        this.usernameError.set(usernameInvalidMessage());
        this.focus(`#${this.usernameId}`);
        return;
      case 'username-reserved':
        this.usernameError.set(registerReservedMessage());
        this.focus(`#${this.usernameId}`);
        return;
      case 'email-invalid':
        this.emailError.set(t('auth.email.invalid'));
        this.focus(`#${this.emailId}`);
        return;
      case 'invalid':
      case 'password-weak':
        this.fail(registerInvalidMessage(), `#${this.usernameId}`);
        return;
      case 'rate-limited':
        this.fail(t('auth.register.tooManyAttempts'), `#${this.usernameId}`);
        return;
      default:
        // 503: no mail configured, or the message could not leave. Either way nothing was sent.
        this.fail(err instanceof HttpErrorResponse && err.status === 503 ? t('auth.register.mailFailed') : t('auth.register.failed'), `#${this.usernameId}`);
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
