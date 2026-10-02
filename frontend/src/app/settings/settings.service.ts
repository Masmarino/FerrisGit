import { HttpClient } from '@angular/common/http';
import { inject, Injectable, signal } from '@angular/core';

export interface PublicSettings {
  executionEngine: 'docker-runners' | 'kubernetes';
}

export interface SystemSettings {
  executionEngine: 'docker-runners' | 'kubernetes';
  k8sNamespace: string | null;
  k8sCacheStorageClass: string | null;
  // Stored hashed on the server; only whether one is configured comes back.
  runnerRegistrationTokenConfigured: boolean;
  logRetentionDays: number | null;
  maxConcurrentJobs: number | null;
  jwtTtlHours: number;
  maxPushSizeMb: number;
  registrationEnabled: boolean;
  /** Visitors without an account can browse public repositories. */
  publicPagesEnabled: boolean;
  /** Search engines may index the public pages, but only while this is on. */
  seoIndexingEnabled: boolean;
  // Detected by the server from the cluster, not persisted: pre-fills the fields above when they're unset.
  detectedK8sNamespace: string | null;
  detectedK8sDefaultStorageClass: string | null;
}

export interface SystemSettingsUpdate {
  executionEngine?: string;
  k8sNamespace?: string | null;
  k8sCacheStorageClass?: string | null;
  runnerRegistrationToken?: string | null;
  logRetentionDays?: number | null;
  maxConcurrentJobs?: number | null;
  jwtTtlHours?: number;
  maxPushSizeMb?: number;
  /** Absent means unchanged. */
  registrationEnabled?: boolean;
  publicPagesEnabled?: boolean;
  seoIndexingEnabled?: boolean;
}

export type SmtpSecurity = 'none' | 'starttls' | 'tls';

export interface SmtpSettings {
  configured: boolean;
  host: string;
  port: number;
  security: SmtpSecurity;
  username: string;
  /** The password never comes back, only whether one is stored. */
  passwordSet: boolean;
  fromAddress: string;
  fromName: string;
}

export interface SmtpSettingsUpdate {
  host: string;
  port: number;
  security: SmtpSecurity;
  username: string;
  /** Leave undefined to keep the stored password. */
  password?: string;
  fromAddress: string;
  fromName: string;
}

export interface SmtpTestResult {
  sent: boolean;
  error?: string;
}

@Injectable({ providedIn: 'root' })
export class SettingsService {
  private http = inject(HttpClient);
  readonly publicSettings = signal<PublicSettings | null>(null);

  loadPublic(): void {
    this.http.get<PublicSettings>('/api/settings/public').subscribe((res) => this.publicSettings.set(res));
  }

  getAdmin() {
    return this.http.get<SystemSettings>('/api/admin/settings');
  }

  updateAdmin(update: SystemSettingsUpdate) {
    return this.http.put<SystemSettings>('/api/admin/settings', update);
  }

  getSmtp() {
    return this.http.get<SmtpSettings>('/api/admin/settings/smtp');
  }

  updateSmtp(update: SmtpSettingsUpdate) {
    return this.http.put<SmtpSettings>('/api/admin/settings/smtp', update);
  }

  testSmtp(to: string) {
    return this.http.post<SmtpTestResult>('/api/admin/settings/smtp/test', { to });
  }
}
