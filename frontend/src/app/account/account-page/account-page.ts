import { NgTemplateOutlet } from '@angular/common';
import { Component, computed, inject, linkedSignal, OnInit, signal } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { FormsModule } from '@angular/forms';
import { ActivatedRoute, Router, RouterLink } from '@angular/router';
import { map } from 'rxjs';
import { MIN_PASSWORD_LENGTH } from '@masmarino/gabarit/auth';
import { Avatar } from '@masmarino/gabarit/avatar';
import { Badge } from '@masmarino/gabarit/badge';
import { Button } from '@masmarino/gabarit/button';
import { Card } from '@masmarino/gabarit/card';
import { Icon } from '@masmarino/gabarit/icon';
import { GbtInput } from '@masmarino/gabarit/input';
import { NavTab, NavTabs } from '@masmarino/gabarit/nav-tabs';
import { PageHeader } from '@masmarino/gabarit/page-header';
import { PageLayout } from '@masmarino/gabarit/page-layout';
import { SaveStatus } from '@masmarino/gabarit/save-status';
import { Select, SelectOption } from '@masmarino/gabarit/select';
import { GbtDateTimePipe } from '@masmarino/gabarit/format';
import { Skeleton } from '@masmarino/gabarit/skeleton';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { AuthService } from '../../auth/auth.service';
import { MeService } from '../../shell/me.service';
import { PageTitleService } from '../../shell/page-title.service';
import { MfaSettings } from '../mfa-settings/mfa-settings';
import { PasskeySettings } from '../passkey-settings/passkey-settings';
import { SessionSettings } from '../session-settings/session-settings';
import { ApiTokensList } from '../../api-tokens/api-tokens-list/api-tokens-list';
import { LANGUAGE_NAMES, Language, SUPPORTED_LANGUAGES, detectBrowserLanguage } from '../../shared/i18n/languages';
import { activeLocale, t } from '../../shared/i18n/translator';
import { TranslocoPipe } from '@jsverse/transloco';

type AccountSectionKey = 'profile' | 'password' | 'security' | 'tokens';

const SECTIONS: { key: AccountSectionKey; label: string; icon: string }[] = [
  { key: 'profile', get label() { return t('account.tabs.profile'); }, icon: 'user' },
  { key: 'password', get label() { return t('account.tabs.password'); }, icon: 'lock' },
  { key: 'security', get label() { return t('account.tabs.security'); }, icon: 'shield-check' },
  { key: 'tokens', get label() { return t('account.tabs.tokens'); }, icon: 'key' },
];

const DEFAULT_SECTION: AccountSectionKey = 'profile';

// Loose on purpose (something@something): it only catches typos, the server decides.
const EMAIL_PATTERN = /^[^\s@]+@[^\s@]+$/;

type SaveState = 'saving' | 'saved' | 'error';

/** The language choice that keeps none on the account: the interface follows the browser. */
const BROWSER_LANGUAGE = 'browser';
type LanguageChoice = Language | typeof BROWSER_LANGUAGE;

/** The section lives in the URL (`?section=<key>`, none for the profile) so it can be deep-linked; an unknown key shows the profile. */
@Component({
  selector: 'fg-account-page',
  standalone: true,
  imports: [TranslocoPipe, FormsModule, NgTemplateOutlet, Avatar, Badge, Button, GbtInput, Icon, SaveStatus, Select, Skeleton, ApiTokensList, MfaSettings, PasskeySettings, SessionSettings, PageHeader, PageLayout, Card, NavTab, NavTabs, RouterLink, GbtDateTimePipe],
  templateUrl: './account-page.html',
  styleUrl: './account-page.scss',
})
export class AccountPage implements OnInit {
  protected readonly locale = activeLocale;
  private auth = inject(AuthService);
  private router = inject(Router);
  private route = inject(ActivatedRoute);
  protected me = inject(MeService);
  private pageTitle = inject(PageTitleService);
  private toast = inject(GbtToastService);

  protected readonly minPasswordLength = MIN_PASSWORD_LENGTH;

  protected readonly sections = SECTIONS.map((section) => ({
    ...section,
    queryParams: section.key === DEFAULT_SECTION ? undefined : { section: section.key },
  }));

  private requestedSection = toSignal(this.route.queryParamMap.pipe(map((params) => params.get('section'))), { initialValue: null });

  protected activeSection = computed<AccountSectionKey>(() => {
    const requested = this.requestedSection();
    return SECTIONS.find((section) => section.key === requested)?.key ?? DEFAULT_SECTION;
  });

  /** Shows the typed address while it is saved, and the saved one again if the save fails. */
  protected emailShown = linkedSignal(() => this.me.email());
  protected avatarName = computed(() => this.me.username().replace(/[._-]+/g, ' '));
  protected emailState = signal<SaveState | null>(null);
  protected emailError = signal<string | null>(null);

