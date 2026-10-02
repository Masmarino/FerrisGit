import { Component, DestroyRef, inject, linkedSignal, signal, WritableSignal } from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { FormsModule } from '@angular/forms';
import { debounceTime, Subject } from 'rxjs';
import { Card, GbtInput, Slider, Switch } from '@masmarino/gabarit';
import { FieldSaveState } from '../field-save-state';
import { SettingsEditor } from '../settings-editor';

type SwitchField = 'registrationEnabled' | 'publicPagesEnabled' | 'seoIndexingEnabled';

const JWT_ERROR = "Entrez un nombre entier d'heures, 1 ou plus";

/** The "Sécurité" section: sessions, registration, public pages and the push size limit. */
@Component({
  selector: 'fg-security-settings',
  standalone: true,
  imports: [FormsModule, Card, GbtInput, Slider, Switch, FieldSaveState],
  templateUrl: './security-settings.html',
  styleUrl: './security-settings.scss',
})
export class SecuritySettings {
  private editor = inject(SettingsEditor);

  protected settings = this.editor.saved;
  protected jwtError = signal<string | null>(null);

  protected jwtShown = linkedSignal(() => String(this.settings().jwtTtlHours));
  protected maxPushSizeShown = linkedSignal(() => this.settings().maxPushSizeMb);
  protected registrationShown = linkedSignal(() => this.settings().registrationEnabled);
  protected publicPagesShown = linkedSignal(() => this.settings().publicPagesEnabled);
  protected seoIndexingShown = linkedSignal(() => this.settings().seoIndexingEnabled);

  protected readonly maxPushSizeFormat = (mb: number): string => `${mb} Mio`;
  // `gbt-slider` emits `ngModelChange` on every drag tick, unlike GbtInput's `committed`. Debounce so one PUT goes out once dragging settles.
  private maxPushSizeInput$ = new Subject<number>();

  constructor() {
    this.maxPushSizeInput$.pipe(debounceTime(400), takeUntilDestroyed(inject(DestroyRef))).subscribe((mb) => this.setMaxPushSizeMb(mb));
  }

  /** Called on blur (see `GbtInput.committed`). The unchanged-value guard skips the PUT for a blur that edited nothing. */
  protected setJwtTtlHours(value: string): void {
    const trimmed = value.trim();
    const hours = Number(trimmed);
    if (!/^\d+$/.test(trimmed) || !Number.isSafeInteger(hours) || hours < 1) {
      this.jwtError.set(JWT_ERROR);
      return;
    }
    this.jwtError.set(null);
    if (hours === this.settings().jwtTtlHours) {
      return;
    }
    this.jwtShown.set(String(hours));
    this.editor.save('jwtTtlHours', { jwtTtlHours: hours }, () => this.jwtShown.set(String(this.settings().jwtTtlHours)));
  }

  protected onMaxPushSizeInput(mb: number): void {
    this.maxPushSizeShown.set(mb);
    this.maxPushSizeInput$.next(mb);
  }

  setMaxPushSizeMb(mb: number): void {
    if (!Number.isFinite(mb) || mb <= 0 || mb === this.settings().maxPushSizeMb) {
      return;
    }
    this.maxPushSizeShown.set(mb);
    this.editor.save('maxPushSizeMb', { maxPushSizeMb: mb }, () => this.maxPushSizeShown.set(this.settings().maxPushSizeMb));
  }

  protected setRegistrationEnabled(enabled: boolean): void {
    this.setSwitch('registrationEnabled', this.registrationShown, enabled);
  }

  protected setPublicPagesEnabled(enabled: boolean): void {
    this.setSwitch('publicPagesEnabled', this.publicPagesShown, enabled);
  }

  protected setSeoIndexingEnabled(enabled: boolean): void {
    this.setSwitch('seoIndexingEnabled', this.seoIndexingShown, enabled);
  }

  private setSwitch(field: SwitchField, shown: WritableSignal<boolean>, enabled: boolean): void {
    if (enabled === shown()) {
      return;
    }
    shown.set(enabled);
    this.editor.save(field, { [field]: enabled }, () => shown.set(this.settings()[field]));
  }
}
