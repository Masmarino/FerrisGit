import { Component, inject } from '@angular/core';
import { Router, RouterLink } from '@angular/router';
import { AuthFooterLink, AuthResetPassword, Button } from '@masmarino/gabarit';
import { AuthLogo } from '../auth-logo/auth-logo';
import { consumeLinkToken } from '../link-token';

/**
 * Owns the `/reset-password#token=…` URL, like `ActivatePage`; Gabarit's reset page asks for the new password.
 * `passwordReset` is left unbound so the user stays on the kit's success view until they choose to sign in.
 */
@Component({
  selector: 'fg-reset-password-page',
  standalone: true,
  imports: [AuthResetPassword, AuthLogo, AuthFooterLink, Button, RouterLink],
  template: `
    <gbt-auth-reset-password [token]="token" (signIn)="toSignIn()">
      <picture auth-logo fgAuthLogo></picture>
      <a gbtButton variant="link" gbtAuthFooterLink routerLink="/login">Se connecter</a>
    </gbt-auth-reset-password>
  `,
  // The logo artwork has a transparent margin above the drawing; this pulls it back to the panel's padding.
  styles: ':host { --gbt-auth-panel-logo-offset: -0.5rem; }',
})
export class ResetPasswordPage {
  private router = inject(Router);

  protected readonly token = consumeLinkToken('/reset-password');

  protected toSignIn(): void {
    this.router.navigateByUrl('/login');
  }
}
