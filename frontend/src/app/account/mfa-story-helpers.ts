// Storybook-only fake server behind the account security stories: password `mot-de-passe-correct`, authenticator code
// `123456`. Calls answer after a short delay so the spinners show.
import { HttpErrorResponse } from '@angular/common/http';
import { MfaSettingsState, type MfaPort, type MfaStatus, type Passkey } from '@masmarino/gabarit/auth';
import { moduleMetadata } from '@storybook/angular-vite';
import { NEVER, Observable, of, switchMap, throwError, timer } from 'rxjs';
import { provideFerrisgitAuth } from '../auth/auth-kit';
import { MfaService } from '../auth/mfa.service';

export const SECRET = 'JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP';
export const OTPAUTH_URL = `otpauth://totp/FerrisGit:florian.simon?secret=${SECRET}&issuer=FerrisGit`;
export const BACKUP_CODES = [
  '3f9a1c07b5d24e8a9c60d1b7e2f4a583',
  '8b2d5e91c47f3a06d9e1b5c8a2f70d34',
  'c40e7a15f9b3d862e1a4c7095bd3f2e6',
  '1d6f3b8a20c9e547f4b1d3a68e05c92b',
  'a7e19c4d5b0f2836e9d1a4c7b3f5082e',
  '5c8b2f6a1d9e4073b8a5c2e1f9d47a06',
  'e93a0d7c4b1f5862a9e3d0c7b5f14a28',
  '2b7f4e1a9c5d0836f1b4e7a2c9d508e3',
  'd05a8c3e7b1f4926a0d5c8e3b7f1a294',
  '6e1c9b4a2f7d0538c1e6b9a4f2d70c85',
];

export const PASSWORD = 'mot-de-passe-correct';
export const CODE = '123456';

export const httpError = (status: number, error: string) => throwError(() => new HttpErrorResponse({ status, statusText: 'x', error: { error } }));
export const later = <T>(answer: () => Observable<T>): Observable<T> => timer(350).pipe(switchMap(answer));

export type StoryMfaStatus = MfaStatus | 'loading' | 'failed';

const CREATION_OPTIONS = {
  rp: { name: 'FerrisGit', id: 'localhost' },
  user: { id: 'CQoLDA', name: 'florian.simon', displayName: 'florian.simon' },
  challenge: 'AQIDBA',
  pubKeyCredParams: [{ type: 'public-key', alg: -7 }],
  timeout: 300000,
  excludeCredentials: [],
  authenticatorSelection: { residentKey: 'discouraged', userVerification: 'preferred' },
  attestation: 'none',
};

export function fakeMfaService(status: StoryMfaStatus): MfaPort {
  const checkPassword = <T>(password: string, ok: () => T) => later(() => (password === PASSWORD ? of(ok()) : httpError(400, 'current password is incorrect')));
  return {
    status: () => (status === 'loading' ? NEVER : status === 'failed' ? throwError(() => new HttpErrorResponse({ status: 500 })) : of(status)),
    enroll: (password) => checkPassword(password, () => ({ secret: SECRET, otpauthUrl: OTPAUTH_URL })),
    confirm: (code) => later(() => (code === CODE ? of({ backupCodes: BACKUP_CODES }) : httpError(400, 'invalid code'))),
    regenerate: (password) => checkPassword(password, () => ({ backupCodes: BACKUP_CODES })),
    disable: (password) => checkPassword(password, () => undefined),
    startPasskeyRegistration: (password) => checkPassword(password, () => ({ challengeId: 'challenge-1', publicKey: CREATION_OPTIONS })),
    finishPasskeyRegistration: (_challengeId, _credential, name) => later(() => of<Passkey>({ id: `new-${name}`, name, createdAt: new Date().toISOString(), lastUsedAt: null })),
    deletePasskey: (_id, password) => checkPassword(password, () => undefined),
  };
}

/** `appFlowActive`: the app card is mid-enrolment (the kit's shared `MfaSettingsState`), so the passkeys card hides "Ajouter". */
export const withMfa = (status: StoryMfaStatus, options: { appFlowActive?: boolean } = {}) =>
  moduleMetadata({
    providers: [
      provideFerrisgitAuth(),
      { provide: MfaService, useFactory: () => fakeMfaService(status) },
      {
        provide: MfaSettingsState,
        useFactory: () => {
          const state = new MfaSettingsState();
          state.setAppFlowActive(options.appFlowActive ?? false);
          return state;
        },
      },
    ],
  });
