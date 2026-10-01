import { HttpErrorResponse } from '@angular/common/http';
import { classifyActivateFailure, classifyRegisterFailure } from './account-errors';

const http = (status: number, error?: string) => new HttpErrorResponse({ status, error: error === undefined ? null : { error } });

describe('classifyRegisterFailure', () => {
  it('reads the "registration is disabled" 400 as a closed instance', () => {
    expect(classifyRegisterFailure(http(400, 'registration is disabled'))).toBe('disabled');
  });

  it.each([
    ["username may only contain letters, digits, '-' and '_'", 'username-invalid'],
    ['username must be 3 to 32 characters', 'username-invalid'],
    ['username must start with a letter', 'username-invalid'],
    ['email is not a valid address', 'email-invalid'],
    ['password must be at least 8 characters', 'password-weak'],
  ])('reads the 400 "%s" as %s, so that the page can point at the field', (message, expected) => {
    expect(classifyRegisterFailure(http(400, message))).toBe(expected);
  });

  it('reads any other 400 as invalid fields', () => {
    expect(classifyRegisterFailure(http(400, 'something new'))).toBe('invalid');
  });

  it('reads an unreadable 400 as invalid fields too, never as a closed instance', () => {
    expect(classifyRegisterFailure(new HttpErrorResponse({ status: 400, error: '<html>' }))).toBe('invalid');
    expect(classifyRegisterFailure(http(400))).toBe('invalid');
  });

  it('tells a reserved name from other invalid fields', () => {
    expect(classifyRegisterFailure(http(400, 'username is reserved'))).toBe('username-reserved');
  });

  it('reads a taken username (or one colliding with a group) and a taken e-mail as conflicts, per field', () => {
    expect(classifyRegisterFailure(http(409, 'username already taken'))).toBe('username-taken');
    expect(classifyRegisterFailure(http(409, 'username collides with an existing root group'))).toBe('username-taken');
    expect(classifyRegisterFailure(http(409, 'email already in use'))).toBe('email-taken');
  });

  it('reads any other 409 as a taken username (the field that most often collides)', () => {
    expect(classifyRegisterFailure(http(409))).toBe('username-taken');
  });

  it('reads a 429 as the rate limiter', () => {
    expect(classifyRegisterFailure(http(429, 'too many attempts, try again later'))).toBe('rate-limited');
  });

  it.each([500, 502, 413, 0])('reads a %i as the generic failure', (status) => {
    expect(classifyRegisterFailure(http(status, 'internal error'))).toBe('other');
  });

  it('reads a non-HTTP error as the generic failure', () => {
    expect(classifyRegisterFailure(new Error('boom'))).toBe('other');
    expect(classifyRegisterFailure(null)).toBe('other');
  });
});

describe('classifyActivateFailure', () => {
  it('reads the unknown / expired / used token 400 as a dead link', () => {
    expect(classifyActivateFailure(http(400, 'invalid or expired invitation'))).toBe('invalid-link');
  });

  it('reads the weak-password 400 as a weak password (the link is still good)', () => {
    expect(classifyActivateFailure(http(400, 'password must be at least 8 characters'))).toBe('weak-password');
  });

  it('reads any other 400 as a dead link, the safe reading', () => {
    expect(classifyActivateFailure(http(400))).toBe('invalid-link');
  });

  it('reads a 429 as the rate limiter and the rest as the generic failure', () => {
    expect(classifyActivateFailure(http(429))).toBe('rate-limited');
    expect(classifyActivateFailure(http(500, 'internal error'))).toBe('other');
    expect(classifyActivateFailure(new Error('boom'))).toBe('other');
  });
});
