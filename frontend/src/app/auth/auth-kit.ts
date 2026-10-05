import type { Provider } from '@angular/core';
import { AUTH_PORT, MFA_PORT, provideAuthLabels } from '@masmarino/gabarit/auth';
import { TOTP_QR_RENDERER } from '@masmarino/gabarit/mfa-enrollment';
import { MfaService } from './mfa.service';
import { AuthService } from './auth.service';
import { FR_AUTH_LABELS } from './auth-labels.fr';
import { renderTotpQr } from './totp-qr-renderer';

/**
 * Wires Gabarit's auth kit to our services, a local QR renderer and the French strings. Each component that uses the
 * kit lists these in its own `providers`, not the app config: the kit then loads with those lazy pages, out of the
 * first chunk. Plain providers rather than environment providers, so a spec or story can list them next to fakes of
 * the two services.
 */
export function provideFerrisgitAuth(): Provider[] {
  return [
    { provide: AUTH_PORT, useExisting: AuthService },
    { provide: MFA_PORT, useExisting: MfaService },
    { provide: TOTP_QR_RENDERER, useValue: renderTotpQr },
    provideAuthLabels(FR_AUTH_LABELS),
  ];
}
