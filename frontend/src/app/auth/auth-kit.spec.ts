import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { AUTH_LABELS, AUTH_PORT, MFA_PORT } from '@masmarino/gabarit/auth';
import { TOTP_QR_RENDERER } from '@masmarino/gabarit/mfa-enrollment';
import { appConfig } from '../app.config';
import { MfaService } from './mfa.service';
import { AuthService } from './auth.service';
import { provideFerrisgitAuth } from './auth-kit';
import { FR_AUTH_LABELS } from './auth-labels.fr';
import { renderTotpQr } from './totp-qr-renderer';

describe('provideFerrisgitAuth', () => {
  it("makes FerrisGit's own services the kit's ports, the same instances the rest of the app uses", () => {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideFerrisgitAuth()] });

    expect(TestBed.inject(AUTH_PORT)).toBe(TestBed.inject(AuthService));
    expect(TestBed.inject(MFA_PORT)).toBe(TestBed.inject(MfaService));
  });

  it('follows a replaced AuthService / MfaService (a fake in a spec or a story)', () => {
    const auth = { logout: () => undefined };
    const mfa = { status: () => undefined };
    TestBed.configureTestingModule({ providers: [provideFerrisgitAuth(), { provide: AuthService, useValue: auth }, { provide: MfaService, useValue: mfa }] });

    expect(TestBed.inject(AUTH_PORT)).toBe(auth);
    expect(TestBed.inject(MFA_PORT)).toBe(mfa);
  });

  it('draws the QR codes locally and speaks French', () => {
    TestBed.configureTestingModule({ providers: [provideFerrisgitAuth()] });

    expect(TestBed.inject(TOTP_QR_RENDERER)).toBe(renderTotpQr);
    expect(TestBed.inject(AUTH_LABELS)).toBe(FR_AUTH_LABELS);
  });

  it('stays out of the root config: the pages that use the kit provide it, so it loads with them', () => {
    TestBed.configureTestingModule({ providers: appConfig.providers });

    expect(TestBed.inject(AUTH_PORT, null)).toBeNull();
    expect(TestBed.inject(MFA_PORT, null)).toBeNull();
    expect(TestBed.inject(TOTP_QR_RENDERER, null)).toBeNull();
    expect(TestBed.inject(AUTH_LABELS, null)).toBeNull();
  });
});
