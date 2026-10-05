import { Component, inject } from '@angular/core';
import { Router, RouterLink } from '@angular/router';
import { AuthFooterLink } from '@masmarino/gabarit/auth';
import { AuthResetPassword } from '@masmarino/gabarit/auth-reset-password';
import { Button } from '@masmarino/gabarit/button';
import { AuthLogo } from '../auth-logo/auth-logo';
import { GitField } from '@masmarino/gabarit/git-field';
import { consumeLinkToken } from '../link-token';
import { provideFerrisgitAuth } from '../auth-kit';

/**
 * Owns the `/reset-password#token=…` URL, like `ActivatePage`; Gabarit's page asks for the new password. Leaving
 * `passwordReset` unbound keeps the user on the kit's success view until they choose to sign in.
 */
@Component({
  selector: 'fg-reset-password-page',
  standalone: true,
  imports: [AuthResetPassword, AuthLogo, AuthFooterLink, Button, RouterLink, GitField],
  providers: [provideFerrisgitAuth()],
  host: { class: 'fg-auth-page' },
  template: `
    <gbt-auth-reset-password [token]="token" (signIn)="toSignIn()">
      <gbt-git-field auth-backdrop />
      <picture auth-logo fgAuthLogo></picture>
      <a gbtButton variant="link" gbtAuthFooterLink routerLink="/login">Se connecter</a>
    </gbt-auth-reset-password>
  `,
  // The logo artwork has a transparent top margin; the offset pulls it back to the panel's padding.
  styles: ':host { --gbt-auth-panel-logo-offset: -0.5rem; }',
})
export class ResetPasswordPage {
  private router = inject(Router);

  protected readonly token = consumeLinkToken('/reset-password');

  protected toSignIn(): void {
    this.router.navigateByUrl('/login');
  }
}
