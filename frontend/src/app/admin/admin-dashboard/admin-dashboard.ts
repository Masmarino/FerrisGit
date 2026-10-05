import { Component, computed, DestroyRef, inject, OnInit, signal } from '@angular/core';
import { NgTemplateOutlet } from '@angular/common';
import { Subscription } from 'rxjs';
import { Alert } from '@masmarino/gabarit/alert';
import { Button } from '@masmarino/gabarit/button';
import { EmptyState } from '@masmarino/gabarit/empty-state';
import { formatBytes } from '@masmarino/gabarit/format';
import { Icon } from '@masmarino/gabarit/icon';
import { LineChart } from '@masmarino/gabarit/line-chart';
import { ListCard } from '@masmarino/gabarit/list-card';
import { PageHeader } from '@masmarino/gabarit/page-header';
import { PageLayout } from '@masmarino/gabarit/page-layout';
import { SegmentedControl, SegmentedControlOption } from '@masmarino/gabarit/segmented-control';
import { Skeleton } from '@masmarino/gabarit/skeleton';
import { StatGrid } from '@masmarino/gabarit/stat-grid';
import { StatTile } from '@masmarino/gabarit/stat-tile';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import type { ChartSeries } from '@masmarino/gabarit/chart';
import { AdminMetricsService, AdminStats, MetricsSnapshot } from '../admin-metrics.service';
import { PageTitleService } from '../../shell/page-title.service';

const BYTE_UNITS = ['Ko', 'Mo', 'Go', 'To'];

/** Gabarit's formatBytes only returns the joined string, not the divisor a chart series needs, so the y-axis unit is computed here. */
function byteUnit(bytes: number): { divisor: number; label: string } {
  let divisor = 1;
  let label = 'o';
  for (const unit of BYTE_UNITS) {
    if (bytes < divisor * 1024) {
      break;
    }
    divisor *= 1024;
    label = unit;
  }
  return { divisor, label };
}

const DAY_OPTIONS: SegmentedControlOption<number>[] = [
  { value: 1, label: '1 jour' },
  { value: 3, label: '3 jours' },
  { value: 7, label: '7 jours' },
  { value: 30, label: '30 jours' },
];

interface DashboardTile {
  key: string;
  label: string;
  value: string;
  hint: string | null;
  icon: string;
}

type LoadState = 'loading' | 'loaded' | 'failed';

/** The server snapshots every hour and a line needs two points, so with fewer the chart says "not enough readings yet". */
@Component({
  selector: 'fg-admin-dashboard',
  standalone: true,
  imports: [NgTemplateOutlet, PageHeader, PageLayout, Alert, EmptyState, LineChart, ListCard, SegmentedControl, Button, Icon, Skeleton, StatGrid, StatTile],
  templateUrl: './admin-dashboard.html',
  styleUrl: './admin-dashboard.scss',
})
export class AdminDashboard implements OnInit {
  private metrics = inject(AdminMetricsService);
  private pageTitle = inject(PageTitleService);
  private toast = inject(GbtToastService);
  // Gabarit's charts take their own `locale` input, they don't read LOCALE_ID.
  protected readonly locale = 'fr-FR';

  protected stats = signal<AdminStats | null>(null);
  protected statsState = signal<LoadState>('loading');
  /** Null until the first history response. Skeletons show only then, not on a period change. */
  protected history = signal<MetricsSnapshot[] | null>(null);
  protected historyState = signal<LoadState>('loading');
  protected historyDays = signal(30);
  protected readonly dayOptions = DAY_OPTIONS;
  protected readonly storagePane = { $implicit: 'storage' } as const;
  protected readonly countsPane = { $implicit: 'counts' } as const;

  private latest = computed(() => {
    const history = this.history();
    return history && history.length > 0 ? history[history.length - 1] : null;
  });

  protected tiles = computed<DashboardTile[]>(() => {
    const stats = this.stats();
    const count = (value: number | undefined) => (value === undefined ? '—' : String(value));
    const latest = this.latest();
    return [
      { key: 'users', label: 'Utilisateurs', value: count(stats?.totalUsers), hint: null, icon: 'user' },
      { key: 'repositories', label: 'Dépôts', value: count(stats?.totalRepositories), hint: null, icon: 'folder-git-2' },
      { key: 'pipelines', label: 'Pipelines', value: count(stats?.pipelinesLast7Days), hint: '7 derniers jours', icon: 'play' },
      {
        key: 'storage',
        label: 'Stockage',
        value: latest ? formatBytes(latest.totalStorageBytes, 'fr', { binaryUnits: 'legacy' }) : '—',
        hint: latest ? 'dernier relevé' : 'aucun relevé',
        icon: 'hard-drive',
      },
    ];
  });

  protected hasEnoughReadings = computed(() => (this.history()?.length ?? 0) >= 2);

  protected storageUnit = computed(() => byteUnit(Math.max(0, ...(this.history() ?? []).map((h) => h.totalStorageBytes))));

  protected storageSeries = computed<ChartSeries<Date>[]>(() => {
    const history = this.history() ?? [];
    const { divisor, label } = this.storageUnit();
    return [{ label: `Stockage (${label})`, points: history.map((h) => ({ x: new Date(h.recordedAt), y: h.totalStorageBytes / divisor })) }];
  });

  protected countsSeries = computed<ChartSeries<Date>[]>(() => {
    const history = this.history() ?? [];
    return [
      { label: 'Utilisateurs', points: history.map((h) => ({ x: new Date(h.recordedAt), y: h.totalUsers })) },
      { label: 'Dépôts', points: history.map((h) => ({ x: new Date(h.recordedAt), y: h.totalRepositories })) },
    ];
  });

  private historyRequest: Subscription | null = null;

  constructor() {
    inject(DestroyRef).onDestroy(() => this.historyRequest?.unsubscribe());
  }

  ngOnInit(): void {
    this.pageTitle.set('Tableau de bord');
    this.metrics.getStats().subscribe({
      next: (s) => {
        this.stats.set(s);
        this.statsState.set('loaded');
      },
      error: () => {
        this.statsState.set('failed');
        this.toast.show('Impossible de charger les statistiques. Réessayez plus tard.', 'error');
      },
    });
    this.loadHistory();
  }

  setHistoryDays(days: number): void {
    this.historyDays.set(days);
    this.loadHistory();
  }

  protected loadHistory(): void {
    // Only the latest period counts: a slower response for an earlier one is dropped.
    this.historyRequest?.unsubscribe();
    this.historyState.set('loading');
    this.historyRequest = this.metrics.getHistory(this.historyDays()).subscribe({
      next: (h) => {
        this.history.set(h);
        this.historyState.set('loaded');
      },
      error: () => {
        this.historyState.set('failed');
        this.toast.show("Impossible de charger l'historique des métriques. Réessayez plus tard.", 'error');
      },
    });
  }
}
