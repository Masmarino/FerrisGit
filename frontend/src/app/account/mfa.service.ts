import { HttpClient } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import type { BackupCodesResult, MfaPort, MfaStatus, Passkey, PasskeyChallenge, TotpEnrollment } from '@masmarino/gabarit';
import { Observable } from 'rxjs';

/**
 * The `MfaPort` behind Gabarit's account security cards. When the password is wrong, password-protected calls answer
 * 400, not 401, which the auth interceptor would read as an expired session. `disable` and `deletePasskey` revoke the
 * caller's own session on the server. The cards then emit `sessionRevoked` and FerrisGit signs out.
 */
@Injectable({ providedIn: 'root' })
export class MfaService implements MfaPort {
  private http = inject(HttpClient);

  status(): Observable<MfaStatus> {
    return this.http.get<MfaStatus>('/api/me/mfa');
  }

  enroll(currentPassword: string): Observable<TotpEnrollment> {
    return this.http.post<TotpEnrollment>('/api/me/mfa/totp/enroll', { currentPassword });
  }

  confirm(code: string): Observable<BackupCodesResult> {
    return this.http.post<BackupCodesResult>('/api/me/mfa/totp/confirm', { code });
  }

  regenerate(currentPassword: string): Observable<BackupCodesResult> {
    return this.http.post<BackupCodesResult>('/api/me/mfa/backup-codes/regenerate', { currentPassword });
  }

  /** MFA is mandatory, so removing the factor also revokes the caller's own session. The user is signed out and enrols again at the next login. */
  disable(currentPassword: string): Observable<void> {
    return this.http.post<void>('/api/me/mfa/totp/disable', { currentPassword });
  }

  /** The server checks the current password, so a stolen session alone can't add a key. */
  startPasskeyRegistration(currentPassword: string): Observable<PasskeyChallenge> {
    return this.http.post<PasskeyChallenge>('/api/me/mfa/passkeys/register/start', { currentPassword });
  }

  finishPasskeyRegistration(challengeId: string, credential: unknown, name: string): Observable<Passkey> {
    return this.http.post<Passkey>('/api/me/mfa/passkeys/register/finish', { challengeId, credential, name });
  }

  /** Revokes the caller's own session like removing the app. A 404 means it was already gone (nobody was signed out). */
  deletePasskey(id: string, currentPassword: string): Observable<void> {
    return this.http.post<void>(`/api/me/mfa/passkeys/${encodeURIComponent(id)}/delete`, { currentPassword });
  }
}
