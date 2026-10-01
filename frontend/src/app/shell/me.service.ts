import { HttpClient } from '@angular/common/http';
import { inject, Injectable, signal } from '@angular/core';
import { tap } from 'rxjs';

export interface Me {
  id: string;
  username: string;
  email: string;
  isAdmin: boolean;
}

@Injectable({ providedIn: 'root' })
export class MeService {
  private http = inject(HttpClient);
  readonly id = signal('');
  readonly username = signal('');
  readonly email = signal('');
  readonly isAdmin = signal(false);

  load(): void {
    this.http.get<Me>('/api/auth/me').subscribe((res) => this.apply(res));
  }

  updateEmail(email: string) {
    return this.http.patch<Me>('/api/auth/me', { email }).pipe(tap((res) => this.apply(res)));
  }

  changePassword(currentPassword: string, newPassword: string) {
    return this.http.post<{ token?: string } | null>('/api/auth/me/password', { currentPassword, newPassword });
  }

  private apply(res: Me): void {
    this.id.set(res.id);
    this.username.set(res.username);
    this.email.set(res.email);
    this.isAdmin.set(res.isAdmin);
  }
}
