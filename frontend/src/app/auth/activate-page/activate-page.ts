import { Component, afterNextRender, inject } from '@angular/core';
import { ActivatedRoute, Router, RouterLink } from '@angular/router';
import { AuthActivate, AuthFooterLink, Button, activationToken } from '@masmarino/gabarit';
import { AuthLogo } from '../auth-logo/auth-logo';

const ACTIVATE_PATH = '/activate';

/**
 * Owns the `/activate#token=…` URL. Gabarit's activation page asks for the password.
 * The token is read once, from the fragment so no server sees it (`?token=` still works for mails sent before). It goes
 * into a plain field, not a value that follows the URL, because the kit's page restarts when its `token` changes. Then
 * it is scrubbed from the address bar and the history entry. A token not shaped like the server's counts as no token:
 * the page shows the dead-link view without sending any request. `activated` is left unbound so the user stays on the
 * kit's success view until they choose to sign in.
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
  // The logo artwork has a transparent margin above the drawing. Pull it back so the drawing sits at the panel's padding.
  styles: ':host { --gbt-auth-panel-logo-offset: -0.5rem; }',
})
export class ActivatePage {
  private router = inject(Router);
  private route = inject(ActivatedRoute);

  protected readonly token: string | null = activationToken(this.route.snapshot.fragment, this.route.snapshot.queryParamMap.get('token'));
  private readonly linkHadParameters = this.route.snapshot.fragment !== null || this.route.snapshot.queryParamMap.keys.length > 0;

  constructor() {
    if (this.linkHadParameters) {
      afterNextRender(() => void this.router.navigateByUrl(ACTIVATE_PATH, { replaceUrl: true }).catch(() => undefined));
    }
  }

  protected toSignIn(): void {
    this.router.navigateByUrl('/login');
  }
}
