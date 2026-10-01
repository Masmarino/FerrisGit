import { HttpClient } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { catchError, map, Observable, of, tap } from 'rxjs';

/**
 * Whether the instance opens its public pages to visitors, from `GET /api/auth/config`. `null` means unknown (the
 * request failed): callers decide what is safe. Only a known answer is cached, so a failure is retried next time.
 */
@Injectable({ providedIn: 'root' })
export class PublicConfigService {
  private http = inject(HttpClient);
  private known: boolean | null = null;

  publicPagesEnabled(): Observable<boolean | null> {
    if (this.known !== null) {
      return of(this.known);
    }
    return this.http.get<{ publicPagesEnabled?: boolean }>('/api/auth/config').pipe(
      map((config) => config.publicPagesEnabled === true),
      tap((enabled) => (this.known = enabled)),
      catchError(() => of(null)),
    );
  }
}
