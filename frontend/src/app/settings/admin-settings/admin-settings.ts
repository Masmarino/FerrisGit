import { NgTemplateOutlet } from '@angular/common';
import { Component, computed, DestroyRef, inject, linkedSignal, OnInit, signal, WritableSignal } from '@angular/core';
import { takeUntilDestroyed, toSignal } from '@angular/core/rxjs-interop';
import { FormsModule } from '@angular/forms';
import { ActivatedRoute, RouterLink } from '@angular/router';
import { debounceTime, map, Subject } from 'rxjs';
import {
  Alert,
  Button,
  Card,
  CardHeader,
  GbtInput,
  GbtToastService,
  Icon,
  NavTab,
  NavTabs,
  PageHeader,
  PageLayout,
  SaveStatus,
  SegmentedControl,
  SegmentedControlOption,
  Skeleton,
  Slider,
  Switch,
} from '@masmarino/gabarit';
import { SettingsService, SystemSettings, SystemSettingsUpdate } from '../settings.service';
import { PageTitleService } from '../../shell/page-title.service';
import { SmtpSettings } from '../smtp-settings/smtp-settings';

export type AdminSettingsSectionKey = 'execution' | 'security' | 'email';

type Engine = SystemSettings['executionEngine'];
type Field = 'executionEngine' | 'k8sNamespace' | 'k8sCacheStorageClass' | 'jwtTtlHours' | 'maxPushSizeMb' | SwitchField;
type SwitchField = 'registrationEnabled' | 'publicPagesEnabled' | 'seoIndexingEnabled';
type SaveState = 'saving' | 'saved' | 'error';
type K8sField = 'k8sNamespace' | 'k8sCacheStorageClass';

const SECTIONS: { key: AdminSettingsSectionKey; label: string; icon: string }[] = [
  { key: 'execution', label: 'Exécution', icon: 'server' },
  { key: 'security', label: 'Sécurité', icon: 'lock' },
  { key: 'email', label: 'E-mail', icon: 'mail' },
];

const DEFAULT_SECTION: AdminSettingsSectionKey = 'execution';

const ENGINE_OPTIONS: SegmentedControlOption<Engine>[] = [
  { value: 'docker-runners', label: 'Docker / runners' },
  { value: 'kubernetes', label: 'Kubernetes' },
];

const JWT_ERROR = "Entrez un nombre entier d'heures, 1 ou plus";

const configuredOrDetected = (configured: string | null, detected: string | null) => configured ?? detected ?? '';

/**
 * Each field saves on its own when committed and shows a `linkedSignal` of the saved value. It shows the typed
 * value while saving and the saved one again if the save fails, so a refused value never stays on screen.
 */
@Component({
  selector: 'fg-admin-settings',
  standalone: true,
  imports: [FormsModule, NgTemplateOutlet, PageHeader, PageLayout, NavTab, NavTabs, RouterLink, Card, CardHeader, Alert, Button, GbtInput, Icon, SaveStatus, SegmentedControl, Skeleton, Slider, Switch, SmtpSettings],
  templateUrl: './admin-settings.html',
  styleUrl: './admin-settings.scss',
})
export class AdminSettings implements OnInit {
  private settings = inject(SettingsService);
  private pageTitle = inject(PageTitleService);
  private destroyRef = inject(DestroyRef);
  private toast = inject(GbtToastService);
  private route = inject(ActivatedRoute);

  protected readonly sections = SECTIONS.map((section) => ({
    ...section,
    queryParams: section.key === DEFAULT_SECTION ? undefined : { section: section.key },
  }));

  private requestedSection = toSignal(this.route.queryParamMap.pipe(map((params) => params.get('section'))), { initialValue: null });

  protected activeSection = computed<AdminSettingsSectionKey>(() => {
    const requested = this.requestedSection();
    return SECTIONS.find((section) => section.key === requested)?.key ?? DEFAULT_SECTION;
  });

  protected current = signal<SystemSettings | null>(null);
  protected loadState = signal<'loading' | 'loaded' | 'failed'>('loading');
  protected saveStates = signal<Partial<Record<Field, SaveState>>>({});
  protected jwtError = signal<string | null>(null);

  protected engineShown = linkedSignal<Engine>(() => this.current()?.executionEngine ?? 'docker-runners');
  protected namespaceShown = linkedSignal(() => this.namespaceDefault());
  protected storageClassShown = linkedSignal(() => this.storageClassDefault());
  protected jwtShown = linkedSignal(() => String(this.current()?.jwtTtlHours ?? ''));
  protected maxPushSizeShown = linkedSignal(() => this.current()?.maxPushSizeMb ?? 1);
  protected registrationShown = linkedSignal(() => this.current()?.registrationEnabled ?? false);
  protected publicPagesShown = linkedSignal(() => this.current()?.publicPagesEnabled ?? false);
  protected seoIndexingShown = linkedSignal(() => this.current()?.seoIndexingEnabled ?? false);
  private edited: Record<K8sField, boolean> = { k8sNamespace: false, k8sCacheStorageClass: false };

