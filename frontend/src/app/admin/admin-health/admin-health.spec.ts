import { TestBed } from '@angular/core/testing';
import { Observable, of, Subject, throwError } from 'rxjs';
import { AdminHealth } from './admin-health';
import { AdminMetricsService, HealthStatus } from '../admin-metrics.service';
import { PageTitleService } from '../../shell/page-title.service';
import { GbtToastService } from '@masmarino/gabarit';

const ALL_UP: HealthStatus = {
  database: { status: 'up', detail: null, responseTimeMs: 3, activeConnections: 2, maxConnections: 10, serverVersion: '18.0' },
  storage: { status: 'up', detail: null, usedBytes: 10, freeBytes: 90, totalBytes: 100 },
  uptimeSeconds: 3661,
};

const DATABASE_DOWN: HealthStatus = {
  database: { status: 'down', detail: 'connection refused', responseTimeMs: 0, activeConnections: 0, maxConnections: 10, serverVersion: null },
  storage: { status: 'up', detail: null, usedBytes: 10, freeBytes: 90, totalBytes: 100 },
  uptimeSeconds: 5,
};

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();

describe('AdminHealth', () => {
  function setup(health: HealthStatus | 'error' | Observable<HealthStatus> = ALL_UP) {
    const metricsStub = {
      getHealth: vi.fn(() => (health === 'error' ? throwError(() => new Error('boom')) : health instanceof Observable ? health : of(health))),
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
    const fixture = TestBed.createComponent(AdminHealth);
    const el = fixture.nativeElement as HTMLElement;
    return { fixture, el, metricsStub, pageTitleStub, toastStub };
  }

  const card = (el: HTMLElement, heading: string) =>
    Array.from(el.querySelectorAll<HTMLElement>('.admin-health__card')).find((c) => text(c.querySelector('h2')) === heading);
  const facts = (container: Element | undefined) => {
    const terms = Array.from(container?.querySelectorAll('dt') ?? []).map((dt) => text(dt));
    const values = Array.from(container?.querySelectorAll('dd') ?? []).map((dd) => text(dd));
    return Object.fromEntries(terms.map((term, i) => [term, values[i]]));
  };
  const button = (el: HTMLElement, label: string) => Array.from(el.querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === label);

  it('sets the page title', () => {
    const { fixture, pageTitleStub } = setup();
    fixture.detectChanges();
    expect(pageTitleStub.set).toHaveBeenCalledWith('Santé');
  });

  it('is a wide page under a "Santé" h1, with an overall status badge', () => {
    const { fixture, el } = setup();
    fixture.detectChanges();

    expect(text(el.querySelector('gbt-page-header h1'))).toBe('Santé');
    expect(el.querySelector('gbt-page-layout')?.getAttribute('data-width')).toBe('wide');
    const badge = el.querySelector('gbt-page-header gbt-badge .gbt-badge');
    expect(text(badge)).toBe('Tous les services sont opérationnels');
    expect(badge?.getAttribute('data-variant')).toBe('success');
  });

  it('shows the database card: an "Opérationnelle" badge, its version, response time and connections', () => {
    const { fixture, el } = setup();
    fixture.detectChanges();

    const database = card(el, 'Base de données')!;
    const badge = database.querySelector('.gbt-card__header gbt-badge .gbt-badge');
    expect(text(badge)).toBe('Opérationnelle');
    expect(badge?.getAttribute('data-variant')).toBe('success');
    expect(badge?.querySelector('gbt-icon')).not.toBeNull(); // never colour alone
    const header = database.querySelector(':scope > .gbt-card__header');
    const box = database.querySelector(':scope > .gbt-card');
    expect(Array.from(database.children)).toEqual([header, box]);
    expect(box?.getAttribute('data-variant')).toBe('outlined');
    expect(header?.hasAttribute('data-tone')).toBe(false);
    expect(header?.querySelector('.gbt-card__icon gbt-icon')).not.toBeNull();
    expect(facts(database)).toEqual({ Version: 'PostgreSQL 18.0', 'Temps de réponse': '3 ms' });
    const connections = database.querySelector('gbt-gauge-bar [role="progressbar"]')!;
    expect(connections.getAttribute('aria-label')).toBe('Connexions actives');
    expect(connections.getAttribute('aria-valuenow')).toBe('20');
    expect(connections.getAttribute('aria-valuetext')).toBe('2 sur 10');
  });

  it('shows the storage card with a gauge of the space used: used/total and the percent', () => {
    const { fixture, el } = setup({
      ...ALL_UP,
      storage: { status: 'up', detail: null, usedBytes: 4_000_000_000, freeBytes: 16_000_000_000, totalBytes: 20_000_000_000 },
    });
    fixture.detectChanges();

    const storage = card(el, 'Stockage')!;
    expect(text(storage.querySelector('.gbt-card__header gbt-badge'))).toBe('Opérationnel');
    const gauge = storage.querySelector('gbt-gauge-bar')!;
    const bar = gauge.querySelector('[role="progressbar"]')!;
    expect(bar.getAttribute('aria-label')).toBe('Espace utilisé');
    expect(bar.getAttribute('aria-valuenow')).toBe('20');
    expect(text(gauge.querySelector('.gbt-gauge-bar__value'))).toBe('3,7 Go sur 18,6 Go · 20 %');
    expect(facts(storage)).toEqual({ Utilisé: '3,7 Go', Libre: '14,9 Go', Total: '18,6 Go' });
  });

  it('shows the uptime, formatted, and when the server started', () => {
    const { fixture, el } = setup({ ...ALL_UP, uptimeSeconds: 2 * 86_400 + 3 * 3600 + 120 });
    fixture.detectChanges();

    const uptime = el.querySelector('.admin-health__uptime')!;
    expect(text(uptime.querySelector('.admin-health__uptime-value'))).toBe('2 j 3 h');
    expect(text(uptime.querySelector('.admin-health__uptime-since'))).toMatch(/^Démarré le \d{2}\/\d{2}\/\d{4} \d{2}:\d{2}$/);
  });

  it('shows the database server version when up', () => {
    const { fixture, el } = setup();
    fixture.detectChanges();
    expect(el.textContent).toContain('18.0');
  });

  it('shows a down status and its detail message, without the figures of a database it cannot reach', () => {
    const { fixture, el } = setup(DATABASE_DOWN);
    fixture.detectChanges();

    const database = card(el, 'Base de données')!;
    const badge = database.querySelector('.gbt-card__header gbt-badge .gbt-badge');
    expect(text(badge)).toBe('Indisponible');
    expect(badge?.getAttribute('data-variant')).toBe('error');
    expect(database.querySelector(':scope > .gbt-card__header')?.getAttribute('data-tone')).toBe('error');
    expect(card(el, 'Stockage')?.querySelector(':scope > .gbt-card__header')?.hasAttribute('data-tone')).toBe(false);
    expect(text(database.querySelector('.admin-health__detail'))).toBe('connection refused');
    expect(database.querySelector('dl')).toBeNull();
    expect(database.querySelector('gbt-gauge-bar')).toBeNull();

    const overall = el.querySelector('gbt-page-header gbt-badge .gbt-badge');
    expect(text(overall)).toBe('Service dégradé');
    expect(overall?.getAttribute('data-variant')).toBe('error');
  });

  it('shows skeleton cards while the first check runs', () => {
    const pending = new Subject<HealthStatus>();
    const { fixture, el } = setup(pending);
    fixture.detectChanges();

    expect(el.querySelector('[aria-busy="true"]')).not.toBeNull();
    expect(el.querySelectorAll('.admin-health__card')).toHaveLength(0);

    pending.next(ALL_UP);
    fixture.detectChanges();
    expect(el.querySelector('[aria-busy="true"]')).toBeNull();
    expect(el.querySelectorAll('.admin-health__card')).toHaveLength(2);
  });

  it('shows a toast and an inline unreachable message when the whole request fails', () => {
    const { fixture, el, toastStub } = setup('error');
    fixture.detectChanges();

    expect(toastStub.show).toHaveBeenCalledWith("Impossible de charger l'état de santé. Réessayez plus tard.", 'error');
    expect(el.textContent).toContain('API ou base de données injoignable.');
    const unreachable = el.querySelector('gbt-alert .gbt-alert');
    expect(text(unreachable)).toBe("API ou base de données injoignable. L'état des services n'a pas pu être vérifié. Actualisez une fois le serveur rétabli.");
    expect(unreachable?.getAttribute('data-variant')).toBe('error');
    expect(unreachable?.getAttribute('role')).toBeNull();
    expect(unreachable?.getAttribute('aria-live')).toBeNull();
    expect(text(el.querySelector('gbt-page-header gbt-badge'))).toBe('Injoignable');
  });

  describe('Actualiser', () => {
    it('is a secondary button of the header that checks again, once per click (no polling)', () => {
      const { fixture, el, metricsStub } = setup();
      fixture.detectChanges();
      expect(metricsStub.getHealth).toHaveBeenCalledTimes(1);

      const refresh = el.querySelector<HTMLButtonElement>('gbt-page-header .gbt-page-header__actions button')!;
      expect(text(refresh)).toBe('Actualiser');
      expect(refresh.classList).toContain('gbt-button--secondary');
      expect(el.querySelectorAll('.gbt-button--primary')).toHaveLength(0);
      refresh.click();
      fixture.detectChanges();

      expect(metricsStub.getHealth).toHaveBeenCalledTimes(2);
    });

    it('keeps the last result on screen while checking again, then shows the new one', () => {
      const { fixture, el, metricsStub } = setup();
      fixture.detectChanges();
      const pending = new Subject<HealthStatus>();
      metricsStub.getHealth.mockReturnValue(pending);

      button(el, 'Actualiser')!.click();
      fixture.detectChanges();
      expect(card(el, 'Base de données')).toBeDefined();
      expect(el.querySelector('.admin-health__cards')?.getAttribute('aria-busy')).toBe('true');

      pending.next(DATABASE_DOWN);
      fixture.detectChanges();
      expect(text(card(el, 'Base de données')!.querySelector('.gbt-card__header gbt-badge'))).toBe('Indisponible');
      expect(el.querySelector('.admin-health__cards')?.getAttribute('aria-busy')).toBeNull();
    });

    it('drops the "vérifié à" time of an earlier success when a check fails', () => {
      const { fixture, el, metricsStub } = setup();
      fixture.detectChanges();
      expect(text(el.querySelector('gbt-page-header .gbt-page-header__meta'))).toContain('vérifié à');

      metricsStub.getHealth.mockReturnValue(throwError(() => new Error('boom')));
      button(el, 'Actualiser')!.click();
      fixture.detectChanges();

      expect(text(el.querySelector('gbt-page-header gbt-badge'))).toBe('Injoignable');
      expect(text(el.querySelector('gbt-page-header .gbt-page-header__meta'))).not.toContain('vérifié à');
    });

    it('recovers from an unreachable API once a check succeeds again', () => {
      const { fixture, el, metricsStub } = setup('error');
      fixture.detectChanges();

      metricsStub.getHealth.mockReturnValue(of(ALL_UP));
      button(el, 'Actualiser')!.click();
      fixture.detectChanges();

      expect(el.textContent).not.toContain('API ou base de données injoignable.');
      expect(card(el, 'Base de données')).toBeDefined();
    });
  });
});
