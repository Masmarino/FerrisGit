import { TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { provideRouter } from '@angular/router';
import { Icon, GbtToastService, formatDateTime, formatRelativeTime } from '@masmarino/gabarit';
import { ONLINE_THRESHOLD_MS, RunnersList, runnerConnectivity } from './runners-list';
import { RunnerSummary } from '../runners.service';
import { PageTitleService } from '../../shell/page-title.service';

const RELATIVE_OPTIONS = { style: 'short', maxUnit: 'day', absoluteAfterDays: 30 } as const;
const ABSOLUTE_OPTIONS = { day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' } as const;
// Gabarit's relative wording joins number and unit with a NBSP while `text()` collapses whitespace, so normalize here too.
const relativeTime = (iso: string) => formatRelativeTime(iso, 'fr', undefined, RELATIVE_OPTIONS).replace(/\s+/g, ' ').trim();
const absoluteDateTime = (iso: string) => formatDateTime(iso, 'fr', ABSOLUTE_OPTIONS);

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();

const minutesAgo = (minutes: number) => new Date(Date.now() - minutes * 60_000).toISOString();

describe('runnerConnectivity', () => {
  const NOW = new Date('2026-09-25T12:00:00Z');
  const before = (ms: number) => new Date(NOW.getTime() - ms).toISOString();

  it('is "never" without a heartbeat', () => {
    expect(runnerConnectivity(null, NOW)).toBe('never');
  });

  it('is "online" while the last heartbeat is younger than 2 minutes, "offline" from 2 minutes on', () => {
    expect(ONLINE_THRESHOLD_MS).toBe(2 * 60 * 1000);
    expect(runnerConnectivity(before(0), NOW)).toBe('online');
    expect(runnerConnectivity(before(ONLINE_THRESHOLD_MS - 1), NOW)).toBe('online');
    expect(runnerConnectivity(before(ONLINE_THRESHOLD_MS), NOW)).toBe('offline');
    expect(runnerConnectivity(before(3 * 24 * 60 * 60 * 1000), NOW)).toBe('offline');
  });

  it('reads a heartbeat slightly in the future (clock skew) as online', () => {
    expect(runnerConnectivity(new Date(NOW.getTime() + 5_000).toISOString(), NOW)).toBe('online');
  });
});

describe('RunnersList', () => {
  function setup(runners: RunnerSummary[] | 'error' = []) {
    const toastStub = { show: vi.fn() };
    const pageTitleStub = { set: vi.fn() };
    TestBed.configureTestingModule({
      providers: [
        provideHttpClient(),
        provideHttpClientTesting(),
        provideRouter([]),
        { provide: GbtToastService, useValue: toastStub },
        { provide: PageTitleService, useValue: pageTitleStub },
      ],
    });
    const fixture = TestBed.createComponent(RunnersList);
    const http = TestBed.inject(HttpTestingController);
    fixture.detectChanges();
    if (runners === 'error') {
      http.expectOne('/api/admin/runners').flush({ message: 'boom' }, { status: 500, statusText: 'Server Error' });
    } else {
      http.expectOne('/api/admin/runners').flush(runners);
    }
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;
    return { fixture, el, http, toastStub, pageTitleStub };
  }

  const button = (root: Element, label: string) => Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === label);
  const modal = () => document.querySelector<HTMLElement>('.runners-list__register');
  const rows = (el: HTMLElement) => Array.from(el.querySelectorAll<HTMLElement>('ul.runners-list__rows > li > gbt-list-row'));

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  it('sets the page title', () => {
    const { pageTitleStub } = setup();
    expect(pageTitleStub.set).toHaveBeenCalledWith('Runners');
  });

  it('is a wide page: a "Runners" h1 whose one primary action registers a runner, and a help panel aside', () => {
    const { el } = setup();

    expect(text(el.querySelector('gbt-page-header h1'))).toBe('Runners');
    const layout = el.querySelector('gbt-page-layout')!;
    expect(layout.getAttribute('data-width')).toBe('wide');
    expect(layout.getAttribute('data-aside-width')).toBe('sm');
    const primaries = Array.from(el.querySelectorAll('.gbt-button--primary'));
    expect(primaries.map((b) => text(b))).toEqual(['Enregistrer un runner']);
    expect(primaries[0].closest('.gbt-page-header__actions')).not.toBeNull();
    const help = el.querySelector('.gbt-page-layout__aside gbt-panel');
    expect(text(help?.querySelector('h2'))).toBe('Comment ça marche');
    expect(text(help)).toContain('FERRISGIT_RUNNER_TOKEN');
  });

  it('derives the aside\'s "hors ligne" delay from ONLINE_THRESHOLD_MS', () => {
    const { fixture, el } = setup();
    const minutes = ONLINE_THRESHOLD_MS / 60_000;
    expect((fixture.componentInstance as unknown as Record<string, unknown>)['onlineThresholdMinutes']).toBe(minutes);
    expect(text(el.querySelector('.runners-list__steps'))).toContain(`« hors ligne » après ${minutes} minutes sans nouvelles`);
  });

  it('says the runners could not be loaded — not that there are none — and offers to retry', () => {
    const { fixture, el, http, toastStub } = setup('error');

    // The toast announces the failure once, and the alert in the card stays silent (no role, no live region).
    expect(toastStub.show).toHaveBeenCalledWith('Impossible de charger les runners. Réessayez plus tard.', 'error');
    expect(el.querySelector('gbt-empty-state')).toBeNull();
    const failed = el.querySelector('gbt-list-card gbt-alert .gbt-alert')!;
    expect(failed.getAttribute('data-variant')).toBe('error');
    expect(failed.getAttribute('role')).toBeNull();
    expect(failed.getAttribute('aria-live')).toBeNull();
    expect(el.querySelector('[role="alert"]')).toBeNull();
    expect(text(failed)).toContain('Impossible de charger les runners');
    expect(text(el.querySelector('gbt-list-card .gbt-list-card__header h2'))).toBe('Runners enregistrés');
    expect(el.querySelector('gbt-list-card .gbt-list-card__box gbt-alert')).not.toBeNull();
    const retry = button(failed, 'Réessayer')!;
    expect(retry.classList).toContain('gbt-button--secondary');

    retry.click();
    fixture.detectChanges();
    expect(el.querySelector('gbt-list-card gbt-alert')).toBeNull();
    expect(el.querySelectorAll('gbt-skeleton-list .gbt-skeleton-list__row')).toHaveLength(3);
    // One polite status, outside any aria-busy region (a busy ancestor can hold back the announcement).
    expect(text(el.querySelector('gbt-skeleton-list [role="status"]'))).toBe('Chargement des runners…');
    expect(el.querySelector('[aria-busy="true"]')).toBeNull();

    http.expectOne('/api/admin/runners').flush([{ id: 'r1', name: 'vps-1', tags: [], lastHeartbeatAt: null, createdAt: '2026-01-01T00:00:00Z' }]);
    fixture.detectChanges();
    expect(rows(el)).toHaveLength(1);
  });

  it('shows an illustrated empty state when there are no runners', () => {
    const { el } = setup();

    const emptyState = el.querySelector('gbt-empty-state');
    expect(emptyState?.getAttribute('illustration')).toBe('server');
    expect(rows(el)).toHaveLength(0);
    expect(el.querySelector('gbt-page-header .gbt-page-header__meta')?.textContent?.trim() ?? '').toBe('');
  });

  it('shows the runners as rows in a card, each tag as a gbt-tag chip', () => {
    const { el } = setup([
      { id: 'r1', name: 'vps-1', tags: ['docker', 'linux'], lastHeartbeatAt: null, createdAt: '2026-01-01T00:00:00Z' },
      { id: 'r2', name: 'vps-2', tags: [], lastHeartbeatAt: null, createdAt: '2026-01-02T00:00:00Z' },
    ]);

    expect(el.querySelector('gbt-empty-state')).toBeNull();
    const card = el.querySelector('gbt-list-card')!;
    expect(text(card.querySelector('h2'))).toBe('Runners enregistrés');
    expect(card.querySelector('.gbt-list-card')!.getAttribute('aria-label')).toBe('Runners enregistrés');
    const [first, second] = rows(el);
    expect(text(first.querySelector('.runners-list__name'))).toBe('vps-1');
    expect(first.querySelector('.runners-list__name')?.getAttribute('title')).toBe('vps-1');
    expect(Array.from(first.querySelectorAll('gbt-tag')).map((t) => text(t))).toEqual(['docker', 'linux']);
    expect(text(second.querySelector('.runners-list__no-tags'))).toBe('Aucun tag');
  });

  it('formats the last heartbeat as a relative date, the exact one on hover, instead of the raw ISO timestamp', () => {
    const heartbeat = minutesAgo(10);
    const { el } = setup([{ id: 'r1', name: 'vps-1', tags: [], lastHeartbeatAt: heartbeat, createdAt: '2026-01-01T00:00:00Z' }]);

    const row = rows(el)[0];
    expect(row.textContent).not.toContain(heartbeat);
    const time = row.querySelector<HTMLTimeElement>('.runners-list__heartbeat time')!;
    expect(text(time)).toBe(relativeTime(heartbeat));
    expect(time.getAttribute('datetime')).toBe(heartbeat);
    expect(time.getAttribute('title')).toBe(absoluteDateTime(heartbeat));
    expect(text(row.querySelector('.runners-list__heartbeat'))).toBe(`Dernier signal ${relativeTime(heartbeat)}`);
    expect(text(row.querySelector('.runners-list__created'))).toBe(`enregistré le ${relativeTime('2026-01-01T00:00:00Z')}`);
    expect(row.querySelector('.runners-list__created time')?.getAttribute('title')).toBe(absoluteDateTime('2026-01-01T00:00:00Z'));
  });

  it('shows "Jamais connecté" when a runner has never sent a heartbeat', () => {
    const { el } = setup([{ id: 'r1', name: 'vps-1', tags: [], lastHeartbeatAt: null, createdAt: '2026-01-01T00:00:00Z' }]);

    const row = rows(el)[0];
    expect(row.textContent).toContain('Jamais connecté');
    expect(row.querySelector('.runners-list__heartbeat')).toBeNull();
  });

  it('badges each runner "En ligne", "Hors ligne" or "Jamais connecté" from its heartbeat — in words, with an icon', () => {
    const { el } = setup([
      { id: 'r1', name: 'recent', tags: [], lastHeartbeatAt: minutesAgo(1), createdAt: '2026-01-01T00:00:00Z' },
      { id: 'r2', name: 'stale', tags: [], lastHeartbeatAt: minutesAgo(5), createdAt: '2026-01-01T00:00:00Z' },
      { id: 'r3', name: 'new', tags: [], lastHeartbeatAt: null, createdAt: '2026-01-01T00:00:00Z' },
    ]);

    const badges = rows(el).map((row) => row.querySelector('.gbt-list-row__trailing gbt-badge .gbt-badge')!);
    expect(badges.map((b) => text(b))).toEqual(['En ligne', 'Hors ligne', 'Jamais connecté']);
    expect(badges.map((b) => b.getAttribute('data-variant'))).toEqual(['success', 'neutral', 'neutral']);
    expect(badges.every((b) => b.querySelector('gbt-icon') !== null)).toBe(true);
    expect(rows(el).map((row) => row.querySelector('.gbt-list-row__leading')?.getAttribute('data-tone'))).toEqual(['success', 'neutral', 'neutral']);
    expect(text(el.querySelector('gbt-page-header .gbt-page-header__meta'))).toBe('3 runners · 1 en ligne');
  });

  describe('registration', () => {
    it('opens a dialog with the name and tags fields from the header button', () => {
      const { fixture, el } = setup();
      expect(modal()).toBeNull();

      button(el.querySelector('gbt-page-header')!, 'Enregistrer un runner')!.click();
      fixture.detectChanges();

      expect(modal()).not.toBeNull();
      expect(text(modal()!.closest('.gbt-modal__dialog')?.querySelector('h2'))).toBe('Enregistrer un runner');
      expect(text(modal()!.querySelector('gbt-input label'))).toBe('Nom du runner *');
      expect(text(modal()!.querySelector('gbt-tag-input label'))).toBe('Tags');
    });

    it('registers a runner with its tags, closes the dialog and shows the token inside a keyed card', () => {
      const { fixture, el, http, toastStub } = setup();
      fixture.componentInstance['registerOpen'].set(true);
      fixture.detectChanges();

      fixture.componentInstance['newRunnerName'].set('vps-2');
      fixture.componentInstance['newRunnerTags'].set(['docker']);
      fixture.componentInstance.register();

      const req = http.expectOne('/api/admin/runners');
      expect(req.request.body).toEqual({ name: 'vps-2', tags: ['docker'] });
      req.flush({ id: 'r2', name: 'vps-2', tags: ['docker'], token: 'plain-token' });
      fixture.detectChanges();
      http.expectOne('/api/admin/runners').flush([{ id: 'r2', name: 'vps-2', tags: ['docker'], lastHeartbeatAt: null, createdAt: '2026-01-01T00:00:00Z' }]);
      fixture.detectChanges();

      expect(modal()).toBeNull();
      const tokenCard = fixture.debugElement.query(By.css('.runners-list__token'));
      expect(text(tokenCard.nativeElement.querySelector('gbt-secret-reveal code'))).toBe('plain-token');
      expect(text(tokenCard.nativeElement.querySelector('h2'))).toBe('Jeton du runner vps-2');
      expect((tokenCard.query(By.directive(Icon))?.componentInstance as Icon | undefined)?.name()).toBe('key');
      const marker = tokenCard.nativeElement.querySelector('gbt-icon-marker .gbt-icon-marker');
      expect(marker.getAttribute('data-tone')).toBe('warning');
      expect(marker.getAttribute('data-appearance')).toBe('outline');
      expect(marker.getAttribute('aria-hidden')).toBe('true');
      expect(tokenCard.nativeElement.tagName).toBe('SECTION');
      expect(tokenCard.nativeElement.getAttribute('aria-labelledby')).toBe(tokenCard.nativeElement.querySelector('h2').id);
      const host = tokenCard.nativeElement.querySelector('gbt-card');
      const header = host.querySelector(':scope > .gbt-card__header');
      const box = host.querySelector(':scope > .gbt-card');
      expect(Array.from(host.children)).toEqual([header, box]);
      expect(box.getAttribute('data-variant')).toBe('outlined');
      expect(header.hasAttribute('data-tone')).toBe(false);
      expect(header.querySelector('h2')).toBe(tokenCard.nativeElement.querySelector('h2'));
      expect(header.querySelector('gbt-icon-marker')).toBe(tokenCard.nativeElement.querySelector('gbt-icon-marker'));
      expect(toastStub.show).toHaveBeenCalledWith('Runner enregistré.');
      expect(rows(el)).toHaveLength(1);
    });

    it('explains a missing name instead of sending nothing silently', () => {
      const { fixture, http } = setup();
      fixture.componentInstance['registerOpen'].set(true);
      fixture.detectChanges();

      fixture.componentInstance['newRunnerName'].set('   ');
      fixture.componentInstance.register();
      fixture.detectChanges();

      http.expectNone('/api/admin/runners');
      expect(text(modal()!.querySelector('.gbt-input__error'))).toBe('Donnez un nom au runner');
    });

    it('shows an error toast when runner registration fails, and keeps the dialog open with the draft', () => {
      const { fixture, http, toastStub } = setup();
      fixture.componentInstance['registerOpen'].set(true);
      fixture.detectChanges();

      fixture.componentInstance['newRunnerName'].set('vps-2');
      fixture.componentInstance['newRunnerTags'].set(['arm64']);
      fixture.componentInstance.register();

      const req = http.expectOne('/api/admin/runners');
      req.flush({ message: 'boom' }, { status: 500, statusText: 'Server Error' });
      fixture.detectChanges();

      expect(toastStub.show).toHaveBeenCalledWith("Impossible d'enregistrer le runner.", 'error');
      expect(modal()).not.toBeNull();
      expect(text(modal()!.querySelector('gbt-alert'))).toContain("Le runner n'a pas pu être enregistré");
      expect(fixture.componentInstance['newRunnerName']()).toBe('vps-2');
      expect(fixture.componentInstance['newRunnerTags']()).toEqual(['arm64']);
      expect(fixture.debugElement.query(By.css('.runners-list__token'))).toBeNull();
    });

    it('ignores Escape, the backdrop, the close button and "Annuler" while the registration runs', () => {
      const { fixture, http } = setup();
      fixture.componentInstance['registerOpen'].set(true);
      fixture.detectChanges();
      fixture.componentInstance['newRunnerName'].set('vps-2');
      fixture.componentInstance.register();
      fixture.detectChanges();

      const dialog = modal()!.closest<HTMLElement>('[role="dialog"]')!;
      const cancel = button(dialog, 'Annuler')!;
      expect(text(dialog.querySelector('[role="status"]'))).toBe('Enregistrement en cours');
      expect(cancel.disabled).toBe(true);
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
      document.querySelector<HTMLElement>('.gbt-modal__backdrop')!.click();
      dialog.querySelector<HTMLButtonElement>('.gbt-modal__close')!.click();
      cancel.click();
      fixture.detectChanges();
      expect(modal()).not.toBeNull();
      expect(fixture.componentInstance['newRunnerName']()).toBe('vps-2');

      http.expectOne('/api/admin/runners').flush({ message: 'boom' }, { status: 500, statusText: 'Server Error' });
      fixture.detectChanges();
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
      fixture.detectChanges();
      expect(modal()).toBeNull();
    });

    it('sends one request however many times it is submitted while in flight', () => {
      const { fixture, http } = setup();
      fixture.componentInstance['registerOpen'].set(true);
      fixture.componentInstance['newRunnerName'].set('vps-2');
      fixture.detectChanges();

      fixture.componentInstance.register();
      fixture.componentInstance.register();

      http.expectOne('/api/admin/runners').flush({ id: 'r2', name: 'vps-2', tags: [], token: 't' });
      http.expectOne('/api/admin/runners').flush([]);
    });

    it('starts empty on every opening: closing the dialog drops the draft and the error', async () => {
      const { fixture, el, http } = setup();
      button(el.querySelector('gbt-page-header')!, 'Enregistrer un runner')!.click();
      fixture.detectChanges();

      fixture.componentInstance['newRunnerName'].set('brouillon');
      fixture.componentInstance['newRunnerTags'].set(['docker']);
      fixture.componentInstance.register();
      http.expectOne('/api/admin/runners').flush({ message: 'boom' }, { status: 500, statusText: 'Server Error' });
      fixture.detectChanges();

      button(modal()!.closest<HTMLElement>('[role="dialog"]')!, 'Annuler')!.click();
      fixture.detectChanges();
      expect(modal()).toBeNull();

      button(el.querySelector('gbt-page-header')!, 'Enregistrer un runner')!.click();
      fixture.detectChanges();
      await fixture.whenStable();
      fixture.detectChanges();

      expect(modal()!.querySelector<HTMLInputElement>('gbt-input input')!.value).toBe('');
      expect(modal()!.querySelectorAll('gbt-tag-input gbt-tag')).toHaveLength(0);
      expect(modal()!.querySelector('gbt-alert')).toBeNull();
    });
  });

  describe('the token card', () => {
    let clipboardDescriptor: PropertyDescriptor | undefined;

    beforeEach(() => {
      clipboardDescriptor = Object.getOwnPropertyDescriptor(navigator, 'clipboard');
    });

    afterEach(() => {
      window.getSelection()?.removeAllRanges();
      if (clipboardDescriptor) {
        Object.defineProperty(navigator, 'clipboard', clipboardDescriptor);
      } else {
        delete (navigator as { clipboard?: unknown }).clipboard;
      }
    });

    function withToken() {
      const context = setup();
      context.fixture.componentInstance['newRunnerName'].set('vps-2');
      context.fixture.componentInstance.register();
      context.http.expectOne('/api/admin/runners').flush({ id: 'r2', name: 'vps-2', tags: [], token: 'plain-token' });
      context.fixture.detectChanges();
      context.http.expectOne('/api/admin/runners').flush([]);
      context.fixture.detectChanges();
      const card = () => context.el.querySelector<HTMLElement>('.runners-list__token');
      return { ...context, card };
    }

    it('says it is shown once, and how to use it', () => {
      const { card } = withToken();
      expect(text(card())).toContain('ne sera plus jamais affiché');
      expect(text(card())).toContain('FERRISGIT_RUNNER_TOKEN');
    });

    const copyToken = (card: HTMLElement) => card.querySelector<HTMLButtonElement>('button[aria-label="Copier le jeton"]')!;
    const copyStatus = (card: HTMLElement) => card.querySelector<HTMLElement>('gbt-secret-reveal [role="status"]')!;

    it('shows the token in full, with a named copy button and a named button to hide it', () => {
      const { card } = withToken();

      expect(text(card()!.querySelector('gbt-secret-reveal code'))).toBe('plain-token');
      expect(copyToken(card()!)).not.toBeNull();
      expect(card()!.querySelector('button[aria-label="Masquer le jeton"]')).not.toBeNull();
      // One live region for the copy, always in the DOM (an announcement needs it there before it changes).
      expect(card()!.querySelectorAll('[role="status"]')).toHaveLength(1);
      expect(text(copyStatus(card()!))).toBe('');
    });

    it('hides the token on request: it is then not in the DOM at all, and copying still works', async () => {
      const writeText = vi.fn(() => Promise.resolve());
      Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
      const { fixture, card } = withToken();

      card()!.querySelector<HTMLButtonElement>('button[aria-label="Masquer le jeton"]')!.click();
      fixture.detectChanges();

      expect(card()!.innerHTML).not.toContain('plain-token');
      expect(text(card()!.querySelector('gbt-secret-reveal code'))).toContain('Jeton masqué');
      copyToken(card()!).click();
      await fixture.whenStable();
      fixture.detectChanges();
      expect(writeText).toHaveBeenCalledWith('plain-token');
    });

    it('copies the token to the clipboard and says so', async () => {
      const writeText = vi.fn(() => Promise.resolve());
      Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
      const { fixture, card } = withToken();

      copyToken(card()!).click();
      await fixture.whenStable();
      fixture.detectChanges();

      expect(writeText).toHaveBeenCalledWith('plain-token');
      expect(text(copyStatus(card()!))).toBe('Jeton copié');
    });

    it('selects the token for a manual copy when the clipboard is unavailable', async () => {
      Object.defineProperty(navigator, 'clipboard', { configurable: true, value: undefined });
      const { fixture, card } = withToken();

      copyToken(card()!).click();
      await fixture.whenStable();
      fixture.detectChanges();
      await fixture.whenStable();

      expect(text(copyStatus(card()!))).toBe('Copie impossible, jeton sélectionné');
      expect(window.getSelection()?.toString()).toBe('plain-token');
    });

    it('goes away once the user says the token is copied', () => {
      const { fixture, card } = withToken();

      button(card()!, "J'ai copié le jeton")!.click();
      fixture.detectChanges();

      expect(card()).toBeNull();
    });
  });
});
