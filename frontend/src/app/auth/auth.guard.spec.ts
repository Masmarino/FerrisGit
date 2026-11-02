import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting } from '@angular/common/http/testing';
import { provideRouter, UrlTree } from '@angular/router';
import { authGuard } from './auth.guard';

const TOKEN_KEY = 'ferrisgit_token';

describe('authGuard', () => {
  afterEach(() => {
    localStorage.removeItem(TOKEN_KEY);
  });

  it('allows navigation when a token is present', () => {
    localStorage.setItem(TOKEN_KEY, 'a-token');
    TestBed.configureTestingModule({
      providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([])],
    });

    const result = TestBed.runInInjectionContext(() => authGuard({} as never, { url: '/home' } as never));

    expect(result).toBe(true);
  });

  it('redirects to /login when no token is present, coming back to the page afterwards', () => {
    localStorage.removeItem(TOKEN_KEY);
    TestBed.configureTestingModule({
      providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([])],
    });

    const result = TestBed.runInInjectionContext(() => authGuard({} as never, { url: '/home' } as never));

    expect(result).toBeInstanceOf(UrlTree);
    expect((result as UrlTree).toString()).toBe('/login?returnUrl=%2Fhome');
  });

  it('keeps the query string in the way back: a terminal\'s link holds its code there', () => {
    localStorage.removeItem(TOKEN_KEY);
    TestBed.configureTestingModule({
      providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([])],
    });

    const result = TestBed.runInInjectionContext(() => authGuard({} as never, { url: '/account/device?code=ABCD-EFGH' } as never)) as UrlTree;

    expect(result.queryParams).toEqual({ returnUrl: '/account/device?code=ABCD-EFGH' });
  });
});
