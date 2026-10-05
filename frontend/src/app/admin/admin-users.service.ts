import { HttpClient } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { map, Observable } from 'rxjs';

export interface AdminUser {
  id: string;
  /**
   * The name to show: the username, or the e-mail address of an invitee who has not chosen one yet (`named` false).
   * The server sends no name for them, never the placeholder it holds until then.
   */
  username: string;
  named: boolean;
  email: string;
  isAdmin: boolean;
  createdAt: string;
  /** Stays `invited` while pending, even once `invitationExpiresAt` has passed. */
  state: 'active' | 'invited';
  invitationExpiresAt: string | null;
  mfaEnabled: boolean;
}

type AdminUserRow = Omit<AdminUser, 'username' | 'named'> & { username: string | null };

const toAdminUser = (row: AdminUserRow): AdminUser => ({ ...row, username: row.username ?? row.email, named: row.username !== null });

const withAdminUser = <T extends { user: AdminUserRow }>(result: T): Omit<T, 'user'> & { user: AdminUser } => ({
  ...result,
  user: toAdminUser(result.user),
});

/** Both fields are absent unless the mail failed to send, in which case the link is the only way to reach the invitee. */
export interface InviteResult {
  user: AdminUser;
  emailSent: boolean;
  emailError?: string;
  activationUrl?: string;
}

type InviteResultRow = Omit<InviteResult, 'user'> & { user: AdminUserRow };

/** Like InviteResult without the user row. `emailError` and `resetUrl` are only there when the mail failed. */
export interface PasswordResetResult {
  emailSent: boolean;
  emailError?: string;
  resetUrl?: string;
}

/** Name and size on disk only, no way into the content. Group repos aren't listed since they outlive the account. */
export interface AdminUserRepository {
  id: string;
  name: string;
  description: string;
  visibility: 'public' | 'private';
  createdAt: string;
  /** Null when the server could not compute it. */
  sizeBytes: number | null;
}

@Injectable({ providedIn: 'root' })
export class AdminUsersService {
  private http = inject(HttpClient);

  private userUrl(id: string): string {
    return `/api/admin/users/${encodeURIComponent(id)}`;
  }

  list(): Observable<AdminUser[]> {
    return this.http.get<AdminUserRow[]>('/api/admin/users').pipe(map((rows) => rows.map(toAdminUser)));
  }

  /** By e-mail only: the invitee chooses their username when activating the account. */
  invite(email: string, isAdmin: boolean): Observable<InviteResult> {
    return this.http.post<InviteResultRow>('/api/admin/users/invite', { email, isAdmin }).pipe(map(withAdminUser));
  }

  /** New link (the old one stops working), mailed again. 400 if the user is already active. */
  resend(id: string): Observable<InviteResult> {
    return this.http.post<InviteResultRow>(`${this.userUrl(id)}/invitation`, {}).pipe(map(withAdminUser));
  }

  /** Ends their sessions and wipes their factors, so the next sign-in enrols from scratch. */
  resetMfa(id: string): Observable<void> {
    return this.http.delete<void>(`${this.userUrl(id)}/mfa`);
  }

  /** Ends their sessions, voids the password and mails a link valid for one hour. 400 for your own account or a pending one. */
  resetPassword(id: string): Observable<PasswordResetResult> {
    return this.http.post<PasswordResetResult>(`${this.userUrl(id)}/reset-password`, {});
  }

  /** Idempotent, 204. 409 when demoting the last active administrator. */
  setAdmin(id: string, isAdmin: boolean): Observable<void> {
    return this.http.put<void>(`${this.userUrl(id)}/admin`, { isAdmin });
  }

  /** Newest first, with their size on disk. 404 for an unknown user. */
  repositories(id: string): Observable<AdminUserRepository[]> {
    return this.http.get<AdminUserRepository[]>(`${this.userUrl(id)}/repositories`);
  }

  /** Deletes the account and its personal repos; what they wrote elsewhere stays, attributed to a deleted user. 204, 400 for your own account, 404 for an unknown user, 409 for the last active administrator or a group's last Maintainer. */
  deleteUser(id: string): Observable<void> {
    return this.http.delete<void>(this.userUrl(id));
  }
}
