import { HttpClient } from '@angular/common/http';
import { computed, inject, Injectable, signal } from '@angular/core';
import type { AuthConfig, AuthPort, LoginResponse, MfaProof, MfaSetupResult, PasskeyChallenge, TotpEnrollment } from '@masmarino/gabarit/auth';
import { map, Observable, tap } from 'rxjs';

const TOKEN_KEY = 'ferrisgit_token';

/**
 * The auth kit's port. For a local account the password step returns no session (`token` is null, `mfaToken` names the
 * next MFA step), and neither does the first enrolment: the kit calls `setToken` once the backup codes are acknowledged.
 * Not `register`: the kit's version signs the new user in, ours mails a link instead (see RegisterPage).
 */
@Injectable({ providedIn: 'root' })
export class AuthService implements Omit<AuthPort, 'register'> {
  private http = inject(HttpClient);
  private tokenSignal = signal<string | null>(localStorage.getItem(TOKEN_KEY));
  readonly isAuthenticated = computed(() => this.tokenSignal() !== null);

  login(username: string, password: string): Observable<LoginResponse> {
    return this.http.post<LoginResponse>('/api/auth/login', { username, password }).pipe(tap((res) => this.storeSession(res)));
  }

  authConfig(): Observable<AuthConfig> {
    return this.http.get<AuthConfig>('/api/auth/config');
  }

  /** No session: the server mails a link to choose a password, and the account is unusable until then. */
  register(username: string, email: string): Observable<void> {
    return this.http.post<void>('/api/auth/register', { username, email });
  }

  /** `username` only from the invitation page: an invitee chooses it there, a registered account already has one. */
  activate(token: string, password: string, username?: string): Observable<void> {
    return this.http.post<void>('/api/auth/activate', username === undefined ? { token, password } : { token, password, username });
  }

  resetPassword(token: string, password: string): Observable<void> {
    return this.http.post<void>('/api/auth/reset-password', { token, password });
  }

  verifyMfa(mfaToken: string, proof: MfaProof): Observable<void> {
    return this.http.post<{ token: string }>('/api/auth/mfa/verify', { mfaToken, ...proof }).pipe(
      tap((res) => this.setToken(res.token)),
      map(() => undefined),
    );
  }

  startPasskeyChallenge(mfaToken: string): Observable<PasskeyChallenge> {
    return this.http.post<PasskeyChallenge>('/api/auth/mfa/passkey/start', { mfaToken });
  }

  finishPasskeyChallenge(mfaToken: string, challengeId: string, credential: unknown): Observable<void> {
    return this.http.post<{ token: string }>('/api/auth/mfa/passkey/finish', { mfaToken, challengeId, credential }).pipe(
      tap((res) => this.setToken(res.token)),
      map(() => undefined),
    );
  }

  startPasskeySetup(mfaToken: string): Observable<PasskeyChallenge> {
    return this.http.post<PasskeyChallenge>('/api/auth/mfa/setup/passkey/start', { mfaToken });
  }

  /** Doesn't store the session token, like `confirmTotp`; the caller does it once the backup codes are saved. */
  finishPasskeySetup(mfaToken: string, challengeId: string, credential: unknown, name: string): Observable<MfaSetupResult> {
    return this.http.post<MfaSetupResult>('/api/auth/mfa/setup/passkey/finish', { mfaToken, challengeId, credential, name });
  }

  enrollTotp(mfaToken: string): Observable<TotpEnrollment> {
    return this.http.post<TotpEnrollment>('/api/auth/mfa/setup/totp/enroll', { mfaToken });
  }

  /** Doesn't store the session token; the caller does it once the backup codes (shown only once) are saved. */
  confirmTotp(mfaToken: string, code: string): Observable<MfaSetupResult> {
    return this.http.post<MfaSetupResult>('/api/auth/mfa/setup/totp/confirm', { mfaToken, code });
  }

  setToken(token: string): void {
    localStorage.setItem(TOKEN_KEY, token);
    this.tokenSignal.set(token);
  }

  private storeSession(res: LoginResponse): void {
    if (res.token) {
      this.setToken(res.token);
    }
  }

  logout(): void {
    localStorage.removeItem(TOKEN_KEY);
    this.tokenSignal.set(null);
  }

  token(): string | null {
    return this.tokenSignal();
  }
}
