import { afterNextRender, Component, computed, ElementRef, inject, Injector, OnInit, signal, viewChild } from '@angular/core';
import { FormsModule } from '@angular/forms';
import {
  Alert,
  Badge,
  BadgeVariant,
  Button,
  Card,
  CardHeader,
  EmptyState,
  GbtInput,
  GbtToastService,
  Icon,
  IconMarker,
  ListCard,
  ListRow,
  Modal,
  PageHeader,
  PageLayout,
  Panel,
  SecretReveal,
  SkeletonList,
  Tag,
  TagInput,
  formatDateTime,
  formatRelativeTime,
} from '@masmarino/gabarit';
import { RunnerSummary, RunnersService } from '../runners.service';
import { PageTitleService } from '../../shell/page-title.service';

const RELATIVE_OPTIONS = { style: 'short', maxUnit: 'day', absoluteAfterDays: 30 } as const;
const ABSOLUTE_OPTIONS = { day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' } as const;

/**
 * Every authenticated request records a heartbeat (a runner polls every 5 s by default), so two minutes of silence means
 * stopped or unreachable, with room for a slow network or a longer poll interval. The API has no explicit status.
 */
export const ONLINE_THRESHOLD_MS = 2 * 60 * 1000;

export type RunnerConnectivity = 'online' | 'offline' | 'never';

/** Online below the threshold, which includes a heartbeat slightly in the future (clock skew). */
export function runnerConnectivity(lastHeartbeatAt: string | null, now: Date = new Date()): RunnerConnectivity {
  if (!lastHeartbeatAt) {
    return 'never';
  }
  return now.getTime() - new Date(lastHeartbeatAt).getTime() < ONLINE_THRESHOLD_MS ? 'online' : 'offline';
}

interface ConnectivityPresentation {
  label: string;
  variant: BadgeVariant;
  icon: string;
}

const CONNECTIVITY: Record<RunnerConnectivity, ConnectivityPresentation> = {
  online: { label: 'En ligne', variant: 'success', icon: 'circle-check' },
  offline: { label: 'Hors ligne', variant: 'neutral', icon: 'circle-slash' },
  never: { label: 'Jamais connecté', variant: 'neutral', icon: 'clock' },
};

interface RowDate {
  iso: string;
  label: string;
  title: string;
}

interface RunnerRow {
  runner: RunnerSummary;
  connectivity: RunnerConnectivity;
  presentation: ConnectivityPresentation;
  heartbeat: RowDate | null;
  created: RowDate;
}

function rowDate(iso: string, now: Date): RowDate {
  const relative = formatRelativeTime(iso, 'fr', now, RELATIVE_OPTIONS);
  return { iso, label: /^\d/.test(relative) ? `le ${relative}` : relative, title: formatDateTime(iso, 'fr', ABSOLUTE_OPTIONS) };
}

/** "En ligne" is derived from the last heartbeat when the list loads; the page doesn't poll. */
@Component({
  selector: 'fg-runners-list',
  standalone: true,
  imports: [
    FormsModule,
    PageHeader,
    PageLayout,
    Panel,
    ListRow,
    Alert,
    Badge,
    Button,
    Card,
    CardHeader,
    EmptyState,
    GbtInput,
    Icon,
    IconMarker,
    ListCard,
    Modal,
    SecretReveal,
    SkeletonList,
    Tag,
    TagInput,
  ],
  templateUrl: './runners-list.html',
  styleUrl: './runners-list.scss',
})
export class RunnersList implements OnInit {
  private runners = inject(RunnersService);
  private pageTitle = inject(PageTitleService);
  private toast = inject(GbtToastService);
  private injector = inject(Injector);

  protected list = signal<RunnerSummary[]>([]);
  protected loaded = signal(false);
  protected loadFailed = signal(false);
  protected readonly onlineThresholdMinutes = ONLINE_THRESHOLD_MS / 60_000;

  protected rows = computed<RunnerRow[]>(() => {
    const now = new Date();
    return this.list().map((runner) => {
      const connectivity = runnerConnectivity(runner.lastHeartbeatAt, now);
      return {
        runner,
        connectivity,
        presentation: CONNECTIVITY[connectivity],
        heartbeat: runner.lastHeartbeatAt ? rowDate(runner.lastHeartbeatAt, now) : null,
        created: rowDate(runner.createdAt, now),
      };
    });
  });

  protected summary = computed(() => {
    const rows = this.rows();
    if (rows.length === 0) {
      return null;
    }
    const online = rows.filter((row) => row.connectivity === 'online').length;
    return `${rows.length} ${rows.length === 1 ? 'runner' : 'runners'} · ${online} en ligne`;
  });

  protected registerOpen = signal(false);
  protected newRunnerName = signal('');
  protected newRunnerTags = signal<string[]>([]);
  protected nameError = signal<string | null>(null);
  protected registerError = signal(false);
  protected registering = signal(false);
  protected readonly removeTagLabel = (tag: string) => `Retirer le tag ${tag}`;

  protected revealed = signal<{ name: string; token: string } | null>(null);
  private tokenCard = viewChild<ElementRef<HTMLElement>>('tokenCard');

  ngOnInit(): void {
    this.pageTitle.set('Runners');
    this.refresh();
  }

  refresh(): void {
    this.runners.list().subscribe({
      next: (list) => {
        this.list.set(list);
        this.loadFailed.set(false);
        this.loaded.set(true);
      },
      error: () => {
        this.loadFailed.set(true);
        this.loaded.set(true);
        this.toast.show('Impossible de charger les runners. Réessayez plus tard.', 'error');
      },
    });
  }

  protected retryLoad(): void {
    this.loaded.set(false);
    this.loadFailed.set(false);
    this.refresh();
  }

  protected openRegister(): void {
    this.registerOpen.set(true);
  }

  protected closeRegister(): void {
    this.registerOpen.set(false);
    this.newRunnerName.set('');
    this.newRunnerTags.set([]);
    this.nameError.set(null);
    this.registerError.set(false);
  }

  register(): void {
    if (this.registering()) {
      return;
    }
    const name = this.newRunnerName().trim();
    if (!name) {
      this.nameError.set('Donnez un nom au runner');
      return;
    }
    this.nameError.set(null);
    this.registerError.set(false);
    this.registering.set(true);
    this.runners.register(name, this.newRunnerTags()).subscribe({
      next: (res) => {
        this.registering.set(false);
        this.revealed.set({ name: res.name, token: res.token });
        this.closeRegister();
        this.toast.show('Runner enregistré.');
        this.refresh();
        // The dialog gives focus back to its opener, so move it on to the token, the next thing to do.
        afterNextRender(() => this.tokenCard()?.nativeElement.focus(), { injector: this.injector });
      },
      error: () => {
        this.registering.set(false);
        this.registerError.set(true);
        this.toast.show("Impossible d'enregistrer le runner.", 'error');
      },
    });
  }

  protected dismissToken(): void {
    this.revealed.set(null);
  }
}
