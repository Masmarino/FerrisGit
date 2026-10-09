import { HttpErrorResponse } from '@angular/common/http';
import { Component, computed, inject, OnInit, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Alert } from '@masmarino/gabarit/alert';
import { Button } from '@masmarino/gabarit/button';
import { Card, CardHeader } from '@masmarino/gabarit/card';
import { Icon } from '@masmarino/gabarit/icon';
import { GbtInput } from '@masmarino/gabarit/input';
import { SegmentedControl, SegmentedControlOption } from '@masmarino/gabarit/segmented-control';
import { Skeleton } from '@masmarino/gabarit/skeleton';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { SettingsService, SmtpSecurity, SmtpSettings as SmtpSettingsData, SmtpSettingsUpdate } from '../settings.service';
import { TranslocoPipe } from '@jsverse/transloco';
import { t } from '../../shared/i18n/translator';

const securityOptions = (): SegmentedControlOption<SmtpSecurity>[] => [
  { value: 'none', label: t('settings.smtp.securityNone') },
  { value: 'starttls', label: 'STARTTLS' },
  { value: 'tls', label: 'TLS' },
];

const SECURITY_HINTS: Record<SmtpSecurity, string> = {
  none: 'settings.smtp.hintNone',
  starttls: 'settings.smtp.hintStarttls',
  tls: 'settings.smtp.hintTls',
};

/** The form as typed: the port as text, the password only what the user typed (never the stored one). */
export interface SmtpFormValues {
  host: string;
  port: string;
  username: string;
  password: string;
  passwordSet: boolean;
  fromAddress: string;
  fromName: string;
}

export interface SmtpFormErrors {
  host: string | null;
  port: string | null;
  password: string | null;
  fromAddress: string | null;
  fromName: string | null;
}

const FROM_NAME_MAX = 100;

/** The server's mailbox rule (one `@`, a dotted domain, no spaces), so a value it would refuse is caught here too. */
export function isValidMailbox(address: string): boolean {
  return /^[^\s@]+@[^\s@.]+(\.[^\s@.]+)+$/.test(address.trim());
}

export function validateSmtpForm(values: SmtpFormValues): SmtpFormErrors {
  const port = values.port.trim();
  const portNumber = Number(port);
  const validPort = /^\d+$/.test(port) && portNumber >= 1 && portNumber <= 65535;
  return {
    host: hostError(values.host),
    port: validPort ? null : t('settings.smtp.portError'),
    password: values.username.trim() && !values.passwordSet && !values.password ? t('settings.smtp.passwordRequired') : null,
    fromAddress: isValidMailbox(values.fromAddress) ? null : t('settings.smtp.addressInvalid'),
    fromName: [...values.fromName.trim()].length > FROM_NAME_MAX ? t('settings.smtp.maxLength', { max: FROM_NAME_MAX }) : null,
  };
}

function hostError(host: string): string | null {
  const trimmed = host.trim();
  if (!trimmed) {
    return t('settings.smtp.hostRequired');
  }
  return /\s/.test(trimmed) ? t('settings.smtp.hostSpaces') : null;
}

/** The password is write-only: sent only when typed, never returned. */
@Component({
  selector: 'fg-smtp-settings',
  standalone: true,
  imports: [TranslocoPipe, FormsModule, Card, CardHeader, Alert, Button, GbtInput, Icon, SegmentedControl, Skeleton],
  templateUrl: './smtp-settings.html',
  styleUrl: './smtp-settings.scss',
})
export class SmtpSettings implements OnInit {
  private settings = inject(SettingsService);
  private toast = inject(GbtToastService);

  protected readonly securityOptions = securityOptions();
  protected readonly skeletonCards = ['9rem', '7rem'];

  protected loadState = signal<'loading' | 'loaded' | 'failed'>('loading');
  private loaded = signal<SmtpSettingsData | null>(null);

  protected host = signal('');
  protected port = signal('');
  protected security = signal<SmtpSecurity>('starttls');
  protected username = signal('');
  protected password = signal('');
  protected fromAddress = signal('');
  protected fromName = signal('');
  protected saving = signal(false);
  protected submitted = signal(false);

