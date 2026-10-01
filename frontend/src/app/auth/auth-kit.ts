import type { Provider } from '@angular/core';
import { AUTH_PORT, MFA_PORT, TOTP_QR_RENDERER, provideAuthLabels } from '@masmarino/gabarit';
import { MfaService } from '../account/mfa.service';
import { AuthService } from './auth.service';
import { FR_AUTH_LABELS } from './auth-labels.fr';
import { renderTotpQr } from './totp-qr-renderer';

/**
 * Connects Gabarit's auth kit to FerrisGit once for the whole app (`app.config.ts`). Its ports are FerrisGit's own
 * services, its QR codes are drawn locally and its strings are French. These are plain `Provider`s, not environment
 * providers, so a spec or story can list them next to fakes of `AuthService` / `MfaService`.
 */
export function provideFerrisgitAuth(): Provider[] {
  return [
    { provide: AUTH_PORT, useExisting: AuthService },
    { provide: MFA_PORT, useExisting: MfaService },
    { provide: TOTP_QR_RENDERER, useValue: renderTotpQr },
    provideAuthLabels(FR_AUTH_LABELS),
  ];
}
