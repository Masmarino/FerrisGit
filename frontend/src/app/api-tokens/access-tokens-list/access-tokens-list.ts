import { afterNextRender, Component, computed, ElementRef, inject, Injector, OnInit, signal, viewChild } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Alert } from '@masmarino/gabarit/alert';
import { Badge } from '@masmarino/gabarit/badge';
import { Button } from '@masmarino/gabarit/button';
import { Card } from '@masmarino/gabarit/card';
import { ConfirmDangerModal } from '@masmarino/gabarit/confirm-danger-modal';
import { CopyField } from '@masmarino/gabarit/copy-field';
import { EmptyState } from '@masmarino/gabarit/empty-state';
import { GbtDateTimePipe, GbtRelativeTimePipe } from '@masmarino/gabarit/format';
import { Icon } from '@masmarino/gabarit/icon';
import { GbtInput } from '@masmarino/gabarit/input';
import { ListRow } from '@masmarino/gabarit/list-row';
import { SegmentedControl, SegmentedControlOption } from '@masmarino/gabarit/segmented-control';
import { SkeletonList } from '@masmarino/gabarit/skeleton-list';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { TranslocoPipe } from '@jsverse/transloco';
import { AccessTokensService, AccessTokenSummary } from '../access-tokens.service';
import { areaHint, areaName, describeScope, SCOPE_AREAS, ScopeArea, ScopeChoice, scopesFromChoices } from '../token-scopes';
import { MeService } from '../../shell/me.service';
import { isAbsoluteDate } from '../../shared/i18n/dated';
import { activeLocale, t, tn } from '../../shared/i18n/translator';

interface RevealedToken {
  id: string;
  name: string;
  token: string;
}

type LifetimeChoice = number | 'custom';

const RELATIVE_OPTIONS = { style: 'short', maxUnit: 'day', absoluteAfterDays: 30 } as const;
const NAME_MAX = 100;
const LIFETIME_MIN = 1;
const LIFETIME_MAX = 365;
const LIFETIME_PRESETS = [7, 30, 90, LIFETIME_MAX];
const DEFAULT_LIFETIME = 90;
const NO_SCOPES: Record<ScopeArea, ScopeChoice> = { repositories: 'none', issues: 'none', 'merge-requests': 'none', pipelines: 'none', releases: 'none', admin: 'none' };

/** The token name the server would refuse: too long, or with a control character it can't show. */
const CONTROL_CHARACTER = /\p{Cc}/u;

@Component({
  selector: 'fg-access-tokens-list',
  standalone: true,
  imports: [TranslocoPipe, FormsModule, Alert, EmptyState, Badge, Button, GbtInput, Icon, SkeletonList, CopyField, ConfirmDangerModal, ListRow, Card, SegmentedControl, GbtRelativeTimePipe, GbtDateTimePipe],
  templateUrl: './access-tokens-list.html',
  styleUrl: './access-tokens-list.scss',
})
export class AccessTokensList implements OnInit {
  protected readonly locale = activeLocale;
  private tokens = inject(AccessTokensService);
  private me = inject(MeService);
  private toast = inject(GbtToastService);
  private injector = inject(Injector);

  private revealCard = viewChild('revealCard', { read: ElementRef });
  private listCard = viewChild.required('listCard', { read: ElementRef });
  private nameField = viewChild.required('nameField', { read: ElementRef });
  private focusAfterRevoke: number | null = null;

  protected readonly nameMax = NAME_MAX;
  protected readonly lifetimeMin = LIFETIME_MIN;
  protected readonly lifetimeMax = LIFETIME_MAX;

  /** "Created on 12/08/2026" but "Created 3 days ago": the "on" only goes before an absolute date. */
  protected readonly on = (iso: string) => (isAbsoluteDate(iso, undefined, RELATIVE_OPTIONS.absoluteAfterDays) ? t('common.dateOn') : '');
  protected readonly scopeLabel = (scope: string) => describeScope(scope).label;

  protected list = signal<AccessTokenSummary[]>([]);
  protected listState = signal<'loading' | 'loaded' | 'failed'>('loading');
  protected newTokenName = signal('');
  protected choices = signal(NO_SCOPES);
  protected lifetime = signal<LifetimeChoice>(DEFAULT_LIFETIME);
  protected customLifetime = signal('');
  protected creating = signal(false);
  protected revealed = signal<RevealedToken | null>(null);
  protected tokenPendingRevoke = signal<AccessTokenSummary | null>(null);
  protected revoking = signal(false);

  protected areas = computed(() => SCOPE_AREAS.filter((area) => area !== 'admin' || this.me.isAdmin()).map((area) => ({ area, name: areaName(area), hint: areaHint(area) })));
  protected levelOptions = computed<SegmentedControlOption<ScopeChoice>[]>(() => [
    { value: 'none', label: t('accessTokens.scopes.none') },
    { value: 'read', label: t('accessTokens.scopes.read') },
    { value: 'write', label: t('accessTokens.scopes.write') },
  ]);
  protected lifetimeOptions = computed<SegmentedControlOption<LifetimeChoice>[]>(() => [
    ...LIFETIME_PRESETS.map((days) => ({ value: days, label: tn('accessTokens.days', days) })),
    { value: 'custom' as const, label: t('accessTokens.lifetimeCustom') },
  ]);

