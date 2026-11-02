import { HttpErrorResponse, HttpInterceptorFn } from '@angular/common/http';
import { inject } from '@angular/core';
import { Router } from '@angular/router';
import { catchError, throwError } from 'rxjs';
import { AuthService } from './auth.service';
import { loginUrl } from './login-link';

export const authInterceptor: HttpInterceptorFn = (req, next) => {
  const auth = inject(AuthService);
  const router = inject(Router);
  const token = auth.token();
  const authedReq = token ? req.clone({ setHeaders: { Authorization: `Bearer ${token}` } }) : req;

  return next(authedReq).pipe(
    catchError((error: unknown) => {
      // Only drop the token this request was sent with: a late 401 from before a password change shouldn't log out the
      // fresh session.
      if (error instanceof HttpErrorResponse && error.status === 401 && auth.token() === token) {
        auth.logout();
        // An expired session comes back to this page once signed in. On the sign-in page itself a wrong password answers
        // 401: stay, or its own `returnUrl` is lost.
        if (!router.url.startsWith('/login')) {
          router.navigateByUrl(loginUrl(router.url));
        }
      }
      return throwError(() => error);
    }),
  );
};
