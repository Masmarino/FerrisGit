import { Component, inject } from '@angular/core';
import { ActivatedRoute, Router, RouterLink } from '@angular/router';
import { AuthFooterLink } from '@masmarino/gabarit/auth';
import { AuthLogin } from '@masmarino/gabarit/auth-login';
import { Button } from '@masmarino/gabarit/button';
import { AuthLogo } from '../auth-logo/auth-logo';
import { safeReturnUrl } from '../login-link';
import { GitField } from '@masmarino/gabarit/git-field';
import { provideFerrisgitAuth } from '../auth-kit';
import { TranslocoPipe } from '@jsverse/transloco';

@Component({
  selector: 'fg-login-page',
  standalone: true,
  imports: [TranslocoPipe, AuthLogin, AuthLogo, AuthFooterLink, Button, RouterLink, GitField],
  providers: [provideFerrisgitAuth()],
  host: { class: 'fg-auth-page' },
  template: `
    <gbt-auth-login (loggedIn)="signedIn()">
      <gbt-git-field auth-backdrop />
      <picture auth-logo fgAuthLogo></picture>
      <a gbtButton variant="link" gbtAuthFooterLink routerLink="/register">{{ 'auth.createAccount' | transloco }}</a>
    </gbt-auth-login>
    <footer class="login-page__footer">
      <a gbtButton variant="link" size="small" iconName="book-open" routerLink="/docs">{{ 'nav.docs' | transloco }}</a>
    </footer>
  `,
  // The logo artwork has a transparent top margin; the offset pulls it back to the panel's padding. The docs link sits
  // in the panel's bottom padding, so it never covers the form however tall it grows.
  styles: `
    :host {
      --gbt-auth-panel-logo-offset: -0.5rem;
      position: relative;
      display: block;
    }
    .login-page__footer {
      position: absolute;
      bottom: 0.125rem;
      left: 0;
      right: 0;
      display: flex;
      justify-content: center;
    }
  `,
})
export class LoginPage {
  private router = inject(Router);
  private route = inject(ActivatedRoute);

  /** Back to the public page that sent the visitor here (`?returnUrl=`), when it is a path on this site. */
  protected signedIn(): void {
    this.router.navigateByUrl(safeReturnUrl(this.route.snapshot.queryParamMap.get('returnUrl')) ?? '/repositories');
  }
}
