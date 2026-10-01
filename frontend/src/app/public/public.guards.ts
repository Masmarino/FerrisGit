import { inject } from '@angular/core';
import { CanActivateFn, CanMatchFn, Router, UrlMatcher, UrlSegment } from '@angular/router';
import { map } from 'rxjs';
import { AuthService } from '../auth/auth.service';
import { PublicConfigService } from './public-config.service';

/** Matches only without a session, so a signed-in user falls through to the authenticated routes at the same URL. */
export const anonymousGuard: CanMatchFn = () => !inject(AuthService).isAuthenticated();

/** The anonymous home page is the catalog, unless the instance closed its public pages (or cannot tell): then the sign-in page. */
export const publicHomeGuard: CanActivateFn = () => {
  const router = inject(Router);
  return inject(PublicConfigService)
    .publicPagesEnabled()
    .pipe(map((enabled) => (enabled === true ? true : router.parseUrl('/login'))));
};

/** `/repositories/<path…>` with at least one segment after the prefix. The bare `/repositories` is the signed-in workspace. */
export const publicRepositoryMatcher: UrlMatcher = (segments: UrlSegment[]) =>
  segments.length >= 2 && segments[0].path === 'repositories' ? { consumed: segments } : null;
