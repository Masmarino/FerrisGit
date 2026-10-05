import { Component, inject } from '@angular/core';
import { Router, RouterLink } from '@angular/router';
import { AuthFooterLink } from '@masmarino/gabarit/auth';
import { AuthActivate } from '@masmarino/gabarit/auth-activate';
import { Button } from '@masmarino/gabarit/button';
import { AuthLogo } from '../auth-logo/auth-logo';
import { GitField } from '@masmarino/gabarit/git-field';
import { consumeLinkToken } from '../link-token';
import { provideFerrisgitAuth } from '../auth-kit';

/**
 * Owns the `/invitation#token=…` URL of an administrator's invitation mail. The administrator gave only an e-mail
 * address, so Gabarit's activation page asks for the username along with the password. A registered account's link
 * goes to `/activate`, which only asks for the password.
 */
@Component({
  selector: 'fg-invitation-page',
  standalone: true,
  imports: [AuthActivate, AuthLogo, AuthFooterLink, Button, RouterLink, GitField],
  providers: [provideFerrisgitAuth()],
  host: { class: 'fg-auth-page' },
  template: `
    <gbt-auth-activate [token]="token" [chooseUsername]="true" (signIn)="toSignIn()">
      <gbt-git-field auth-backdrop />
      <picture auth-logo fgAuthLogo></picture>
      <a gbtButton variant="link" gbtAuthFooterLink routerLink="/login">Se connecter</a>
    </gbt-auth-activate>
  `,
  // The logo artwork has a transparent top margin; the offset pulls it back to the panel's padding.
  styles: ':host { --gbt-auth-panel-logo-offset: -0.5rem; }',
})
export class InvitationPage {
  private router = inject(Router);

  protected readonly token = consumeLinkToken('/invitation');

  protected toSignIn(): void {
    this.router.navigateByUrl('/login');
  }
}
