import { loginLink, safeReturnUrl } from './login-link';

describe('loginLink', () => {
  it('comes back to the current page after signing in', () => {
    expect(loginLink('/repositories/alice/hello/-/releases')).toEqual({ commands: ['/login'], queryParams: { returnUrl: '/repositories/alice/hello/-/releases' } });
  });

  it('needs no return from the site root or the sign-in page itself', () => {
    expect(loginLink('/').queryParams).toBeNull();
    expect(loginLink('/login?returnUrl=%2Fx').queryParams).toBeNull();
  });
});

describe('safeReturnUrl', () => {
  it('keeps a path on this site, with its query', () => {
    expect(safeReturnUrl('/explore?q=rust&page=2')).toBe('/explore?q=rust&page=2');
  });

  it.each([null, '', 'https://evil.example', '//evil.example', '/\\evil.example', 'javascript:alert(1)', 'repositories', '/login'])('drops %s', (value) => {
    expect(safeReturnUrl(value)).toBeNull();
  });
});
