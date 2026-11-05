import { loginLink, loginUrl, safeReturnUrl } from './login-link';

describe('loginLink', () => {
  it('comes back to the current page after signing in', () => {
    expect(loginLink('/repositories/alice/hello/-/releases')).toEqual({ commands: ['/login'], queryParams: { returnUrl: '/repositories/alice/hello/-/releases' } });
  });

  it('needs no return from the site root or the sign-in page itself', () => {
    expect(loginLink('/').queryParams).toBeNull();
    expect(loginLink('/login?returnUrl=%2Fx').queryParams).toBeNull();
  });
});

describe('loginUrl', () => {
  it('writes the way back as one URL, its query encoded', () => {
    expect(loginUrl('/account/device?code=ABCD-EFGH')).toBe('/login?returnUrl=%2Faccount%2Fdevice%3Fcode%3DABCD-EFGH');
  });

  it('is the bare sign-in page when there is no way back', () => {
    expect(loginUrl('/')).toBe('/login');
  });

  it('gives what safeReturnUrl follows back', () => {
    const back = new URL(loginUrl('/account/device?code=ABCD-EFGH'), 'https://x.example').searchParams.get('returnUrl');

    expect(safeReturnUrl(back)).toBe('/account/device?code=ABCD-EFGH');
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
