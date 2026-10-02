import { HttpClient } from '@angular/common/http';
import { inject, Injectable } from '@angular/core';
import { Observable } from 'rxjs';

export interface AdminUser {
  id: string;
  username: string;
  email: string;
  isAdmin: boolean;
  createdAt: string;
  /** `invited` while an invitation is pending, even if its `invitationExpiresAt` is already past. */
  state: 'active' | 'invited';
  invitationExpiresAt: string | null;
  mfaEnabled: boolean;
}

/** `emailError` and `activationUrl` are missing keys (not null) unless the mail could not be sent. The link is then the only way to reach the invitee. */
export interface InviteResult {
  user: AdminUser;
  emailSent: boolean;
  emailError?: string;
  activationUrl?: string;
}

/** Same contract as {@link InviteResult}, without the user row. `emailError` and `resetUrl` are missing unless the mail was not sent. */
export interface PasswordResetResult {
  emailSent: boolean;
  emailError?: string;
  resetUrl?: string;
}

/** Metadata and size on disk only, never a path into its content. Group repositories are not listed (they survive the account's deletion). */
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
    return this.http.get<AdminUser[]>('/api/admin/users');
  }

  invite(username: string, email: string, isAdmin: boolean): Observable<InviteResult> {
    return this.http.post<InviteResult>('/api/admin/users/invite', { username, email, isAdmin });
  }

  /** Issues a new link (the old one stops working) and mails it again. 400 for a user who is already active. */
  resend(id: string): Observable<InviteResult> {
    return this.http.post<InviteResult>(`${this.userUrl(id)}/invitation`, {});
  }

  /** Ends the user's sessions and deletes their factors, so their next sign-in starts a first enrolment. */
  resetMfa(id: string): Observable<void> {
    return this.http.delete<void>(`${this.userUrl(id)}/mfa`);
  }

  /** Ends the user's sessions, voids their current password and mails a link valid for one hour. 400 for the caller's own account or a pending one. */
  resetPassword(id: string): Observable<PasswordResetResult> {
    return this.http.post<PasswordResetResult>(`${this.userUrl(id)}/reset-password`, {});
  }

  /** Grants or removes the administrator flag. 204, idempotent; 409 for the demotion of the last active administrator. */
  setAdmin(id: string, isAdmin: boolean): Observable<void> {
    return this.http.put<void>(`${this.userUrl(id)}/admin`, { isAdmin });
  }

  /** Newest first, with their size on disk. 404 for an unknown user. */
  repositories(id: string): Observable<AdminUserRepository[]> {
    return this.http.get<AdminUserRepository[]>(`${this.userUrl(id)}/repositories`);
  }

  /** Deletes the account and its personal repositories. What the user wrote elsewhere stays, attributed to a deleted user. Returns 204, 400 for the caller's own account, 404 for an unknown user, 409 for the last active administrator or a group's last Maintainer. */
  deleteUser(id: string): Observable<void> {
    return this.http.delete<void>(this.userUrl(id));
  }
}
