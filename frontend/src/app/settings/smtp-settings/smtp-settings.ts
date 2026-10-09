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

const SECURITY_OPTIONS: SegmentedControlOption<SmtpSecurity>[] = [
  { value: 'none', label: 'Aucune' },
  { value: 'starttls', label: 'STARTTLS' },
  { value: 'tls', label: 'TLS' },
];

const SECURITY_HINTS: Record<SmtpSecurity, string> = {
  none: 'Connexion en clair, généralement sur le port 25.',
  starttls: 'La connexion démarre en clair puis passe en chiffré, généralement sur le port 587.',
  tls: 'Connexion chiffrée dès le départ, généralement sur le port 465.',
};

const TEST_FAILED = "Échec de l'envoi : le serveur n'a pas pu traiter la demande. Réessayez plus tard.";

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
    port: validPort ? null : 'Entre 1 et 65535',
    password: values.username.trim() && !values.passwordSet && !values.password ? 'Indiquez le mot de passe de ce compte' : null,
    fromAddress: isValidMailbox(values.fromAddress) ? null : 'Entrez une adresse e-mail valide',
    fromName: [...values.fromName.trim()].length > FROM_NAME_MAX ? `${FROM_NAME_MAX} caractères au maximum` : null,
  };
}

function hostError(host: string): string | null {
  const trimmed = host.trim();
  if (!trimmed) {
    return 'Indiquez le serveur SMTP';
  }
  return /\s/.test(trimmed) ? "Le serveur ne peut pas contenir d'espace" : null;
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

  protected readonly securityOptions = SECURITY_OPTIONS;
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
  protected securityHint = computed(() => SECURITY_HINTS[this.security()]);
  protected passwordPlaceholder = computed(() => (this.passwordSet() && !this.password() ? '•••••••• (inchangé)' : ''));

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
  protected testHint = computed(() => (this.dirty() || !this.configured() ? 'Enregistrez les réglages avant de tester.' : 'Le test utilise les réglages enregistrés.'));

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
        this.toast.show('Impossible de charger les réglages e-mail. Réessayez plus tard.', 'error');
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
        this.toast.show('Réglages e-mail enregistrés');
      },
      error: (err: HttpErrorResponse) => {
        this.saving.set(false);
        // A 400 means the server refused a value the client rules let through, so retrying would fail the same way.
        this.toast.show(err.status === 400 ? 'Réglages refusés : vérifiez les champs.' : "Échec de l'enregistrement. Réessayez.", 'error');
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
            ? { sent: true, message: `E-mail de test envoyé à ${to}` }
            : { sent: false, message: `Échec de l'envoi : ${result.error ?? 'erreur inconnue'}` },
        );
      },
      error: () => {
        this.testing.set(false);
        this.testResult.set({ sent: false, message: TEST_FAILED });
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
