import { Component, inject } from '@angular/core';
import { Router } from '@angular/router';
import { PasskeySettings as GbtPasskeySettings } from '@masmarino/gabarit/passkey-settings';
import { AuthService } from '../../auth/auth.service';
import { provideFerrisgitAuth } from '../../auth/auth-kit';

/**
 * Deleting a passkey revokes every session of the account, this one included, with no fresh token. On `sessionRevoked`
 * we drop the local session and go to sign-in. The card goes inert by itself but can't clear the stored token, so this
 * binding does the sign-out.
 */
@Component({
  selector: 'fg-passkey-settings',
  standalone: true,
  imports: [GbtPasskeySettings],
  providers: [provideFerrisgitAuth()],
  template: `<gbt-passkey-settings [locale]="'fr'" (sessionRevoked)="signOut()" />`,
  styles: ':host { display: block; min-width: 0; }',
})
export class PasskeySettings {
  private auth = inject(AuthService);
  private router = inject(Router);

  protected signOut(): void {
    this.auth.logout();
    this.router.navigateByUrl('/login');
  }
}
