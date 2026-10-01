import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { AdminUser, AdminUserRepository, AdminUsersService, InviteResult, PasswordResetResult } from './admin-users.service';

const USER: AdminUser = {
  id: 'u2',
  username: 'bob',
  email: 'bob@example.com',
  isAdmin: false,
  createdAt: '2026-09-20T10:00:00Z',
  state: 'invited',
  invitationExpiresAt: '2026-09-26T10:00:00Z',
  mfaEnabled: false,
};

describe('AdminUsersService', () => {
  function setup() {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting(), AdminUsersService] });
    return { service: TestBed.inject(AdminUsersService), http: TestBed.inject(HttpTestingController) };
  }

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  it('lists the users', () => {
    const { service, http } = setup();
    let result: unknown;
    service.list().subscribe((users) => (result = users));
    http.expectOne({ url: '/api/admin/users', method: 'GET' }).flush([USER]);
    expect(result).toEqual([USER]);
  });

  it('invites a user: username, e-mail and the administrator flag in the body', () => {
    const { service, http } = setup();
    let result: InviteResult | undefined;
    service.invite('bob', 'bob@example.com', true).subscribe((r) => (result = r));
    const req = http.expectOne({ url: '/api/admin/users/invite', method: 'POST' });
    expect(req.request.body).toEqual({ username: 'bob', email: 'bob@example.com', isAdmin: true });
    req.flush({ user: USER, emailSent: true });
    expect(result).toEqual({ user: USER, emailSent: true });
    expect(result && 'activationUrl' in result).toBe(false);
  });

  it('reads the link and the reason the mail was not sent', () => {
    const { service, http } = setup();
    let result: InviteResult | undefined;
    service.invite('bob', 'bob@example.com', false).subscribe((r) => (result = r));
    http.expectOne('/api/admin/users/invite').flush({ user: USER, emailSent: false, emailError: 'connection refused', activationUrl: 'http://localhost:4200/activate#token=abc' });
    expect(result?.emailSent).toBe(false);
    expect(result?.emailError).toBe('connection refused');
    expect(result?.activationUrl).toBe('http://localhost:4200/activate#token=abc');
  });

  it('resends an invitation to the user id (empty body)', () => {
    const { service, http } = setup();
    let result: InviteResult | undefined;
    service.resend('u2').subscribe((r) => (result = r));
    const req = http.expectOne({ url: '/api/admin/users/u2/invitation', method: 'POST' });
    expect(req.request.body).toEqual({});
    req.flush({ user: USER, emailSent: true });
    expect(result?.user.id).toBe('u2');
  });

  it('resets the double authentication of a user (204)', () => {
    const { service, http } = setup();
    let completed = false;
    service.resetMfa('u2').subscribe({ complete: () => (completed = true) });
    http.expectOne({ url: '/api/admin/users/u2/mfa', method: 'DELETE' }).flush(null, { status: 204, statusText: 'No Content' });
    expect(completed).toBe(true);
  });

  it('resets the password of a user (empty body) and reads a sent mail', () => {
    const { service, http } = setup();
    let result: PasswordResetResult | undefined;
    service.resetPassword('u2').subscribe((r) => (result = r));
    const req = http.expectOne({ url: '/api/admin/users/u2/reset-password', method: 'POST' });
    expect(req.request.body).toEqual({});
    req.flush({ emailSent: true });
    expect(result).toEqual({ emailSent: true });
  });

  it('reads the reset link and the reason the mail was not sent', () => {
    const { service, http } = setup();
    let result: PasswordResetResult | undefined;
    service.resetPassword('u2').subscribe((r) => (result = r));
    http
      .expectOne('/api/admin/users/u2/reset-password')
      .flush({ emailSent: false, emailError: 'the user has no deliverable e-mail address', resetUrl: 'http://localhost:4200/reset-password#token=abc' });
    expect(result).toEqual({ emailSent: false, emailError: 'the user has no deliverable e-mail address', resetUrl: 'http://localhost:4200/reset-password#token=abc' });
  });

  it.each([true, false])('sets the administrator flag to %s (204)', (isAdmin) => {
    const { service, http } = setup();
    let completed = false;
    service.setAdmin('u2', isAdmin).subscribe({ complete: () => (completed = true) });
    const req = http.expectOne({ url: '/api/admin/users/u2/admin', method: 'PUT' });
    expect(req.request.body).toEqual({ isAdmin });
    req.flush(null, { status: 204, statusText: 'No Content' });
    expect(completed).toBe(true);
  });

  it('encodes the user id in the URLs', () => {
    const { service, http } = setup();
    service.resend('a/b').subscribe();
    http.expectOne('/api/admin/users/a%2Fb/invitation').flush({ user: USER, emailSent: true });
    service.resetMfa('a/b').subscribe();
    http.expectOne('/api/admin/users/a%2Fb/mfa').flush(null, { status: 204, statusText: 'No Content' });
    service.resetPassword('a/b').subscribe();
    http.expectOne('/api/admin/users/a%2Fb/reset-password').flush({ emailSent: true });
    service.setAdmin('a/b', true).subscribe();
    http.expectOne('/api/admin/users/a%2Fb/admin').flush(null, { status: 204, statusText: 'No Content' });
    service.repositories('a/b').subscribe();
    http.expectOne('/api/admin/users/a%2Fb/repositories').flush([]);
    service.deleteUser('a/b').subscribe();
    http.expectOne({ url: '/api/admin/users/a%2Fb', method: 'DELETE' }).flush(null, { status: 204, statusText: 'No Content' });
  });

  it("lists a user's personal repositories, a size that could not be computed included (null)", () => {
    const { service, http } = setup();
    const repositories: AdminUserRepository[] = [
      { id: 'r1', name: 'widget', description: 'A widget', visibility: 'private', createdAt: '2026-09-20T10:00:00Z', sizeBytes: 2048 },
      { id: 'r2', name: 'gone', description: '', visibility: 'public', createdAt: '2026-09-19T10:00:00Z', sizeBytes: null },
    ];
    let result: AdminUserRepository[] | undefined;
    service.repositories('u2').subscribe((r) => (result = r));
    http.expectOne({ url: '/api/admin/users/u2/repositories', method: 'GET' }).flush(repositories);
    expect(result).toEqual(repositories);
  });

  it('deletes a user (204)', () => {
    const { service, http } = setup();
    let completed = false;
    service.deleteUser('u2').subscribe({ complete: () => (completed = true) });
    const req = http.expectOne({ url: '/api/admin/users/u2', method: 'DELETE' });
    expect(req.request.body).toBeNull();
    req.flush(null, { status: 204, statusText: 'No Content' });
    expect(completed).toBe(true);
  });
});