  private scopes = computed(() => scopesFromChoices(this.areas().map(({ area }) => area), this.choices()));
  private lifetimeDays = computed(() => {
    const choice = this.lifetime();
    if (choice !== 'custom') {
      return choice;
    }
    const raw = String(this.customLifetime()).trim();
    const days = Number(raw);
    return /^\d+$/.test(raw) && days >= LIFETIME_MIN && days <= LIFETIME_MAX ? days : null;
  });

  protected nameError = computed(() => {
    const name = this.newTokenName();
    return CONTROL_CHARACTER.test(name) || name.trim().length > NAME_MAX ? t('accessTokens.nameInvalid', { max: NAME_MAX }) : null;
  });
  /** Only once something is typed: an empty field disables the button, no need to scold on top. */
  protected lifetimeError = computed(() =>
    this.lifetime() === 'custom' && String(this.customLifetime()).trim() !== '' && this.lifetimeDays() === null ? t('accessTokens.lifetimeInvalid', { min: LIFETIME_MIN, max: LIFETIME_MAX }) : null,
  );
  protected noScope = computed(() => this.scopes().length === 0);
  protected canCreate = computed(() => !!this.newTokenName().trim() && !this.nameError() && !this.noScope() && this.lifetimeDays() !== null);

  ngOnInit(): void {
    this.refresh();
  }

  refresh(): void {
    this.tokens.list().subscribe({
      next: (list) => {
        this.list.set(list);
        this.listState.set('loaded');
        if (this.focusAfterRevoke !== null) {
          const index = this.focusAfterRevoke;
          this.focusAfterRevoke = null;
          afterNextRender(() => this.focusInList(index), { injector: this.injector });
        }
      },
      error: () => {
        if (this.listState() !== 'loaded') {
          this.listState.set('failed');
        }
        this.toast.show(t('apiTokens.list.toastLoadFailed'), 'error');
      },
    });
  }

  protected retry(): void {
    this.listState.set('loading');
    this.refresh();
  }

  protected choose(area: ScopeArea, choice: ScopeChoice): void {
    this.choices.update((choices) => ({ ...choices, [area]: choice }));
  }

  create(): void {
    const days = this.lifetimeDays();
    if (!this.canCreate() || days === null || this.creating()) {
      return;
    }
    this.creating.set(true);
    this.tokens.create({ name: this.newTokenName().trim(), scopes: this.scopes(), expiresInDays: days }).subscribe({
      next: (res) => {
        this.creating.set(false);
        this.revealed.set({ id: res.id, name: res.name, token: res.token });
        this.newTokenName.set('');
        this.choices.set(NO_SCOPES);
        this.lifetime.set(DEFAULT_LIFETIME);
        this.customLifetime.set('');
        this.refresh();
        this.toast.show(t('apiTokens.list.toastGenerated'));
        // Emptying the form disables the focused button, so focus moves to the token, shown only once.
        afterNextRender(() => this.focusCard(this.revealCard()), { injector: this.injector });
      },
      error: () => {
        this.creating.set(false);
        this.toast.show(t('apiTokens.list.toastGenerateFailed'), 'error');
      },
    });
  }

  protected dismissReveal(): void {
    this.revealed.set(null);
    afterNextRender(() => this.nameField().nativeElement.querySelector('input')?.focus(), { injector: this.injector });
  }

  protected confirmRevoke(token: AccessTokenSummary): void {
    this.tokenPendingRevoke.set(token);
  }

  revoke(id: string): void {
    if (this.revoking()) {
      return;
    }
    this.revoking.set(true);
    const index = this.list().findIndex((token) => token.id === id);
    this.tokens.revoke(id).subscribe({
      next: () => {
        // The dialog gives focus back to the row's button, which the refreshed list removes.
        this.focusAfterRevoke = Math.max(index, 0);
        this.revoking.set(false);
        this.tokenPendingRevoke.set(null);
        if (this.revealed()?.id === id) {
          this.revealed.set(null);
        }
        this.toast.show(t('apiTokens.list.toastRevoked'));
        this.refresh();
      },
      // The confirmation covers the page, so an error has to close it first to be visible.
      error: () => {
        this.revoking.set(false);
        this.tokenPendingRevoke.set(null);
        this.toast.show(t('apiTokens.list.toastRevokeFailed'), 'error');
      },
    });
  }

  private focusInList(index: number): void {
    const card = this.listCard().nativeElement as HTMLElement;
    const buttons = card.querySelectorAll<HTMLButtonElement>('.access-tokens-list__revoke button');
    const next = buttons[Math.min(index, buttons.length - 1)];
    if (next) {
      next.focus();
    } else {
      this.focusCard(this.listCard());
    }
  }

  /** `gbt-card` has no labelled region to focus, so the heading takes the focus, from script only (`tabindex="-1"`). */
  private focusCard(host: ElementRef | undefined): void {
    const heading = (host?.nativeElement as HTMLElement | undefined)?.querySelector<HTMLElement>('h2');
    if (heading) {
      heading.tabIndex = -1;
      heading.focus();
    }
  }
}
