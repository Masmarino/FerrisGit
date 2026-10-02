import type { Provider } from '@angular/core';
import { AUTH_PORT, MFA_PORT, TOTP_QR_RENDERER, provideAuthLabels } from '@masmarino/gabarit';
import { MfaService } from './mfa.service';
import { AuthService } from './auth.service';
import { FR_AUTH_LABELS } from './auth-labels.fr';
import { renderTotpQr } from './totp-qr-renderer';

/**
 * Wires Gabarit's auth kit to our services, a local QR renderer and the French strings. Plain providers rather than
 * environment providers, so a spec or story can list them next to fakes of the two services.
 */
export function provideFerrisgitAuth(): Provider[] {
  return [
    { provide: AUTH_PORT, useExisting: AuthService },
    { provide: MFA_PORT, useExisting: MfaService },
    { provide: TOTP_QR_RENDERER, useValue: renderTotpQr },
    provideAuthLabels(FR_AUTH_LABELS),
  ];
}
