import { Component, inject } from '@angular/core';
import { Router } from '@angular/router';
import { PasskeySettings as GbtPasskeySettings } from '@masmarino/gabarit';
import { AuthService } from '../../auth/auth.service';

/**
 * Deleting a passkey revokes every session of the account, this one included, and no fresh token comes back. On
 * `sessionRevoked` the local session is dropped and the user goes to the sign-in page. The card goes inert on its own
 * but can't clear the stored token, so this binding is what signs the user out.
 */
@Component({
  selector: 'fg-passkey-settings',
  standalone: true,
  imports: [GbtPasskeySettings],
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
