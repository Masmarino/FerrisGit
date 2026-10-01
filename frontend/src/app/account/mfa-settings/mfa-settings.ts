import { Component, inject } from '@angular/core';
import { Router } from '@angular/router';
import { MfaSettings as GbtMfaSettings } from '@masmarino/gabarit';
import { AuthService } from '../../auth/auth.service';

/**
 * Removing the app revokes this session on the server. MFA is mandatory, so no fresh token comes back. On
 * `sessionRevoked` the local session is dropped and the user goes to the sign-in page, where they must enrol again
 * unless a passkey remains. The card goes inert on its own but can't clear the stored token, so this binding is what
 * signs the user out.
 */
@Component({
  selector: 'fg-mfa-settings',
  standalone: true,
  imports: [GbtMfaSettings],
  template: `<gbt-mfa-settings (sessionRevoked)="signOut()" />`,
  styles: ':host { display: block; min-width: 0; }',
})
export class MfaSettings {
  private auth = inject(AuthService);
  private router = inject(Router);

  protected signOut(): void {
    this.auth.logout();
    this.router.navigateByUrl('/login');
  }
}
