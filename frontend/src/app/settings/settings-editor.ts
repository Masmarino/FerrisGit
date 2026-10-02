import { computed, inject, Injectable, signal } from '@angular/core';
import { GbtToastService } from '@masmarino/gabarit';
import { SettingsService, SystemSettings, SystemSettingsUpdate } from './settings.service';

export type SettingsField =
  | 'executionEngine'
  | 'k8sNamespace'
  | 'k8sCacheStorageClass'
  | 'jwtTtlHours'
  | 'maxPushSizeMb'
  | 'runnerRegistrationToken'
  | 'maxConcurrentJobs'
  | 'logRetentionDays'
  | 'registrationEnabled'
  | 'publicPagesEnabled'
  | 'seoIndexingEnabled';

export type SaveState = 'saving' | 'saved' | 'error';

/**
 * The instance settings the admin page edits, and the save each field does on its own. A section shows a `linkedSignal`
 * of the saved value: the typed value while saving, the saved one again if the save fails, so a refused value never
 * stays on screen. Provided by the page, so every section shares one copy.
 */
@Injectable()
export class SettingsEditor {
  private settings = inject(SettingsService);
  private toast = inject(GbtToastService);

  readonly current = signal<SystemSettings | null>(null);
  readonly loadState = signal<'loading' | 'loaded' | 'failed'>('loading');
  readonly saveStates = signal<Partial<Record<SettingsField, SaveState>>>({});

  /** The loaded settings. Sections only exist once the page has loaded them. */
  readonly saved = computed(() => {
    const current = this.current();
    if (!current) {
      throw new Error('the instance settings are not loaded yet');
    }
    return current;
  });

  load(): void {
    this.loadState.set('loading');
    this.settings.getAdmin().subscribe({
      next: (s) => {
        this.current.set(s);
        this.loadState.set('loaded');
      },
      error: () => {
        this.loadState.set('failed');
        this.toast.show('Impossible de charger les réglages. Réessayez plus tard.', 'error');
      },
    });
  }

  /** `rollback` puts the field back on its saved value when the server refuses; `onSaved` runs once it's stored. */
  save(field: SettingsField, update: SystemSettingsUpdate, rollback: () => void, onSaved?: () => void): void {
    this.setSaveState(field, 'saving');
    this.settings.updateAdmin(update).subscribe({
      next: (s) => {
        this.current.set(s);
        this.setSaveState(field, 'saved');
        onSaved?.();
      },
      error: () => {
        rollback();
        this.setSaveState(field, 'error');
        this.toast.show("Échec de l'enregistrement. Réessayez.", 'error');
      },
    });
  }

  private setSaveState(field: SettingsField, state: SaveState): void {
    this.saveStates.update((states) => ({ ...states, [field]: state }));
  }
}
