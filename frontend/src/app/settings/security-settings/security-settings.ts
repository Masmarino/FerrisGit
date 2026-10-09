import { Component, computed, inject, linkedSignal, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Card } from '@masmarino/gabarit/card';
import { GbtInput } from '@masmarino/gabarit/input';
import { Slider } from '@masmarino/gabarit/slider';
import { Switch } from '@masmarino/gabarit/switch';
import { SettingsEditor } from '../settings-editor';
import { SystemSettingsUpdate } from '../settings.service';
import { SettingsSaveBar } from '../settings-save-bar/settings-save-bar';
import { TranslocoPipe } from '@jsverse/transloco';
import { t } from '../../shared/i18n/translator';


/** A whole number of hours, 1 or more, as the server requires. `undefined`: invalid. */
function parseHours(value: string): number | undefined {
  const trimmed = value.trim();
  const hours = Number(trimmed);
  return /^\d+$/.test(trimmed) && Number.isSafeInteger(hours) && hours >= 1 ? hours : undefined;
}

/**
 * The "Sécurité" section: sessions, registration, public pages and the push size limit. Nothing is saved until
 * "Enregistrer", which sends every change in one request, as in the "Exécution" section.
 */
@Component({
  selector: 'fg-security-settings',
  standalone: true,
  imports: [TranslocoPipe, FormsModule, Card, GbtInput, Slider, Switch, SettingsSaveBar],
  templateUrl: './security-settings.html',
  styleUrl: './security-settings.scss',
})
export class SecuritySettings {
  private editor = inject(SettingsEditor);

  protected settings = this.editor.saved;

  // Drafts: each follows the saved value until edited, and again after a save.
  protected jwt = linkedSignal(() => String(this.settings().jwtTtlHours));
  protected maxPushSize = linkedSignal(() => this.settings().maxPushSizeMb);
  protected registration = linkedSignal(() => this.settings().registrationEnabled);
  protected publicPages = linkedSignal(() => this.settings().publicPagesEnabled);
  protected seoIndexing = linkedSignal(() => this.settings().seoIndexingEnabled);

  protected readonly maxPushSizeFormat = (mb: number): string => t('settings.security.mebibytes', { value: mb });

  protected saving = signal(false);
  /** Errors show once the admin tried to save, then follow the value. */
  private submitted = signal(false);

  private jwtValue = computed(() => parseHours(this.jwt()));
  protected jwtError = computed(() => (this.submitted() && this.jwtValue() === undefined ? t('settings.security.jwtError') : null));
  private invalid = computed(() => this.jwtValue() === undefined);

  /** What "Enregistrer" sends: the changes, and only those. */
  private update = computed<SystemSettingsUpdate>(() => {
    const saved = this.settings();
    const update: SystemSettingsUpdate = {};
    const jwt = this.jwtValue();
    if (jwt !== undefined && jwt !== saved.jwtTtlHours) {
      update.jwtTtlHours = jwt;
    }
    if (this.maxPushSize() !== saved.maxPushSizeMb) {
      update.maxPushSizeMb = this.maxPushSize();
    }
    if (this.registration() !== saved.registrationEnabled) {
      update.registrationEnabled = this.registration();
    }
    if (this.publicPages() !== saved.publicPagesEnabled) {
      update.publicPagesEnabled = this.publicPages();
    }
    if (this.seoIndexing() !== saved.seoIndexingEnabled) {
      update.seoIndexingEnabled = this.seoIndexing();
    }
    return update;
  });

  /** An invalid value counts too: it is a change, just not one that can be saved yet. */
  protected dirty = computed(() => Object.keys(this.update()).length > 0 || this.invalid());

  protected save(): void {
    if (this.saving() || !this.dirty()) {
      return;
    }
    this.submitted.set(true);
    if (this.invalid()) {
      return;
    }
    this.saving.set(true);
    this.editor.saveSection(this.update(), t('settings.security.saved'), (saved) => {
      this.saving.set(false);
      if (saved) {
        this.submitted.set(false);
      }
    });
  }

  /** Back to what is saved. */
  protected discard(): void {
    const saved = this.settings();
    this.jwt.set(String(saved.jwtTtlHours));
    this.maxPushSize.set(saved.maxPushSizeMb);
    this.registration.set(saved.registrationEnabled);
    this.publicPages.set(saved.publicPagesEnabled);
    this.seoIndexing.set(saved.seoIndexingEnabled);
    this.submitted.set(false);
  }
}
