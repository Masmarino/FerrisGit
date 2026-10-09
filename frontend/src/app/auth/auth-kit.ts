import type { Provider } from '@angular/core';
import { AUTH_LABELS, AUTH_PORT, MFA_PORT } from '@masmarino/gabarit/auth';
import { TOTP_QR_RENDERER } from '@masmarino/gabarit/mfa-enrollment';
import { MfaService } from './mfa.service';
import { AuthService } from './auth.service';
import { activeLanguage } from '../shared/i18n/translator';
import { kitLabelsFor } from './kit-labels';
import { renderTotpQr } from './totp-qr-renderer';

/**
 * Wires Gabarit's auth kit to our services, a local QR renderer and the strings of the active language. Each
 * component that uses the kit lists these in its own `providers`, not the app config: the kit then loads with those
 * lazy pages, out of the first chunk. Plain providers rather than environment providers, so a spec or story can list them next to fakes of
 * the two services.
 */
export function provideFerrisgitAuth(): Provider[] {
  return [
    { provide: AUTH_PORT, useExisting: AuthService },
    { provide: MFA_PORT, useExisting: MfaService },
    { provide: TOTP_QR_RENDERER, useValue: renderTotpQr },
    { provide: AUTH_LABELS, useFactory: () => kitLabelsFor(activeLanguage()) },
  ];
}
