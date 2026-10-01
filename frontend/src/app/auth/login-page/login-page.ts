import { Component, inject } from '@angular/core';
import { ActivatedRoute, Router, RouterLink } from '@angular/router';
import { AuthFooterLink, AuthLogin, Button } from '@masmarino/gabarit';
import { AuthLogo } from '../auth-logo/auth-logo';
import { safeReturnUrl } from '../../public/login-link';

@Component({
  selector: 'fg-login-page',
  standalone: true,
  imports: [AuthLogin, AuthLogo, AuthFooterLink, Button, RouterLink],
  template: `
    <gbt-auth-login (loggedIn)="signedIn()">
      <picture auth-logo fgAuthLogo></picture>
      <a gbtButton variant="link" gbtAuthFooterLink routerLink="/register">Créer un compte</a>
    </gbt-auth-login>
  `,
  // The logo artwork has a transparent margin above the drawing. Pull it back so the drawing sits at the panel's padding.
  styles: ':host { --gbt-auth-panel-logo-offset: -0.5rem; }',
})
export class LoginPage {
  private router = inject(Router);
  private route = inject(ActivatedRoute);

  /** Back to the public page that sent the visitor here (`?returnUrl=`), when it is a path on this site. */
  protected signedIn(): void {
    this.router.navigateByUrl(safeReturnUrl(this.route.snapshot.queryParamMap.get('returnUrl')) ?? '/repositories');
  }
}
