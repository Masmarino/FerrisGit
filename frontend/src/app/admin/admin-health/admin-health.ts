import { Component, computed, inject, OnInit, signal } from '@angular/core';
import { Alert } from '@masmarino/gabarit/alert';
import { Badge, BadgeVariant } from '@masmarino/gabarit/badge';
import { Button } from '@masmarino/gabarit/button';
import { Card, CardHeader } from '@masmarino/gabarit/card';
import { DescriptionList, DescriptionListEntry } from '@masmarino/gabarit/description-list';
import { formatBytes, formatDateTime, formatDuration, formatPercent } from '@masmarino/gabarit/format';
import { GaugeBar } from '@masmarino/gabarit/gauge-bar';
import { Icon } from '@masmarino/gabarit/icon';
import { PageHeader } from '@masmarino/gabarit/page-header';
import { PageLayout } from '@masmarino/gabarit/page-layout';
import { Panel } from '@masmarino/gabarit/panel';
import { Skeleton } from '@masmarino/gabarit/skeleton';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { AdminMetricsService, HealthStatus } from '../admin-metrics.service';
import { PageTitleService } from '../../shell/page-title.service';
import { ABSOLUTE_OPTIONS } from '../row-date';

const bytes = (value: number) => formatBytes(value, 'fr', { binaryUnits: 'legacy' });

interface StatusBadge {
  label: string;
  variant: BadgeVariant;
  icon: string;
}

interface Gauge {
  label: string;
  value: number;
  max: number;
  formatted: string;
}

interface HealthCard {
  key: string;
  heading: string;
  icon: string;
  status: StatusBadge;
  detail: string | null;
  facts: DescriptionListEntry[];
  gauge: Gauge | null;
}

const UP_ICON = 'circle-check';
const DOWN_ICON = 'circle-x';

const componentStatus = (status: 'up' | 'down', upLabel: string): StatusBadge =>
  status === 'up' ? { label: upLabel, variant: 'success', icon: UP_ICON } : { label: 'Indisponible', variant: 'error', icon: DOWN_ICON };

@Component({
  selector: 'fg-admin-health',
  standalone: true,
  imports: [PageHeader, PageLayout, Panel, Alert, Badge, Button, Card, CardHeader, DescriptionList, GaugeBar, Icon, Skeleton],
  templateUrl: './admin-health.html',
  styleUrl: './admin-health.scss',
})
export class AdminHealth implements OnInit {
  private metrics = inject(AdminMetricsService);
  private pageTitle = inject(PageTitleService);
  private toast = inject(GbtToastService);

  protected health = signal<HealthStatus | null>(null);
  // Different from `health()?.database.status === 'down'`, which comes from a successful response: this is the request
  // itself failing, e.g. Postgres down (the AdminUser extractor queries it before the handler runs, so the request 500s).
  protected loadError = signal(false);
  protected checking = signal(false);
  protected checkedAt = signal<Date | null>(null);
  protected readonly skeletonCards = [0, 1];
  protected readonly gaugeThresholds = { warning: 80, critical: 95 };

  protected cards = computed<HealthCard[]>(() => {
    const health = this.health();
    if (!health) {
      return [];
    }
    const { database, storage } = health;
    return [
      {
        key: 'database',
        heading: 'Base de données',
        icon: 'database',
        status: componentStatus(database.status, 'Opérationnelle'),
        detail: database.detail,
        facts:
          database.status === 'up'
            ? [
                { term: 'Version', value: database.serverVersion ? `PostgreSQL ${database.serverVersion}` : 'Inconnue' },
                { term: 'Temps de réponse', value: `${database.responseTimeMs} ms` },
              ]
            : [],
        gauge:
          database.status === 'up'
            ? { label: 'Connexions actives', value: database.activeConnections, max: database.maxConnections, formatted: `${database.activeConnections} sur ${database.maxConnections}` }
            : null,
      },
      {
        key: 'storage',
        heading: 'Stockage',
        icon: 'hard-drive',
        status: componentStatus(storage.status, 'Opérationnel'),
        detail: storage.detail,
        facts:
          storage.status === 'up'
            ? [
                { term: 'Utilisé', value: bytes(storage.usedBytes) },
                { term: 'Libre', value: bytes(storage.freeBytes) },
                { term: 'Total', value: bytes(storage.totalBytes) },
              ]
            : [],
        gauge:
          storage.status === 'up'
            ? {
                label: 'Espace utilisé',
                value: storage.usedBytes,
                max: storage.totalBytes,
                formatted: `${bytes(storage.usedBytes)} sur ${bytes(storage.totalBytes)} · ${formatPercent(storage.totalBytes > 0 ? storage.usedBytes / storage.totalBytes : 0, 'fr')}`,
              }
            : null,
      },
    ];
  });

  protected overall = computed<StatusBadge | null>(() => {
    if (this.loadError()) {
      return { label: 'Injoignable', variant: 'error', icon: DOWN_ICON };
    }
    const health = this.health();
    if (!health) {
      return null;
    }
    return health.database.status === 'up' && health.storage.status === 'up'
      ? { label: 'Tous les services sont opérationnels', variant: 'success', icon: UP_ICON }
      : { label: 'Service dégradé', variant: 'error', icon: 'alert-triangle' };
  });

  protected uptime = computed(() => {
    const health = this.health();
    const checkedAt = this.checkedAt();
    if (!health || !checkedAt) {
      return null;
    }
    const startedAt = new Date(checkedAt.getTime() - health.uptimeSeconds * 1000).toISOString();
    return { value: formatDuration(health.uptimeSeconds * 1000, 'fr', { days: true }), startedAt: formatDateTime(startedAt, 'fr', ABSOLUTE_OPTIONS) };
  });

  protected checkedAtLabel = computed(() => {
    const checkedAt = this.checkedAt();
    if (!checkedAt) {
      return null;
    }
    const pad = (value: number) => String(value).padStart(2, '0');
    return `${pad(checkedAt.getHours())}:${pad(checkedAt.getMinutes())}:${pad(checkedAt.getSeconds())}`;
  });

  ngOnInit(): void {
    this.pageTitle.set('Santé');
    this.refresh();
  }

  protected refresh(): void {
    this.checking.set(true);
    this.metrics.getHealth().subscribe({
      next: (h) => {
        this.health.set(h);
        this.checkedAt.set(new Date());
        this.loadError.set(false);
        this.checking.set(false);
      },
      error: () => {
        this.loadError.set(true);
        this.checking.set(false);
        this.toast.show("Impossible de charger l'état de santé. Réessayez plus tard.", 'error');
      },
    });
  }
}
