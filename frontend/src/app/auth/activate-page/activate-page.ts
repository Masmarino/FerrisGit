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
 * Owns the `/activate#token=…` URL; Gabarit's page asks for the password. A missing or malformed token shows the kit's
 * dead-link view without any request. Leaving `activated` unbound keeps the user on the success view until they sign in.
 */
@Component({
  selector: 'fg-activate-page',
  standalone: true,
  imports: [AuthActivate, AuthLogo, AuthFooterLink, Button, RouterLink, GitField],
  providers: [provideFerrisgitAuth()],
  host: { class: 'fg-auth-page' },
  template: `
    <gbt-auth-activate [token]="token" (signIn)="toSignIn()">
      <gbt-git-field auth-backdrop />
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
