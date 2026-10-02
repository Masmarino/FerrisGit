import { TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { Observable, of, Subject, throwError } from 'rxjs';
import { LineChart, GbtToastService } from '@masmarino/gabarit';
import { AdminDashboard } from './admin-dashboard';
import { AdminMetricsService, AdminStats, MetricsSnapshot } from '../admin-metrics.service';
import { PageTitleService } from '../../shell/page-title.service';

const STATS: AdminStats = { totalUsers: 3, totalRepositories: 5, pipelinesLast7Days: 12 };

function snapshot(daysAgo: number, fields: Partial<MetricsSnapshot> = {}): MetricsSnapshot {
  return {
    recordedAt: new Date(Date.UTC(2026, 8, 20 - daysAgo)).toISOString(),
    totalUsers: 2,
    totalRepositories: 4,
    totalStorageBytes: 1024 * 1024,
    ...fields,
  };
}

const HISTORY: MetricsSnapshot[] = [
  snapshot(2, { totalUsers: 1, totalRepositories: 2, totalStorageBytes: 1024 ** 3 }),
  snapshot(1, { totalUsers: 2, totalRepositories: 3, totalStorageBytes: 2 * 1024 ** 3 }),
  snapshot(0, { totalUsers: 3, totalRepositories: 5, totalStorageBytes: 3 * 1024 ** 3 }),
];

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();

describe('AdminDashboard', () => {
  function setup(overrides: { stats?: Observable<AdminStats>; history?: Observable<MetricsSnapshot[]> } = {}) {
    const metricsStub = {
      getStats: vi.fn(() => overrides.stats ?? of(STATS)),
      getHistory: vi.fn((_days: number) => overrides.history ?? of(HISTORY)),
    };
    const pageTitleStub = { set: vi.fn() };
    const toastStub = { show: vi.fn() };
    TestBed.configureTestingModule({
      providers: [
        { provide: AdminMetricsService, useValue: metricsStub },
        { provide: PageTitleService, useValue: pageTitleStub },
        { provide: GbtToastService, useValue: toastStub },
      ],
    });
    const fixture = TestBed.createComponent(AdminDashboard);
    const el = fixture.nativeElement as HTMLElement;
    return { fixture, el, metricsStub, pageTitleStub, toastStub };
  }

  const tiles = (el: HTMLElement) =>
    Array.from(el.querySelectorAll('gbt-stat-tile')).map((tile) => ({
      label: text(tile.querySelector('.gbt-stat-tile__label')),
      value: text(tile.querySelector('.gbt-stat-tile__value')),
      hint: text(tile.querySelector('.gbt-stat-tile__hint')) ?? null,
    }));

  it('sets the page title', () => {
    const { fixture, pageTitleStub } = setup();
    fixture.detectChanges();
    expect(pageTitleStub.set).toHaveBeenCalledWith('Tableau de bord');
  });

  it('is a wide page under a "Tableau de bord" h1', () => {
    const { fixture, el } = setup();
    fixture.detectChanges();

    expect(text(el.querySelector('gbt-page-header h1'))).toBe('Tableau de bord');
    expect(el.querySelector('gbt-page-layout')?.getAttribute('data-width')).toBe('wide');
  });

  describe('stat tiles', () => {
    it('shows the totals as four tiles: users, repositories, pipelines of the last 7 days, and the latest storage reading', () => {
      const { fixture, el } = setup();
      fixture.detectChanges();

      const grid = el.querySelector('gbt-stat-grid [role="list"]');
      expect(grid?.getAttribute('aria-label')).toBe('Résumé');
      expect(Array.from(el.querySelectorAll('gbt-stat-tile')).every((tile) => tile.getAttribute('role') === 'listitem' && grid?.contains(tile))).toBe(true);
      expect(tiles(el)).toEqual([
        { label: 'Utilisateurs', value: '3', hint: null },
        { label: 'Dépôts', value: '5', hint: null },
        { label: 'Pipelines', value: '12', hint: '7 derniers jours' },
        { label: 'Stockage', value: '3 Go', hint: 'dernier relevé' },
      ]);
      const markers = Array.from(el.querySelectorAll('gbt-stat-tile gbt-icon-marker .gbt-icon-marker'));
      expect(markers.map((marker) => marker.closest('gbt-stat-tile')?.getAttribute('data-key'))).toEqual(['users', 'repositories', 'pipelines', 'storage']);
      expect(markers.every((marker) => marker.getAttribute('aria-hidden') === 'true')).toBe(true);
    });

    it('shows tile skeletons while the stats load, then the tiles', () => {
      const pending = new Subject<AdminStats>();
      const { fixture, el } = setup({ stats: pending });
      fixture.detectChanges();

      const grid = el.querySelector('gbt-stat-grid')!;
      expect(grid.querySelector('[role="list"]')?.getAttribute('aria-hidden')).toBe('true');
      expect(grid.querySelectorAll('.gbt-stat-grid__placeholder')).toHaveLength(4);
      expect(text(grid.querySelector('[role="status"]'))).toBe('Chargement des statistiques…');
      expect(el.querySelectorAll('gbt-stat-tile')).toHaveLength(0);

      pending.next(STATS);
      fixture.detectChanges();
      expect(grid.querySelector('.gbt-stat-grid__placeholder')).toBeNull();
      expect(text(grid.querySelector('[role="status"]'))).toBe('');
      expect(grid.querySelector('[role="list"]')?.hasAttribute('aria-hidden')).toBe(false);
      expect(el.querySelector('[aria-busy="true"]')).toBeNull();
      expect(tiles(el).map((t) => t.value)).toEqual(['3', '5', '12', '3 Go']);
    });

    it('reads "—" for storage until a first snapshot exists', () => {
      const { fixture, el } = setup({ history: of([]) });
      fixture.detectChanges();

      expect(tiles(el)[3]).toEqual({ label: 'Stockage', value: '—', hint: 'aucun relevé' });
    });

    it('shows a toast when loading the stats fails, and "—" in the count tiles', () => {
      const { fixture, el, toastStub } = setup({ stats: throwError(() => new Error('boom')) });
      fixture.detectChanges();

      expect(toastStub.show).toHaveBeenCalledWith('Impossible de charger les statistiques. Réessayez plus tard.', 'error');
      expect(tiles(el).map((t) => t.value)).toEqual(['—', '—', '—', '3 Go']);
    });
  });

  describe('the evolution card', () => {
    const card = (el: HTMLElement) => el.querySelector<HTMLElement>('.admin-dashboard__evolution')!;
    const rangeOptions = (el: HTMLElement) => Array.from(card(el).querySelectorAll<HTMLButtonElement>('.admin-dashboard__band [role="radio"]'));

    it('is a section named by its h2, with the period control in its header', () => {
      const { fixture, el } = setup();
      fixture.detectChanges();

      const heading = card(el).querySelector('.admin-dashboard__band h2')!;
      expect(text(heading)).toBe('Évolution');
      expect(card(el).getAttribute('aria-labelledby')).toBe(heading.id);
      expect(card(el).querySelector('.admin-dashboard__band [role="radiogroup"]')?.getAttribute('aria-label')).toBe('Période');
      expect(rangeOptions(el).map((o) => text(o))).toEqual(['1 jour', '3 jours', '7 jours', '30 jours']);
      const section = card(el).querySelector('gbt-list-card > .gbt-list-card')!;
      const header = section.querySelector(':scope > .gbt-list-card__header')!;
      const box = section.querySelector(':scope > .gbt-list-card__box')!;
      expect(header.querySelector('.admin-dashboard__band h2')).toBe(heading);
      expect(box.querySelector('.admin-dashboard__band')).toBeNull();
      expect(header.compareDocumentPosition(box) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    });

    it('requests a 30-day history by default, the selected period', () => {
      const { fixture, el, metricsStub } = setup();
      fixture.detectChanges();

      expect(metricsStub.getHistory).toHaveBeenCalledWith(30);
      expect(rangeOptions(el).map((o) => o.getAttribute('aria-checked'))).toEqual(['false', 'false', 'false', 'true']);
    });

    it('re-fetches history with the new range when the day-range selector changes', () => {
      const { fixture, metricsStub } = setup();
      fixture.detectChanges();

      fixture.componentInstance.setHistoryDays(7);

      expect(metricsStub.getHistory).toHaveBeenCalledWith(7);
    });

    it('re-fetches history when another period is clicked in the header, and marks it selected', () => {
      const { fixture, el, metricsStub } = setup();
      fixture.detectChanges();

      rangeOptions(el)[2].click();
      fixture.detectChanges();

      expect(metricsStub.getHistory).toHaveBeenLastCalledWith(7);
      expect(rangeOptions(el).map((o) => o.getAttribute('aria-checked'))).toEqual(['false', 'false', 'true', 'false']);
    });

    it('ignores a slower answer for a previous period: the last period asked wins', () => {
      const thirtyDays = new Subject<MetricsSnapshot[]>();
      const sevenDays = new Subject<MetricsSnapshot[]>();
      const { fixture, metricsStub } = setup();
      metricsStub.getHistory.mockImplementation((days: number) => (days === 30 ? thirtyDays : sevenDays));
      fixture.detectChanges();

      fixture.componentInstance.setHistoryDays(7);
      sevenDays.next(HISTORY.slice(1));
      thirtyDays.next(HISTORY);
      fixture.detectChanges();

      const storage = fixture.debugElement.queryAll(By.directive(LineChart))[0].componentInstance as LineChart<Date>;
      expect(storage.series()[0].points).toHaveLength(2);
    });

    it('draws storage and users/repositories side by side, each under its h3; storage in a unit fitting its largest reading', () => {
      const { fixture, el } = setup();
      fixture.detectChanges();

      const panes = Array.from(card(el).querySelectorAll('.admin-dashboard__chart-pane'));
      expect(panes.map((pane) => text(pane.querySelector('h3')))).toEqual(['Stockage', 'Utilisateurs et dépôts']);
      const [storage, counts] = fixture.debugElement.queryAll(By.directive(LineChart)).map((de) => de.componentInstance as LineChart<Date>);
      expect(storage.series().map((s) => s.label)).toEqual(['Stockage (Go)']);
      expect(storage.series()[0].points.map((p) => p.y)).toEqual([1, 2, 3]);
      expect(counts.series().map((s) => s.label)).toEqual(['Utilisateurs', 'Dépôts']);
      expect(counts.series()[1].points.map((p) => p.y)).toEqual([2, 3, 5]);
    });

    it('keeps the same series objects between renders (no fresh array bound on each check)', () => {
      const { fixture } = setup();
      fixture.detectChanges();
      const before = fixture.debugElement.queryAll(By.directive(LineChart)).map((de) => (de.componentInstance as LineChart<Date>).series());
      fixture.detectChanges();
      const after = fixture.debugElement.queryAll(By.directive(LineChart)).map((de) => (de.componentInstance as LineChart<Date>).series());
      expect(after[0]).toBe(before[0]);
      expect(after[1]).toBe(before[1]);
    });

    it('explains, instead of an empty chart, that there are not enough readings yet (fewer than two)', () => {
      const { fixture, el } = setup({ history: of([HISTORY[0]]) });
      fixture.detectChanges();

      expect(fixture.debugElement.queryAll(By.directive(LineChart))).toHaveLength(0);
      const empties = Array.from(card(el).querySelectorAll('.admin-dashboard__chart-empty gbt-empty-state'));
      expect(empties).toHaveLength(2);
      expect(empties.every((e) => text(e)?.includes('Pas encore assez de mesures'))).toBe(true);
    });

    it('shows chart skeletons while the history loads the first time', () => {
      const pending = new Subject<MetricsSnapshot[]>();
      const { fixture, el } = setup({ history: pending });
      fixture.detectChanges();

      expect(card(el).querySelector('.admin-dashboard__charts[aria-busy="true"]')).not.toBeNull();
      // The polite status sits beside the busy charts, never inside them.
      expect(text(card(el).querySelector('.gbt-list-card__body [role="status"]'))).toBe("Chargement de l'historique…");
      expect(card(el).querySelector('[aria-busy="true"] [role="status"]')).toBeNull();
      expect(text(card(el).querySelector('h2'))).toBe('Évolution');
      expect(fixture.debugElement.queryAll(By.directive(LineChart))).toHaveLength(0);

      pending.next(HISTORY);
      fixture.detectChanges();
      expect(card(el).querySelector('[aria-busy="true"]')).toBeNull();
      expect(fixture.debugElement.queryAll(By.directive(LineChart))).toHaveLength(2);
      // Emptied, not removed: a screen reader needs the region in the DOM before its text changes.
      expect(text(card(el).querySelector('.gbt-list-card__body [role="status"]'))).toBe('');
    });

    it('shows a toast when loading the history fails, and a retry in the card that loads it again', () => {
      const { fixture, el, metricsStub, toastStub } = setup({ history: throwError(() => new Error('boom')) });
      fixture.detectChanges();

      expect(toastStub.show).toHaveBeenCalledWith("Impossible de charger l'historique des métriques. Réessayez plus tard.", 'error');
      const failed = card(el).querySelector('gbt-alert .gbt-alert');
      expect(text(failed)).toContain("L'historique n'a pas pu être chargé");
      // The toast announces it; the card stays silent with a retry button, so there's only one live region.
      expect(failed?.getAttribute('data-variant')).toBe('error');
      expect(failed?.getAttribute('role')).toBeNull();
      expect(failed?.getAttribute('aria-live')).toBeNull();
      expect(failed?.querySelector('button')?.textContent?.trim()).toBe('Réessayer');

      metricsStub.getHistory.mockReturnValue(of(HISTORY));
      const retry = Array.from(card(el).querySelectorAll('button')).find((b) => text(b) === 'Réessayer')!;
      retry.click();
      fixture.detectChanges();

      expect(metricsStub.getHistory).toHaveBeenCalledTimes(2);
      expect(card(el).querySelector('gbt-alert')).toBeNull();
      expect(fixture.debugElement.queryAll(By.directive(LineChart))).toHaveLength(2);
    });
  });
});
