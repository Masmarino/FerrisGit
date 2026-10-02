import { Component, inject } from '@angular/core';
import { Router, RouterLink } from '@angular/router';
import { AuthActivate, AuthFooterLink, Button } from '@masmarino/gabarit';
import { AuthLogo } from '../auth-logo/auth-logo';
import { consumeLinkToken } from '../link-token';

/**
 * Owns the `/activate#token=…` URL; Gabarit's page asks for the password. A missing or malformed token shows the kit's
 * dead-link view without any request. Leaving `activated` unbound keeps the user on the success view until they sign in.
 */
@Component({
  selector: 'fg-activate-page',
  standalone: true,
  imports: [AuthActivate, AuthLogo, AuthFooterLink, Button, RouterLink],
  template: `
    <gbt-auth-activate [token]="token" (signIn)="toSignIn()">
      <picture auth-logo fgAuthLogo></picture>
      <a gbtButton variant="link" gbtAuthFooterLink routerLink="/login">Se connecter</a>
    </gbt-auth-activate>
  `,
  // The logo artwork has a transparent top margin; the offset pulls it back to the panel's padding.
  styles: ':host { --gbt-auth-panel-logo-offset: -0.5rem; }',
})
export class ActivatePage {
  private router = inject(Router);

  protected readonly token = consumeLinkToken('/activate');

  protected toSignIn(): void {
    this.router.navigateByUrl('/login');
  }
}
