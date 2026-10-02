import { Component, inject, linkedSignal, signal, WritableSignal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Card, GbtInput, Icon, SegmentedControl, SegmentedControlOption } from '@masmarino/gabarit';
import { FieldSaveState } from '../field-save-state';
import { RunnerRegistrationToken } from '../runner-registration-token/runner-registration-token';
import { SettingsEditor } from '../settings-editor';
import { SystemSettings } from '../settings.service';

type Engine = SystemSettings['executionEngine'];
type K8sField = 'k8sNamespace' | 'k8sCacheStorageClass';
type CountField = 'maxConcurrentJobs' | 'logRetentionDays';

const ENGINE_OPTIONS: SegmentedControlOption<Engine>[] = [
  { value: 'docker-runners', label: 'Docker / runners' },
  { value: 'kubernetes', label: 'Kubernetes' },
];

const MAX_JOBS_ERROR = 'Entrez un nombre entier, 1 ou plus, ou laissez vide pour ne pas limiter';
const RETENTION_ERROR = 'Entrez un nombre entier de jours, 1 ou plus, ou laissez vide pour tout conserver';

const configuredOrDetected = (configured: string | null, detected: string | null) => configured ?? detected ?? '';

/** The "Exécution" section: where the jobs run, how runners register, the Kubernetes target and the log retention. */
@Component({
  selector: 'fg-execution-settings',
  standalone: true,
  imports: [FormsModule, Card, GbtInput, Icon, SegmentedControl, FieldSaveState, RunnerRegistrationToken],
  templateUrl: './execution-settings.html',
  styleUrl: './execution-settings.scss',
})
export class ExecutionSettings {
  private editor = inject(SettingsEditor);

  protected settings = this.editor.saved;
  protected readonly engineOptions = ENGINE_OPTIONS;

  protected engineShown = linkedSignal<Engine>(() => this.settings().executionEngine);
  protected namespaceShown = linkedSignal(() => this.namespaceDefault());
  protected storageClassShown = linkedSignal(() => this.storageClassDefault());
  protected maxJobsShown = linkedSignal(() => String(this.settings().maxConcurrentJobs ?? ''));
  protected retentionShown = linkedSignal(() => String(this.settings().logRetentionDays ?? ''));
  protected maxJobsError = signal<string | null>(null);
  protected retentionError = signal<string | null>(null);
  private edited: Record<K8sField, boolean> = { k8sNamespace: false, k8sCacheStorageClass: false };

  protected setEngine(engine: Engine): void {
    if (engine === this.engineShown()) {
      return;
    }
    this.engineShown.set(engine);
    this.editor.save('executionEngine', { executionEngine: engine }, () => this.engineShown.set(this.settings().executionEngine));
  }

  /** Empty means "no limit". Same rule as the server: a whole number, 1 or more. */
  protected setMaxConcurrentJobs(value: string): void {
    this.commitCount('maxConcurrentJobs', value, this.maxJobsShown, this.maxJobsError, MAX_JOBS_ERROR);
  }

  /** Empty means "keep the logs forever". */
  protected setLogRetentionDays(value: string): void {
    this.commitCount('logRetentionDays', value, this.retentionShown, this.retentionError, RETENTION_ERROR);
  }

  private commitCount(field: CountField, value: string, shown: WritableSignal<string>, error: WritableSignal<string | null>, message: string): void {
    const trimmed = value.trim();
    const count = trimmed === '' ? null : Number(trimmed);
    if (count !== null && (!/^\d+$/.test(trimmed) || !Number.isSafeInteger(count) || count < 1)) {
      error.set(message);
      return;
    }
    error.set(null);
    shown.set(trimmed);
    if (count !== this.settings()[field]) {
      this.editor.save(field, { [field]: count }, () => shown.set(String(this.settings()[field] ?? '')));
    }
  }

  /**
   * Saved on blur only when edited (`markEdited`), so tabbing through a field pre-filled from the cluster saves
   * nothing. Saving the detected StorageClass is how the admin confirms that it supports ReadWriteMany.
   */
  protected markEdited(field: K8sField): void {
    this.edited[field] = true;
  }

  protected setK8sNamespace(value: string): void {
    this.commitK8sField('k8sNamespace', value, this.namespaceShown, () => this.namespaceDefault());
  }

  protected setK8sCacheStorageClass(value: string): void {
    this.commitK8sField('k8sCacheStorageClass', value, this.storageClassShown, () => this.storageClassDefault());
  }

  private commitK8sField(field: K8sField, value: string, shown: WritableSignal<string>, savedText: () => string): void {
    if (!this.edited[field]) {
      return;
    }
    this.edited[field] = false;
    const trimmed = value.trim();
    const next = trimmed === '' ? null : trimmed;
    if (next === this.settings()[field]) {
      return;
    }
    shown.set(trimmed);
    this.editor.save(field, { [field]: next }, () => shown.set(savedText()));
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
