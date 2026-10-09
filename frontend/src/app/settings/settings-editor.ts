import { HttpErrorResponse } from '@angular/common/http';
import { computed, inject, Injectable, signal } from '@angular/core';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { SettingsService, SystemSettings, SystemSettingsUpdate } from './settings.service';
import { t } from '../shared/i18n/translator';

/**
 * A setting saved on its own, with its save state shown beside it. Only the runners' registration token is now: its
 * generation and removal are actions, while the sections save their other fields with one "Enregistrer".
 */
export type SettingsField = 'runnerRegistrationToken';

export type SaveState = 'saving' | 'saved' | 'error';

/**
 * The instance settings the admin page edits. A section keeps drafts as `linkedSignal`s of the saved values and sends
 * them with `saveSection`; the few actions saved at once go through `save`. Provided by the page, so every section
 * shares one copy.
 */
@Injectable()
export class SettingsEditor {
  private settings = inject(SettingsService);
  private toast = inject(GbtToastService);

  readonly current = signal<SystemSettings | null>(null);
  readonly loadState = signal<'loading' | 'loaded' | 'failed'>('loading');
  readonly saveStates = signal<Partial<Record<SettingsField, SaveState>>>({});
  /** A registration token typed in the "Exécution" section, sent with the section's other changes. Never shown back. */
  readonly runnerTokenDraft = signal('');

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
        this.toast.show(t('settings.loadFailed'), 'error');
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
        this.toast.show(t('settings.saveFailed'), 'error');
      },
    });
  }

  /**
   * Several fields at once, for a section saved with one button: one request, then a toast either way. `done` gets
   * whether they were stored.
   */
  saveSection(update: SystemSettingsUpdate, successMessage: string, done: (saved: boolean) => void): void {
    this.settings.updateAdmin(update).subscribe({
      next: (s) => {
        this.current.set(s);
        this.toast.show(successMessage);
        done(true);
      },
      error: (err: unknown) => {
        // A 400 means the server refused a value the client rules let through: retrying would fail the same way.
        const refused = err instanceof HttpErrorResponse && err.status === 400;
        this.toast.show(refused ? t('settings.refused') : t('settings.saveFailed'), 'error');
        done(false);
      },
    });
  }

  private setSaveState(field: SettingsField, state: SaveState): void {
    this.saveStates.update((states) => ({ ...states, [field]: state }));
  }
}