  protected testRecipient = signal('');
  protected testing = signal(false);
  protected testResult = signal<{ sent: boolean; message: string } | null>(null);

  protected passwordSet = computed(() => this.loaded()?.passwordSet ?? false);
  protected configured = computed(() => this.loaded()?.configured ?? false);
  protected securityHint = computed(() => t(SECURITY_HINTS[this.security()]));
  protected passwordPlaceholder = computed(() => (this.passwordSet() && !this.password() ? t('settings.smtp.passwordUnchanged') : ''));

  protected dirty = computed(() => {
    const s = this.loaded();
    return (
      !!s &&
      (this.host().trim() !== s.host ||
        this.port().trim() !== String(s.port) ||
        this.security() !== s.security ||
        this.username().trim() !== s.username ||
        this.password() !== '' ||
        this.fromAddress().trim() !== s.fromAddress ||
        this.fromName().trim() !== s.fromName)
    );
  });

  private errors = computed(() =>
    validateSmtpForm({
      host: this.host(),
      port: this.port(),
      username: this.username(),
      password: this.password(),
      passwordSet: this.passwordSet(),
      fromAddress: this.fromAddress(),
      fromName: this.fromName(),
    }),
  );
  protected shown = computed(() => (this.submitted() ? this.errors() : null));

  protected canTest = computed(() => isValidMailbox(this.testRecipient()) && !this.testing() && !this.dirty() && this.configured());
  protected testHint = computed(() => (this.dirty() || !this.configured() ? t('settings.smtp.saveTestFirst') : t('settings.smtp.testUsesSaved')));

  ngOnInit(): void {
    this.load();
  }

  protected load(): void {
    this.loadState.set('loading');
    this.settings.getSmtp().subscribe({
      next: (s) => {
        this.hydrate(s);
        this.loadState.set('loaded');
      },
      error: () => {
        this.loadState.set('failed');
        this.toast.show(t('settings.smtp.loadFailed'), 'error');
      },
    });
  }

  protected save(): void {
    if (this.saving() || !this.dirty()) {
      return;
    }
    this.submitted.set(true);
    if (Object.values(this.errors()).some((error) => error !== null)) {
      return;
    }
    const update: SmtpSettingsUpdate = {
      host: this.host().trim(),
      port: Number(this.port().trim()),
      security: this.security(),
      username: this.username().trim(),
      // Only when typed: leaving it out keeps the stored one.
      ...(this.password() ? { password: this.password() } : {}),
      fromAddress: this.fromAddress().trim(),
      fromName: this.fromName().trim(),
    };
    this.saving.set(true);
    this.settings.updateSmtp(update).subscribe({
      next: (s) => {
        this.saving.set(false);
        this.hydrate(s);
        this.toast.show(t('settings.smtp.saved'));
      },
      error: (err: HttpErrorResponse) => {
        this.saving.set(false);
        // A 400 means the server refused a value the client rules let through, so retrying would fail the same way.
        this.toast.show(err.status === 400 ? t('settings.refused') : t('settings.saveFailed'), 'error');
      },
    });
  }

  protected sendTest(): void {
    if (!this.canTest()) {
      return;
    }
    const to = this.testRecipient().trim();
    this.testing.set(true);
    this.testResult.set(null);
    this.settings.testSmtp(to).subscribe({
      next: (result) => {
        this.testing.set(false);
        this.testResult.set(
          result.sent
            ? { sent: true, message: t('settings.smtp.testSent', { to }) }
            : { sent: false, message: t('settings.smtp.sendFailed', { reason: result.error ?? t('settings.smtp.unknownError') }) },
        );
      },
      error: () => {
        this.testing.set(false);
        this.testResult.set({ sent: false, message: t('settings.smtp.testFailed') });
      },
    });
  }

  private hydrate(s: SmtpSettingsData): void {
    this.loaded.set(s);
    this.host.set(s.host);
    this.port.set(String(s.port));
    this.security.set(s.security);
    this.username.set(s.username);
    this.password.set('');
    this.fromAddress.set(s.fromAddress);
    this.fromName.set(s.fromName);
    this.submitted.set(false);
    this.testResult.set(null);
  }
}
