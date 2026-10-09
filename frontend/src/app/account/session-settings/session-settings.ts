import { HttpClient } from '@angular/common/http';
import { Component, inject, signal } from '@angular/core';
import { Router } from '@angular/router';
import { Button } from '@masmarino/gabarit/button';
import { Card } from '@masmarino/gabarit/card';
import { ConfirmDangerModal } from '@masmarino/gabarit/confirm-danger-modal';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { AuthService } from '../../auth/auth.service';
import { TranslocoPipe } from '@jsverse/transloco';
import { t } from '../../shared/i18n/translator';

/**
 * Signs the account out everywhere, after a confirmation: every session ends, this one included, so the page goes to
 * sign-in. The Git tokens stay valid; the description says where to revoke them. ArtiFerris's card says the same.
 */
@Component({
  selector: 'fg-session-settings',
  standalone: true,
  imports: [TranslocoPipe, Button, Card, ConfirmDangerModal],
  template: `
    <gbt-card
      variant="outlined"
      icon="log-out"
      [heading]="'account.sessions.title' | transloco"
      [description]="'account.sessions.description' | transloco"
    >
      <div class="session-settings__actions">
        <gbt-button variant="danger" iconName="log-out" [text]="'account.sessions.everywhere' | transloco" (clicked)="confirming.set(true)" />
      </div>
    </gbt-card>
    @if (confirming()) {
      <gbt-confirm-danger-modal
        [isOpen]="true"
        [heading]="'account.sessions.everywhere' | transloco"
        [message]="'account.sessions.confirmMessage' | transloco"
        [confirmLabel]="'account.sessions.everywhere' | transloco"
        confirmIcon="log-out"
        [cancelLabel]="'common.cancel' | transloco"
        [closeLabel]="'common.close' | transloco"
        [busyLabel]="'account.sessions.busy' | transloco"
        [busy]="signingOut()"
        (confirmed)="logoutEverywhere()"
        (closed)="confirming.set(false)"
      />
    }
  `,
  styles: `
    :host {
      display: block;
    }
    .session-settings__actions {
      display: flex;
      justify-content: flex-end;
    }
  `,
})
export class SessionSettings {
  private http = inject(HttpClient);
  private auth = inject(AuthService);
  private router = inject(Router);
  private toast = inject(GbtToastService);

  protected confirming = signal(false);
  protected signingOut = signal(false);

  logoutEverywhere(): void {
    if (this.signingOut()) {
      return;
    }
    this.signingOut.set(true);
    this.http.post<void>('/api/auth/logout-all', {}).subscribe({
      next: () => {
        this.signingOut.set(false);
        this.confirming.set(false);
        this.auth.logout();
        this.router.navigateByUrl('/login');
      },
      // The confirmation covers the page, so an error closes it first to be seen.
      error: () => {
        this.signingOut.set(false);
        this.confirming.set(false);
        this.toast.show(t('account.sessions.failed'), 'error');
      },
    });
  }
}
