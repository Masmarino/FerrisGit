import { HttpClient } from '@angular/common/http';
import { inject, Injectable, signal } from '@angular/core';
import { tap } from 'rxjs';
import { Language, isSupported } from '../shared/i18n/languages';
import { LanguageService } from '../shared/i18n/language.service';

export interface Me {
  id: string;
  username: string;
  email: string;
  isAdmin: boolean;
  createdAt: string;
  /** The interface language saved on the account, `null` while it follows the browser. */
  language: string | null;
}

@Injectable({ providedIn: 'root' })
export class MeService {
  private http = inject(HttpClient);
  private languages = inject(LanguageService);
  readonly id = signal('');
  readonly username = signal('');
  readonly email = signal('');
  readonly isAdmin = signal(false);
  /** When the account was created, ISO 8601; empty until loaded. */
  readonly createdAt = signal('');
  /** The account's interface language, `null` while it follows the browser. */
  readonly language = signal<Language | null>(null);

  load(): void {
    this.http.get<Me>('/api/auth/me').subscribe((res) => this.apply(res));
  }

  updateEmail(email: string) {
    return this.http.patch<Me>('/api/auth/me', { email }).pipe(tap((res) => this.apply(res)));
  }

  /** Saves the language on the account, then switches to it; `null` follows the browser again. */
  updateLanguage(language: Language | null) {
    return this.http.put<Me>('/api/auth/me/language', { language }).pipe(
      tap((res) => {
        this.apply(res);
        if (language === null) {
          void this.languages.choose(null);
        }
      }),
    );
  }

  changePassword(currentPassword: string, newPassword: string) {
    return this.http.post<{ token?: string } | null>('/api/auth/me/password', { currentPassword, newPassword });
  }

  private apply(res: Me): void {
    this.id.set(res.id);
    this.username.set(res.username);
    this.email.set(res.email);
    this.isAdmin.set(res.isAdmin);
    this.createdAt.set(res.createdAt ?? '');
    const language = res.language && isSupported(res.language) ? res.language : null;
    this.language.set(language);
    // The account's choice wins over this browser's: it follows the account from one device to the next.
    if (language) {
      void this.languages.adopt(language);
    }
  }
}
