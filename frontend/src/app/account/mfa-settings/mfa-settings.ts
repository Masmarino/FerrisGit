import { Component, inject } from '@angular/core';
import { Router } from '@angular/router';
import { MfaSettings as GbtMfaSettings } from '@masmarino/gabarit/mfa-settings';
import { AuthService } from '../../auth/auth.service';
import { provideFerrisgitAuth } from '../../auth/auth-kit';

/**
 * Removing the app revokes this session on the server and, MFA being mandatory, no fresh token comes back. On
 * `sessionRevoked` we drop the local session and go to sign-in, where the user enrols again unless a passkey remains.
 * The card goes inert by itself but can't clear the stored token, so this binding does the sign-out.
 */
@Component({
  selector: 'fg-mfa-settings',
  standalone: true,
  imports: [GbtMfaSettings],
  providers: [provideFerrisgitAuth()],
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
