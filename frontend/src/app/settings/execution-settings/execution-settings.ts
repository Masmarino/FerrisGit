import { Component, computed, inject, linkedSignal, signal } from '@angular/core';
import { Card } from '@masmarino/gabarit/card';
import { Icon } from '@masmarino/gabarit/icon';
import { GbtInput } from '@masmarino/gabarit/input';
import { SegmentedControl, SegmentedControlOption } from '@masmarino/gabarit/segmented-control';
import { FormsModule } from '@angular/forms';
import { RunnerRegistrationToken } from '../runner-registration-token/runner-registration-token';
import { SettingsSaveBar } from '../settings-save-bar/settings-save-bar';
import { SettingsEditor } from '../settings-editor';
import { SystemSettings, SystemSettingsUpdate } from '../settings.service';

type Engine = SystemSettings['executionEngine'];

const ENGINE_OPTIONS: SegmentedControlOption<Engine>[] = [
  { value: 'docker-runners', label: 'Docker / runners' },
  { value: 'kubernetes', label: 'Kubernetes' },
];

const MAX_JOBS_ERROR = 'Entrez un nombre entier, 1 ou plus, ou laissez vide pour ne pas limiter';
const RETENTION_ERROR = 'Entrez un nombre entier de jours, 1 ou plus, ou laissez vide pour tout conserver';

const configuredOrDetected = (configured: string | null, detected: string | null) => configured ?? detected ?? '';

/** Empty is "no limit" (`null`); otherwise a whole number, 1 or more, as the server requires. `undefined`: invalid. */
function parseCount(value: string): number | null | undefined {
  const trimmed = value.trim();
  if (trimmed === '') {
    return null;
  }
  const count = Number(trimmed);
  return /^\d+$/.test(trimmed) && Number.isSafeInteger(count) && count >= 1 ? count : undefined;
}

const countText = (count: number | null) => String(count ?? '');

/**
 * The "Exécution" section: where jobs run, then the settings of that engine only (the runners' for Docker, the
 * cluster's for Kubernetes), and log retention. Nothing is saved until "Enregistrer", which sends what is shown in one
 * request; generating or removing the runners' registration token stays an action of its own.
 */
@Component({
  selector: 'fg-execution-settings',
  standalone: true,
  imports: [FormsModule, Card, GbtInput, Icon, SegmentedControl, RunnerRegistrationToken, SettingsSaveBar],
  templateUrl: './execution-settings.html',
  styleUrl: './execution-settings.scss',
})
export class ExecutionSettings {
  private editor = inject(SettingsEditor);

  protected settings = this.editor.saved;
  protected readonly engineOptions = ENGINE_OPTIONS;

  // Drafts: each follows the saved value until edited, and again after a save.
  protected engine = linkedSignal<Engine>(() => this.settings().executionEngine);
  protected namespace = linkedSignal(() => this.namespaceDefault());
  protected storageClass = linkedSignal(() => this.storageClassDefault());
  protected maxJobs = linkedSignal(() => countText(this.settings().maxConcurrentJobs));
  protected retention = linkedSignal(() => countText(this.settings().logRetentionDays));
  /**
   * A Kubernetes field pre-filled from the cluster is only saved once edited: tabbing through it saves nothing. Saving
   * the detected StorageClass is how the admin confirms it supports ReadWriteMany.
   */
  private namespaceEdited = linkedSignal(() => (this.settings(), false));
  private storageClassEdited = linkedSignal(() => (this.settings(), false));
  protected tokenDraft = this.editor.runnerTokenDraft;

  protected saving = signal(false);
  /** Errors show once the admin tried to save, then follow the value. */
  private submitted = signal(false);

  protected engineChanged = computed(() => this.engine() !== this.settings().executionEngine);
  protected onKubernetes = computed(() => this.engine() === 'kubernetes');

