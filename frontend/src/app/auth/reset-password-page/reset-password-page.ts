import { Component, afterNextRender, inject } from '@angular/core';
import { ActivatedRoute, Router, RouterLink } from '@angular/router';
import { AuthFooterLink, AuthResetPassword, Button, activationToken } from '@masmarino/gabarit';
import { AuthLogo } from '../auth-logo/auth-logo';

const RESET_PASSWORD_PATH = '/reset-password';

/**
 * Owns the `/reset-password#token=…` URL, like `ActivatePage`. Gabarit's reset page asks for the new password.
 * The token is read once, from the fragment so no server sees it (`?token=` works too). It goes into a plain field, not
 * a value that follows the URL, because the kit's page restarts when its `token` changes. Then it is scrubbed from the
 * address bar and the history entry. A token not shaped like the server's counts as no token: the page shows the
 * dead-link view without sending any request. `passwordReset` is left unbound so the user stays on the kit's success
 * view until they choose to sign in.
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
  // The logo artwork has a transparent margin above the drawing. Pull it back so the drawing sits at the panel's padding.
  styles: ':host { --gbt-auth-panel-logo-offset: -0.5rem; }',
})
export class ResetPasswordPage {
  private router = inject(Router);
  private route = inject(ActivatedRoute);

  protected readonly token: string | null = activationToken(this.route.snapshot.fragment, this.route.snapshot.queryParamMap.get('token'));
  private readonly linkHadParameters = this.route.snapshot.fragment !== null || this.route.snapshot.queryParamMap.keys.length > 0;

  constructor() {
    if (this.linkHadParameters) {
      afterNextRender(() => void this.router.navigateByUrl(RESET_PASSWORD_PATH, { replaceUrl: true }).catch(() => undefined));
    }
  }

  protected toSignIn(): void {
    this.router.navigateByUrl('/login');
  }
}