  protected readonly engineOptions = ENGINE_OPTIONS;
  protected readonly maxPushSizeFormat = (mb: number): string => `${mb} Mio`;
  protected readonly skeletonCards = [
    { title: '9rem', help: '65%' },
    { title: '7rem', help: '50%' },
  ];
  // `gbt-slider` emits `ngModelChange` on every drag tick, unlike `GbtInput`'s `committed`. Debounce so there is one PUT once dragging settles.
  protected maxPushSizeMbInput$ = new Subject<number>();

  constructor() {
    this.maxPushSizeMbInput$.pipe(debounceTime(400), takeUntilDestroyed(this.destroyRef)).subscribe((mb) => this.setMaxPushSizeMb(mb));
  }

  ngOnInit(): void {
    this.pageTitle.set("Réglages de l'instance");
    this.load();
  }

  protected load(): void {
    this.loadState.set('loading');
    this.edited = { k8sNamespace: false, k8sCacheStorageClass: false };
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

  setEngine(engine: Engine): void {
    if (!this.current() || engine === this.engineShown()) {
      return;
    }
    this.engineShown.set(engine);
    this.save('executionEngine', { executionEngine: engine }, () => this.engineShown.set(this.current()?.executionEngine ?? 'docker-runners'));
  }

  /** Called on blur (see `GbtInput.committed`); the unchanged-value guard skips the PUT for a blur that edited nothing. */
  setJwtTtlHours(value: string): void {
    const trimmed = value.trim();
    const hours = Number(trimmed);
    if (!/^\d+$/.test(trimmed) || !Number.isSafeInteger(hours) || hours < 1) {
      this.jwtError.set(JWT_ERROR);
      return;
    }
    this.jwtError.set(null);
    if (hours === this.current()?.jwtTtlHours) {
      return;
    }
    this.jwtShown.set(String(hours));
    this.save('jwtTtlHours', { jwtTtlHours: hours }, () => this.jwtShown.set(String(this.current()?.jwtTtlHours ?? '')));
  }

  protected onMaxPushSizeInput(mb: number): void {
    this.maxPushSizeShown.set(mb);
    this.maxPushSizeMbInput$.next(mb);
  }

  setMaxPushSizeMb(mb: number): void {
    if (!Number.isFinite(mb) || mb <= 0 || mb === this.current()?.maxPushSizeMb) {
      return;
    }
    this.maxPushSizeShown.set(mb);
    this.save('maxPushSizeMb', { maxPushSizeMb: mb }, () => this.maxPushSizeShown.set(this.current()?.maxPushSizeMb ?? 1));
  }

  setRegistrationEnabled(enabled: boolean): void {
    this.setSwitch('registrationEnabled', this.registrationShown, enabled);
  }

  setPublicPagesEnabled(enabled: boolean): void {
    this.setSwitch('publicPagesEnabled', this.publicPagesShown, enabled);
  }

  setSeoIndexingEnabled(enabled: boolean): void {
    this.setSwitch('seoIndexingEnabled', this.seoIndexingShown, enabled);
  }

  private setSwitch(field: SwitchField, shown: WritableSignal<boolean>, enabled: boolean): void {
    if (!this.current() || enabled === shown()) {
      return;
    }
    shown.set(enabled);
    this.save(field, { [field]: enabled }, () => shown.set(this.current()?.[field] ?? false));
  }

  /**
   * Saved on blur only when edited (`markEdited`), so tabbing through a field pre-filled from the cluster saves
   * nothing. Saving the detected StorageClass is how the admin confirms that it supports ReadWriteMany.
   */
  protected markEdited(field: K8sField): void {
    this.edited[field] = true;
  }

  setK8sNamespace(value: string): void {
    this.commitK8sField('k8sNamespace', value, this.namespaceShown, () => this.namespaceDefault());
  }

  setK8sCacheStorageClass(value: string): void {
    this.commitK8sField('k8sCacheStorageClass', value, this.storageClassShown, () => this.storageClassDefault());
  }

  private commitK8sField(field: K8sField, value: string, shown: WritableSignal<string>, savedText: () => string): void {
    if (!this.edited[field]) {
      return;
    }
    this.edited[field] = false;
    const trimmed = value.trim();
    const next = trimmed === '' ? null : trimmed;
    if (next === this.current()?.[field]) {
      return;
    }
    shown.set(trimmed);
    this.save(field, { [field]: next }, () => shown.set(savedText()));
  }

  private namespaceDefault(): string {
    const s = this.current();
    return s ? configuredOrDetected(s.k8sNamespace, s.detectedK8sNamespace) : '';
  }

  private storageClassDefault(): string {
    const s = this.current();
    return s ? configuredOrDetected(s.k8sCacheStorageClass, s.detectedK8sDefaultStorageClass) : '';
  }

  private save(field: Field, update: SystemSettingsUpdate, rollback: () => void): void {
    this.setSaveState(field, 'saving');
    this.settings.updateAdmin(update).subscribe({
      next: (s) => {
        this.current.set(s);
        this.setSaveState(field, 'saved');
      },
      error: () => {
        rollback();
        this.setSaveState(field, 'error');
        this.toast.show("Échec de l'enregistrement. Réessayez.", 'error');
      },
    });
  }

  private setSaveState(field: Field, state: SaveState): void {
    this.saveStates.update((states) => ({ ...states, [field]: state }));
  }
}