  private maxJobsValue = computed(() => parseCount(this.maxJobs()));
  private retentionValue = computed(() => parseCount(this.retention()));
  protected maxJobsError = computed(() => (this.submitted() && this.maxJobsValue() === undefined ? MAX_JOBS_ERROR : null));
  protected retentionError = computed(() => (this.submitted() && this.retentionValue() === undefined ? RETENTION_ERROR : null));
  private invalid = computed(() => this.retentionValue() === undefined || (!this.onKubernetes() && this.maxJobsValue() === undefined));

  /** What "Enregistrer" sends: the changes to the fields shown, and only those. */
  private update = computed<SystemSettingsUpdate>(() => {
    const saved = this.settings();
    const update: SystemSettingsUpdate = {};
    if (this.engineChanged()) {
      update.executionEngine = this.engine();
    }
    const retention = this.retentionValue();
    if (retention !== undefined && retention !== saved.logRetentionDays) {
      update.logRetentionDays = retention;
    }
    if (this.onKubernetes()) {
      const namespace = this.k8sValue(this.namespace(), this.namespaceEdited(), saved.k8sNamespace);
      if (namespace !== saved.k8sNamespace) {
        update.k8sNamespace = namespace;
      }
      const storageClass = this.k8sValue(this.storageClass(), this.storageClassEdited(), saved.k8sCacheStorageClass);
      if (storageClass !== saved.k8sCacheStorageClass) {
        update.k8sCacheStorageClass = storageClass;
      }
    } else {
      const maxJobs = this.maxJobsValue();
      if (maxJobs !== undefined && maxJobs !== saved.maxConcurrentJobs) {
        update.maxConcurrentJobs = maxJobs;
      }
      const token = this.tokenDraft().trim();
      if (token !== '') {
        update.runnerRegistrationToken = token;
      }
    }
    return update;
  });

  /** An invalid value counts too: it is a change, just not one that can be saved yet. */
  protected dirty = computed(() => Object.keys(this.update()).length > 0 || this.invalid());

  protected setEngine(engine: Engine): void {
    this.engine.set(engine);
  }

  protected editNamespace(value: string): void {
    this.namespace.set(value);
    this.namespaceEdited.set(true);
  }

  protected editStorageClass(value: string): void {
    this.storageClass.set(value);
    this.storageClassEdited.set(true);
  }

  protected save(): void {
    if (this.saving() || !this.dirty()) {
      return;
    }
    this.submitted.set(true);
    if (this.invalid()) {
      return;
    }
    this.saving.set(true);
    this.editor.saveSection(this.update(), "Réglages d'exécution enregistrés", (saved) => {
      this.saving.set(false);
      if (saved) {
        this.submitted.set(false);
        this.tokenDraft.set('');
      }
    });
  }

  /** Back to what is saved, the engine included. */
  protected discard(): void {
    const saved = this.settings();
    this.engine.set(saved.executionEngine);
    this.namespace.set(this.namespaceDefault());
    this.storageClass.set(this.storageClassDefault());
    this.namespaceEdited.set(false);
    this.storageClassEdited.set(false);
    this.maxJobs.set(countText(saved.maxConcurrentJobs));
    this.retention.set(countText(saved.logRetentionDays));
    this.tokenDraft.set('');
    this.submitted.set(false);
  }

  /** Untouched, a field keeps its saved value even when it shows the detected one; edited, empty clears it. */
  private k8sValue(shown: string, edited: boolean, saved: string | null): string | null {
    if (!edited) {
      return saved;
    }
    const trimmed = shown.trim();
    return trimmed === '' ? null : trimmed;
  }

  private namespaceDefault(): string {
    const { k8sNamespace, detectedK8sNamespace } = this.settings();
    return configuredOrDetected(k8sNamespace, detectedK8sNamespace);
  }

  private storageClassDefault(): string {
    const { k8sCacheStorageClass, detectedK8sDefaultStorageClass } = this.settings();
    return configuredOrDetected(k8sCacheStorageClass, detectedK8sDefaultStorageClass);
  }
}
