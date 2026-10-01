import { TestBed } from '@angular/core/testing';
import { HttpErrorResponse, provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { MfaService } from './mfa.service';

const PASSKEY = { id: 'p1', name: 'MacBook', createdAt: '2026-09-01T10:00:00Z', lastUsedAt: null };

describe('MfaService', () => {
  function setup() {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting()] });
    return { service: TestBed.inject(MfaService), http: TestBed.inject(HttpTestingController) };
  }

  afterEach(() => TestBed.inject(HttpTestingController).verify());

  it('reads the status', () => {
    const { service, http } = setup();
    let result: unknown;

    service.status().subscribe((status) => (result = status));
    const request = http.expectOne('/api/me/mfa');

    expect(request.request.method).toBe('GET');
    request.flush({ totpEnabled: true, backupCodesRemaining: 7, passkeys: [PASSKEY] });
    expect(result).toEqual({ totpEnabled: true, backupCodesRemaining: 7, passkeys: [PASSKEY] });
  });

  it('asks for a new secret with the current password', () => {
    const { service, http } = setup();
    let result: unknown;

    service.enroll('s3cret').subscribe((res) => (result = res));
    const request = http.expectOne('/api/me/mfa/totp/enroll');

    expect(request.request.method).toBe('POST');
    expect(request.request.body).toEqual({ currentPassword: 's3cret' });
    request.flush({ secret: 'JBSWY3DPEHPK3PXP', otpauthUrl: 'otpauth://totp/x' });
    expect(result).toEqual({ secret: 'JBSWY3DPEHPK3PXP', otpauthUrl: 'otpauth://totp/x' });
  });

  it('confirms the enrolment with a code and returns the backup codes', () => {
    const { service, http } = setup();
    let result: unknown;

    service.confirm('123456').subscribe((res) => (result = res));
    const request = http.expectOne('/api/me/mfa/totp/confirm');

    expect(request.request.method).toBe('POST');
    expect(request.request.body).toEqual({ code: '123456' });
    request.flush({ backupCodes: ['a', 'b'] });
    expect(result).toEqual({ backupCodes: ['a', 'b'] });
  });

  it('regenerates the backup codes with the current password', () => {
    const { service, http } = setup();
    let result: unknown;

    service.regenerate('s3cret').subscribe((res) => (result = res));
    const request = http.expectOne('/api/me/mfa/backup-codes/regenerate');

    expect(request.request.method).toBe('POST');
    expect(request.request.body).toEqual({ currentPassword: 's3cret' });
    request.flush({ backupCodes: ['a'] });
    expect(result).toEqual({ backupCodes: ['a'] });
  });

  it('disables the factor with the current password: a 204 with no body and no token', () => {
    const { service, http } = setup();
    const results: unknown[] = [];
    let completed = false;

    service.disable('s3cret').subscribe({ next: (res) => results.push(res), complete: () => (completed = true) });
    const request = http.expectOne('/api/me/mfa/totp/disable');

    expect(request.request.method).toBe('POST');
    expect(request.request.body).toEqual({ currentPassword: 's3cret' });
    request.flush(null, { status: 204, statusText: 'No Content' });
    expect(results).toEqual([null]);
    expect(completed).toBe(true);
  });

  it('passes a refused call on untouched: the status and the body, which the cards read (a wrong password is a 400, never a 401)', () => {
    const { service, http } = setup();
    let error: unknown;

    service.regenerate('nope').subscribe({ error: (err) => (error = err) });
    http.expectOne('/api/me/mfa/backup-codes/regenerate').flush({ error: 'current password is incorrect' }, { status: 400, statusText: 'Bad Request' });

    expect(error).toBeInstanceOf(HttpErrorResponse);
    expect((error as HttpErrorResponse).status).toBe(400);
    expect((error as HttpErrorResponse).error).toEqual({ error: 'current password is incorrect' });
  });

  describe('passkeys', () => {
    it('starts a registration with the current password (the server checks it before any ceremony)', () => {
      const { service, http } = setup();
      let result: unknown;

      service.startPasskeyRegistration('s3cret').subscribe((res) => (result = res));
      const request = http.expectOne('/api/me/mfa/passkeys/register/start');

      expect(request.request.method).toBe('POST');
      expect(request.request.body).toEqual({ currentPassword: 's3cret' });
      request.flush({ challengeId: 'c1', publicKey: { challenge: 'AQ' } });
      expect(result).toEqual({ challengeId: 'c1', publicKey: { challenge: 'AQ' } });
    });

    it('finishes a registration with the challenge, the credential and the name, and answers the new passkey', () => {
      const { service, http } = setup();
      let result: unknown;
      const credential = { id: 'x', rawId: 'x', type: 'public-key', response: { attestationObject: 'a', clientDataJSON: 'b' } };

      service.finishPasskeyRegistration('c1', credential, 'MacBook').subscribe((res) => (result = res));
      const request = http.expectOne('/api/me/mfa/passkeys/register/finish');

      expect(request.request.method).toBe('POST');
      expect(request.request.body).toEqual({ challengeId: 'c1', credential, name: 'MacBook' });
      request.flush(PASSKEY, { status: 201, statusText: 'Created' });
      expect(result).toEqual(PASSKEY);
    });

    it('deletes a passkey with the current password: a 204 with no body (the session is revoked)', () => {
      const { service, http } = setup();
      const results: unknown[] = [];
      let completed = false;

      service.deletePasskey('p1', 's3cret').subscribe({ next: (res) => results.push(res), complete: () => (completed = true) });
      const request = http.expectOne('/api/me/mfa/passkeys/p1/delete');

      expect(request.request.method).toBe('POST');
      expect(request.request.body).toEqual({ currentPassword: 's3cret' });
      request.flush(null, { status: 204, statusText: 'No Content' });
      expect(results).toEqual([null]);
      expect(completed).toBe(true);
    });

    it('encodes the id in the path', () => {
      const { service, http } = setup();

      service.deletePasskey('a/b', 's3cret').subscribe();

      http.expectOne('/api/me/mfa/passkeys/a%2Fb/delete').flush(null, { status: 204, statusText: 'No Content' });
    });
  });
});