  /** The browser's language first, named in brackets, then each language under its own name. */
  protected readonly languageOptions: SelectOption<LanguageChoice>[] = [
    { value: BROWSER_LANGUAGE, label: t('account.languageBrowser', { language: LANGUAGE_NAMES[detectBrowserLanguage()] }) },
    ...SUPPORTED_LANGUAGES.map((language) => ({ value: language, label: LANGUAGE_NAMES[language] })),
  ];
  protected languageShown = linkedSignal<LanguageChoice>(() => this.me.language() ?? BROWSER_LANGUAGE);
  protected languageState = signal<SaveState | null>(null);

  protected currentPassword = signal('');
  protected newPassword = signal('');
  protected confirmPassword = signal('');
  protected newPasswordError = signal<string | null>(null);
  protected confirmPasswordError = signal<string | null>(null);
  protected passwordSaving = signal(false);
  protected passwordResult = signal<{ state: 'saved' | 'error'; message: string } | null>(null);

  protected passwordLongEnough = computed(() => this.newPassword().length >= MIN_PASSWORD_LENGTH);
  protected passwordReady = computed(() => !!this.currentPassword() && !!this.newPassword() && !!this.confirmPassword());

  ngOnInit(): void {
    this.pageTitle.set(t('nav.account'));
    this.me.load();
  }

  /** Runs on blur (`committed`), not on each keystroke, so a half-typed address is never saved. */
  updateEmail(value: string): void {
    const email = value.trim();
    if (!email) {
      this.emailError.set(t('account.emailRequired'));
      return;
    }
    if (!EMAIL_PATTERN.test(email)) {
      this.emailError.set(t('account.emailInvalid'));
      return;
    }
    this.emailError.set(null);
    if (email === this.me.email()) {
      return;
    }
    const previous = this.me.email();
    this.emailShown.set(email);
    this.emailState.set('saving');
    this.me.updateEmail(email).subscribe({
      next: () => {
        this.emailState.set('saved');
        this.toast.show(t('account.emailUpdated'));
      },
      error: () => {
        this.emailShown.set(previous);
        this.emailState.set('error');
        this.toast.show(t('account.emailUpdateFailed'), 'error');
      },
    });
  }

  /** Saved on the account first: the interface then switches, and this page is drawn again in the new language. */
  protected updateLanguage(choice: LanguageChoice): void {
    if (choice === (this.me.language() ?? BROWSER_LANGUAGE)) {
      return;
    }
    this.languageShown.set(choice);
    this.languageState.set('saving');
    this.me.updateLanguage(choice === BROWSER_LANGUAGE ? null : choice).subscribe({
      next: () => this.languageState.set('saved'),
      error: () => {
        this.languageShown.set(this.me.language() ?? BROWSER_LANGUAGE);
        this.languageState.set('error');
        this.toast.show(t('account.languageUpdateFailed'), 'error');
      },
    });
  }

  protected setNewPassword(value: string): void {
    this.newPassword.set(value);
    this.newPasswordError.set(null);
  }

  protected setConfirmPassword(value: string): void {
    this.confirmPassword.set(value);
    this.confirmPasswordError.set(null);
  }

  changePassword(): void {
    if (this.passwordSaving()) {
      return;
    }
    this.passwordResult.set(null);
    this.newPasswordError.set(null);
    this.confirmPasswordError.set(null);
    if (this.newPassword() !== this.confirmPassword()) {
      this.confirmPasswordError.set(t('account.passwordsDiffer'));
      return;
    }
    if (this.newPassword().length < MIN_PASSWORD_LENGTH) {
      this.newPasswordError.set(t('account.passwordTooShort', { count: MIN_PASSWORD_LENGTH }));
      return;
    }
    this.passwordSaving.set(true);
    this.me.changePassword(this.currentPassword(), this.newPassword()).subscribe({
      next: (res) => {
        // The change revoked our token, so swap in the fresh one the server returns (an older server returns none).
        if (res?.token) {
          this.auth.setToken(res.token);
        }
        this.passwordSaving.set(false);
        this.passwordResult.set({ state: 'saved', message: t('account.passwordChanged') });
        this.currentPassword.set('');
        this.newPassword.set('');
        this.confirmPassword.set('');
        this.toast.show(t('account.passwordChanged'));
      },
      error: (err: { status?: number }) => {
        this.passwordSaving.set(false);
        // The length rule is checked before sending, so a 400 can only mean a wrong current password.
        const wrongPassword = err?.status === 400;
        this.passwordResult.set({
          state: 'error',
          message: wrongPassword ? t('account.wrongPassword') : t('account.passwordChangeFailed'),
        });
        this.toast.show(t('account.passwordChangeFailedToast'), 'error');
      },
    });
  }

  logout(): void {
    this.auth.logout();
    this.router.navigateByUrl('/login');
  }
}
