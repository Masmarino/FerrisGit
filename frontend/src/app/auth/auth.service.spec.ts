import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import type { LoginResponse } from '@masmarino/gabarit/auth';
import { AuthService } from './auth.service';

describe('AuthService', () => {
  beforeEach(() => {
    localStorage.clear();
    TestBed.configureTestingModule({ providers: [AuthService, provideHttpClient(), provideHttpClientTesting()] });
  });

  it('is not authenticated before login', () => {
    const service = TestBed.inject(AuthService);
    expect(service.isAuthenticated()).toBe(false);
  });

  it('is authenticated and stores the token after a successful login', () => {
    const service = TestBed.inject(AuthService);
    const http = TestBed.inject(HttpTestingController);

    service.login('admin', 'secret').subscribe();
    http.expectOne('/api/auth/login').flush({ token: 'jwt-token' });

    expect(service.isAuthenticated()).toBe(true);
    expect(service.token()).toBe('jwt-token');
    expect(localStorage.getItem('ferrisgit_token')).toBe('jwt-token');
  });

  it('setToken stores the token exactly like a login does', () => {
    const service = TestBed.inject(AuthService);

    service.setToken('fresh-jwt');

    expect(service.isAuthenticated()).toBe(true);
    expect(service.token()).toBe('fresh-jwt');
    expect(localStorage.getItem('ferrisgit_token')).toBe('fresh-jwt');
  });

  it('setToken replaces the token of a session already in progress', () => {
    const service = TestBed.inject(AuthService);
    const http = TestBed.inject(HttpTestingController);
    service.login('admin', 'secret').subscribe();
    http.expectOne('/api/auth/login').flush({ token: 'jwt-token' });

    service.setToken('fresh-jwt');

    expect(service.token()).toBe('fresh-jwt');
    expect(localStorage.getItem('ferrisgit_token')).toBe('fresh-jwt');
  });

  it('clears the token on logout', () => {
    const service = TestBed.inject(AuthService);
    const http = TestBed.inject(HttpTestingController);
    service.login('admin', 'secret').subscribe();
    http.expectOne('/api/auth/login').flush({ token: 'jwt-token' });

    service.logout();

    expect(service.isAuthenticated()).toBe(false);
    expect(localStorage.getItem('ferrisgit_token')).toBeNull();
  });
  describe('MFA', () => {
    it('login keeps the token unset when the server asks for an MFA step, and hands the response back', () => {
      const service = TestBed.inject(AuthService);
      const http = TestBed.inject(HttpTestingController);
      let response: LoginResponse | undefined;

      service.login('admin', 'secret').subscribe((res) => (response = res));
      http.expectOne('/api/auth/login').flush({ token: null, mfaToken: 'pending', mfaSetupRequired: true, mfaHasTotp: false, mfaHasPasskey: false });

      expect(response?.mfaToken).toBe('pending');
      expect(response?.mfaSetupRequired).toBe(true);
      expect(service.isAuthenticated()).toBe(false);
      expect(service.token()).toBeNull();
      expect(localStorage.getItem('ferrisgit_token')).toBeNull();
    });

    it('verifyMfa posts the mfaToken and a TOTP code, then stores the session token', () => {
      const service = TestBed.inject(AuthService);
      const http = TestBed.inject(HttpTestingController);
      let done = false;

      service.verifyMfa('pending', { code: '123456' }).subscribe(() => (done = true));
      const request = http.expectOne('/api/auth/mfa/verify');
      expect(request.request.method).toBe('POST');
      expect(request.request.body).toEqual({ mfaToken: 'pending', code: '123456' });
      request.flush({ token: 'session-jwt' });

      expect(done).toBe(true);
      expect(service.token()).toBe('session-jwt');
      expect(localStorage.getItem('ferrisgit_token')).toBe('session-jwt');
    });

    it('verifyMfa posts a backup code as backupCode', () => {
      const service = TestBed.inject(AuthService);
      const http = TestBed.inject(HttpTestingController);

      service.verifyMfa('pending', { backupCode: 'abcd' }).subscribe();
      const request = http.expectOne('/api/auth/mfa/verify');
      expect(request.request.body).toEqual({ mfaToken: 'pending', backupCode: 'abcd' });
      request.flush({ token: 'session-jwt' });

      expect(service.token()).toBe('session-jwt');
    });

    it('verifyMfa leaves the token unset when the code is refused', () => {
      const service = TestBed.inject(AuthService);
      const http = TestBed.inject(HttpTestingController);
      let failed = false;

      service.verifyMfa('pending', { code: '000000' }).subscribe({ error: () => (failed = true) });
      http.expectOne('/api/auth/mfa/verify').flush({}, { status: 401, statusText: 'Unauthorized' });

      expect(failed).toBe(true);
      expect(service.isAuthenticated()).toBe(false);
    });

    it('enrollTotp posts the mfaToken and returns the secret and the otpauth URL', () => {
      const service = TestBed.inject(AuthService);
      const http = TestBed.inject(HttpTestingController);
      let result: unknown;

      service.enrollTotp('pending').subscribe((res) => (result = res));
      const request = http.expectOne('/api/auth/mfa/setup/totp/enroll');
      expect(request.request.method).toBe('POST');
      expect(request.request.body).toEqual({ mfaToken: 'pending' });
      request.flush({ secret: 'JBSWY3DP', otpauthUrl: 'otpauth://totp/FerrisGit:admin?secret=JBSWY3DP' });

      expect(result).toEqual({ secret: 'JBSWY3DP', otpauthUrl: 'otpauth://totp/FerrisGit:admin?secret=JBSWY3DP' });
      expect(service.isAuthenticated()).toBe(false);
    });

    it('confirmTotp posts the code and hands the session token back WITHOUT storing it', () => {
      const service = TestBed.inject(AuthService);
      const http = TestBed.inject(HttpTestingController);
      let result: unknown;

      service.confirmTotp('pending', '123456').subscribe((res) => (result = res));
      const request = http.expectOne('/api/auth/mfa/setup/totp/confirm');
      expect(request.request.method).toBe('POST');
      expect(request.request.body).toEqual({ mfaToken: 'pending', code: '123456' });
      request.flush({ token: 'session-jwt', backupCodes: ['a', 'b'] });

      expect(result).toEqual({ token: 'session-jwt', backupCodes: ['a', 'b'] });
      expect(service.isAuthenticated()).toBe(false);
      expect(localStorage.getItem('ferrisgit_token')).toBeNull();
    });
  });
  describe('passkeys', () => {
    const CREDENTIAL = { id: 'cred', rawId: 'cmF3', type: 'public-key', response: { authenticatorData: 'YQ', clientDataJSON: 'Yg', signature: 'Yw', userHandle: null } };
    const CHALLENGE = { challengeId: '7b1f7f2e-0000-4000-8000-000000000001', publicKey: { challenge: 'AQID', rpId: 'localhost' } };

    it('startPasskeyChallenge posts the mfaToken and hands back the challenge id and the options', () => {
      const service = TestBed.inject(AuthService);
      const http = TestBed.inject(HttpTestingController);
      let result: unknown;

      service.startPasskeyChallenge('pending').subscribe((res) => (result = res));
      const request = http.expectOne('/api/auth/mfa/passkey/start');
      expect(request.request.method).toBe('POST');
      expect(request.request.body).toEqual({ mfaToken: 'pending' });
      request.flush(CHALLENGE);

      expect(result).toEqual(CHALLENGE);
      expect(service.isAuthenticated()).toBe(false);
    });

    it('finishPasskeyChallenge posts the assertion and stores the session token, handing back nothing', () => {
      const service = TestBed.inject(AuthService);
      const http = TestBed.inject(HttpTestingController);
      let done = false;
      let result: unknown = 'unset';

      service.finishPasskeyChallenge('pending', CHALLENGE.challengeId, CREDENTIAL).subscribe((res) => {
        done = true;
        result = res;
      });
      const request = http.expectOne('/api/auth/mfa/passkey/finish');
      expect(request.request.method).toBe('POST');
      expect(request.request.body).toEqual({ mfaToken: 'pending', challengeId: CHALLENGE.challengeId, credential: CREDENTIAL });
      request.flush({ token: 'session-jwt' });

      expect(done).toBe(true);
      expect(result).toBeUndefined();
      expect(service.token()).toBe('session-jwt');
      expect(localStorage.getItem('ferrisgit_token')).toBe('session-jwt');
    });

    it('finishPasskeyChallenge leaves the token unset when the assertion is refused', () => {
      const service = TestBed.inject(AuthService);
      const http = TestBed.inject(HttpTestingController);
      let failed = false;

      service.finishPasskeyChallenge('pending', CHALLENGE.challengeId, CREDENTIAL).subscribe({ error: () => (failed = true) });
      http.expectOne('/api/auth/mfa/passkey/finish').flush({ error: 'invalid code' }, { status: 401, statusText: 'Unauthorized' });

      expect(failed).toBe(true);
      expect(service.isAuthenticated()).toBe(false);
      expect(localStorage.getItem('ferrisgit_token')).toBeNull();
    });

    it('startPasskeySetup posts the mfaToken to the setup route and hands back the challenge id and the options', () => {
      const service = TestBed.inject(AuthService);
      const http = TestBed.inject(HttpTestingController);
      let result: unknown;

      service.startPasskeySetup('pending').subscribe((res) => (result = res));
      const request = http.expectOne('/api/auth/mfa/setup/passkey/start');
      expect(request.request.method).toBe('POST');
      expect(request.request.body).toEqual({ mfaToken: 'pending' });
      request.flush(CHALLENGE);

      expect(result).toEqual(CHALLENGE);
      expect(service.isAuthenticated()).toBe(false);
    });

    it('finishPasskeySetup posts the attestation and the name, and hands the session token back WITHOUT storing it', () => {
      const service = TestBed.inject(AuthService);
      const http = TestBed.inject(HttpTestingController);
      let result: unknown;

      service.finishPasskeySetup('pending', CHALLENGE.challengeId, CREDENTIAL, 'MacBook').subscribe((res) => (result = res));
      const request = http.expectOne('/api/auth/mfa/setup/passkey/finish');
      expect(request.request.method).toBe('POST');
      expect(request.request.body).toEqual({ mfaToken: 'pending', challengeId: CHALLENGE.challengeId, credential: CREDENTIAL, name: 'MacBook' });
      request.flush({ token: 'session-jwt', backupCodes: ['a', 'b'] });

      expect(result).toEqual({ token: 'session-jwt', backupCodes: ['a', 'b'] });
      expect(service.isAuthenticated()).toBe(false);
      expect(localStorage.getItem('ferrisgit_token')).toBeNull();
    });
  });
  describe('account creation', () => {
    it('authConfig reads whether registration is open and whether passkeys are available, without sending a token', () => {
      const service = TestBed.inject(AuthService);
      const http = TestBed.inject(HttpTestingController);
      let result: unknown;

      service.authConfig().subscribe((res) => (result = res));
      const request = http.expectOne('/api/auth/config');
      expect(request.request.method).toBe('GET');
      request.flush({ registrationEnabled: true, passkeysAvailable: false });

      expect(result).toEqual({ registrationEnabled: true, passkeysAvailable: false });
    });

    it('register posts the username and address only, completes, and issues no session', () => {
      const service = TestBed.inject(AuthService);
      const http = TestBed.inject(HttpTestingController);
      let done = false;

      service.register('alice', 'alice@example.com').subscribe(() => (done = true));
      const request = http.expectOne('/api/auth/register');
      expect(request.request.method).toBe('POST');
      expect(request.request.body).toEqual({ username: 'alice', email: 'alice@example.com' });
      request.flush(null, { status: 204, statusText: 'No Content' });

      expect(done).toBe(true);
      expect(service.isAuthenticated()).toBe(false);
      expect(localStorage.getItem('ferrisgit_token')).toBeNull();
    });

    it('register fails and leaves the token unset when the server refuses', () => {
      const service = TestBed.inject(AuthService);
      const http = TestBed.inject(HttpTestingController);
      let failed = false;

      service.register('alice', 'alice@example.com').subscribe({ error: () => (failed = true) });
      http.expectOne('/api/auth/register').flush({ error: 'username already taken' }, { status: 409, statusText: 'Conflict' });

      expect(failed).toBe(true);
      expect(service.isAuthenticated()).toBe(false);
    });

    it('activate posts the token and the new password, completes, and issues no session', () => {
      const service = TestBed.inject(AuthService);
      const http = TestBed.inject(HttpTestingController);
      let done = false;

      service.activate('a'.repeat(64), 'a-long-password').subscribe(() => (done = true));
      const request = http.expectOne('/api/auth/activate');
      expect(request.request.method).toBe('POST');
      expect(request.request.body).toEqual({ token: 'a'.repeat(64), password: 'a-long-password' });
      request.flush(null, { status: 204, statusText: 'No Content' });

      expect(done).toBe(true);
      expect(service.isAuthenticated()).toBe(false);
    });

    it('activate fails on a dead link', () => {
      const service = TestBed.inject(AuthService);
      const http = TestBed.inject(HttpTestingController);
      let failed = false;

      service.activate('dead', 'a-long-password').subscribe({ error: () => (failed = true) });
      http.expectOne('/api/auth/activate').flush({ error: 'invalid or expired invitation' }, { status: 400, statusText: 'Bad Request' });

      expect(failed).toBe(true);
    });

    it('resetPassword posts the token and the new password, completes, and issues no session', () => {
      const service = TestBed.inject(AuthService);
      const http = TestBed.inject(HttpTestingController);
      let done = false;

      service.resetPassword('a'.repeat(64), 'a-long-password').subscribe(() => (done = true));
      const request = http.expectOne('/api/auth/reset-password');
      expect(request.request.method).toBe('POST');
      expect(request.request.body).toEqual({ token: 'a'.repeat(64), password: 'a-long-password' });
      request.flush(null, { status: 204, statusText: 'No Content' });

      expect(done).toBe(true);
      expect(service.isAuthenticated()).toBe(false);
    });

    it('resetPassword fails on a dead link', () => {
      const service = TestBed.inject(AuthService);
      const http = TestBed.inject(HttpTestingController);
      let failed = false;

      service.resetPassword('dead', 'a-long-password').subscribe({ error: () => (failed = true) });
      http.expectOne('/api/auth/reset-password').flush({ error: 'invalid or expired password reset link' }, { status: 400, statusText: 'Bad Request' });

      expect(failed).toBe(true);
    });
  });
});
