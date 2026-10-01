import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting } from '@angular/common/http/testing';
import { provideRouter, UrlSegment, UrlTree } from '@angular/router';
import { firstValueFrom, Observable, of } from 'rxjs';
import { anonymousGuard, publicHomeGuard, publicRepositoryMatcher } from './public.guards';
import { PublicConfigService } from './public-config.service';

const TOKEN_KEY = 'ferrisgit_token';

describe('public guards', () => {
  afterEach(() => localStorage.removeItem(TOKEN_KEY));

  function configure(publicPagesEnabled: boolean | null = true) {
    TestBed.configureTestingModule({
      providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([]), { provide: PublicConfigService, useValue: { publicPagesEnabled: () => of(publicPagesEnabled) } }],
    });
  }

  describe('anonymousGuard', () => {
    it('matches without a session', () => {
      configure();
      expect(TestBed.runInInjectionContext(() => anonymousGuard({} as never, [], {} as never))).toBe(true);
    });

    it('does not match with a session', () => {
      localStorage.setItem(TOKEN_KEY, 'a-token');
      configure();
      expect(TestBed.runInInjectionContext(() => anonymousGuard({} as never, [], {} as never))).toBe(false);
    });
  });

  describe('publicHomeGuard', () => {
    const run = () => firstValueFrom(TestBed.runInInjectionContext(() => publicHomeGuard({} as never, {} as never)) as Observable<boolean | UrlTree>);

    it('lets the catalog show when the public pages are on', async () => {
      configure(true);
      expect(await run()).toBe(true);
    });

    it.each([false, null])('sends to the sign-in page when they are off or unknown (%s)', async (enabled) => {
      configure(enabled);
      const result = await run();
      expect(result).toBeInstanceOf(UrlTree);
      expect(String(result)).toBe('/login');
    });
  });

  describe('publicRepositoryMatcher', () => {
    const segments = (...paths: string[]) => paths.map((path) => new UrlSegment(path, {}));

    it('consumes a repository path with everything after it', () => {
      const url = segments('repositories', 'alice', 'hello', '-', 'blob', 'main', 'README.md');
      expect(publicRepositoryMatcher(url, {} as never, {} as never)).toEqual({ consumed: url });
    });

    it('leaves the bare /repositories (the signed-in workspace) and other prefixes alone', () => {
      expect(publicRepositoryMatcher(segments('repositories'), {} as never, {} as never)).toBeNull();
      expect(publicRepositoryMatcher(segments('groups', 'acme'), {} as never, {} as never)).toBeNull();
    });
  });
});
