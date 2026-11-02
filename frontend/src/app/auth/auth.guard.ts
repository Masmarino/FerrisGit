import { inject } from '@angular/core';
import { CanActivateFn, Router } from '@angular/router';
import { AuthService } from './auth.service';
import { loginUrl } from './login-link';

/** Signing in comes back to the page that was asked for, query string included (a terminal's link holds its code there). */
export const authGuard: CanActivateFn = (_route, state) => {
  const auth = inject(AuthService);
  const router = inject(Router);
  return auth.isAuthenticated() ? true : router.parseUrl(loginUrl(state.url));
};
