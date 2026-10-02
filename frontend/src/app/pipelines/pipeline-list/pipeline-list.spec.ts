import { LOCALE_ID } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { By } from '@angular/platform-browser';
import { provideRouter } from '@angular/router';
import { EmptyState, GbtInput, Select, GbtToastService, formatDateTime, formatRelativeTime } from '@masmarino/gabarit';
import { PipelineList } from './pipeline-list';
import { PipelineSummary } from '../pipelines.service';
import { PageTitleService } from '../../shell/page-title.service';

const RELATIVE_OPTIONS = { style: 'short', maxUnit: 'day', absoluteAfterDays: 30 } as const;
const ABSOLUTE_OPTIONS = { day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' } as const;
const relativeTime = (iso: string) => formatRelativeTime(iso, 'fr', undefined, RELATIVE_OPTIONS);
const absoluteDateTime = (iso: string) => formatDateTime(iso, 'fr', ABSOLUTE_OPTIONS);

const LIST_URL = '/api/repositories/repo-1/pipelines';

function pipeline(overrides: Partial<PipelineSummary> = {}): PipelineSummary {
  return {
    id: '3f2a9c1e-7b4d-4e21-9a0b-5c6d7e8f9a0b',
    commitSha: 'a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0',
    status: 'success',
    createdAt: '2026-01-01T00:00:00Z',
    finishedAt: '2026-01-01T00:01:35Z',
    triggeredBy: { id: 'u1', username: 'alice' },
    commitMessage: 'Paginer la liste des tickets',
    error: null,
    ...overrides,
  };
}

function manyPipelines(count: number): PipelineSummary[] {
  return Array.from({ length: count }, (_, index) =>
    pipeline({
      id: `p${index + 1}`,
      commitSha: `${String(index + 1).padStart(4, '0')}ffffffffffff`,
      commitMessage: `Commit ${index + 1}`,
      createdAt: new Date(Date.UTC(2026, 0, 1, 0, index)).toISOString(),
      finishedAt: new Date(Date.UTC(2026, 0, 1, 0, index, 30)).toISOString(),
    }),
  );
}

describe('PipelineList', () => {
  function setup() {
    TestBed.configureTestingModule({
      providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([]), { provide: LOCALE_ID, useValue: 'fr' }],
    });
    const fixture = TestBed.createComponent(PipelineList);
    fixture.componentRef.setInput('repositoryId', 'repo-1');
    fixture.componentRef.setInput('path', ['acme', 'widget']);
    const http = TestBed.inject(HttpTestingController);
    const el = fixture.nativeElement as HTMLElement;
    return { fixture, http, el };
  }

  function loaded(list: PipelineSummary[]) {
    const ctx = setup();
    ctx.fixture.detectChanges();
    ctx.http.expectOne(LIST_URL).flush(list);
    ctx.fixture.detectChanges();
    return ctx;
  }

  const text = (el: Element | null | undefined) => (el?.textContent ?? '').replace(/\s+/g, ' ').trim();
  const expectFailedCard = (el: HTMLElement) => {
    const card = el.querySelector('gbt-list-card')!;
    expect(card.querySelector('.gbt-list-card')!.getAttribute('data-state')).toBe('failed');
    const alerts = el.querySelectorAll('[role="alert"]');
    expect(alerts).toHaveLength(1);
    expect(card.contains(alerts[0])).toBe(true);
    expect(text(alerts[0])).toBe("Les pipelines n'ont pas pu être chargés.");
    expect(alerts[0].querySelector('.gbt-empty-state')!.getAttribute('data-tone')).toBe('error');
    expect(card.querySelector('[aria-live]')).toBeNull();
    expect(TestBed.inject(GbtToastService).toasts()).toEqual([]);
    // "Actualiser" in the header is the retry: none in the card.
    expect(card.querySelector('button')).toBeNull();
  };
  const rows = (el: HTMLElement) => Array.from(el.querySelectorAll<HTMLElement>('.pipeline-list__items > li'));
  const rowTitles = (el: HTMLElement) => rows(el).map((row) => text(row.querySelector('.gbt-list-row__title > a')));
  const buttonByText = (root: Element, label: string) => Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find((button) => text(button) === label);
  const tabButtons = (el: HTMLElement) => Array.from(el.querySelectorAll<HTMLButtonElement>('.pipeline-list__tabs [role="radio"]'));
  const searchInput = (el: HTMLElement) => el.querySelector<HTMLInputElement>('.pipeline-list__search input')!;

  function type(input: HTMLInputElement, value: string, fixture: { detectChanges(): void }) {
    input.value = value;
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();
  }

  function pickOrder(fixture: { detectChanges(): void; nativeElement: HTMLElement }, optionLabel: string): void {
    fixture.nativeElement.querySelector<HTMLButtonElement>('.pipeline-list__order .gbt-select__trigger')!.click();
    fixture.detectChanges();
    const option = Array.from(document.querySelectorAll<HTMLElement>('.gbt-select__option')).find((el) => el.textContent?.trim() === optionLabel);
    option!.click();
    fixture.detectChanges();
  }

  const goToPage = (fixture: { detectChanges(): void }, el: HTMLElement, page: number) => {
    el.querySelector<HTMLButtonElement>(`gbt-pagination [aria-label="Page ${page}"]`)!.click();
    fixture.detectChanges();
  };

  const refreshButton = (el: HTMLElement) => buttonByText(el.querySelector('.gbt-page-header__actions')!, 'Actualiser')!;

  describe('filters in the header', () => {
    it('are two small fields (32px, level with the tabs) named by labels that stay hidden, the search with a leading magnifier', () => {
      const { el, fixture } = loaded([pipeline()]);

      const search = fixture.debugElement.query(By.css('.pipeline-list__search')).componentInstance as GbtInput;
      expect(search.size()).toBe('sm');
      expect(search.leadingIcon()).toBe('search');
      expect(search.hideLabel()).toBe(true);
      expect(el.querySelector('.pipeline-list__search label')?.textContent?.trim()).toBe('Rechercher un pipeline');
      const order = fixture.debugElement.query(By.css('.pipeline-list__order')).componentInstance as Select;
      expect(order.size()).toBe('sm');
      expect(order.hideLabel()).toBe(true);
      expect(el.querySelector('.pipeline-list__order label')?.textContent?.trim()).toBe('Ordre');
    });
  });

  describe('page frame', () => {
    it('lays the page out as a wide page layout without an aside, under a "Pipelines" page header', () => {
      const { el } = loaded([pipeline()]);

      const layout = el.querySelector('gbt-page-layout');
      expect(layout).toBeTruthy();
      expect(layout!.getAttribute('data-width')).toBe('wide');
      expect(el.querySelector('.gbt-page-layout__main gbt-list-card')).toBeTruthy();
      expect(el.querySelector('.gbt-page-layout__aside')!.children.length).toBe(0);
      expect(text(el.querySelector('gbt-page-header h1'))).toBe('Pipelines');
      expect(TestBed.inject(PageTitleService).title()).toBe('Pipelines');
    });

    it('offers a secondary "Actualiser" button in the header, and no primary button', () => {
      const { el } = loaded([pipeline()]);

      expect(refreshButton(el)).toBeTruthy();
      expect(refreshButton(el).classList).toContain('gbt-button--secondary');
      expect(el.querySelector('.gbt-button--primary')).toBeNull();
    });
  });

  describe('loading, empty and error states', () => {
    it('shows skeleton rows until the pipelines arrive, then the rows', () => {
      const { fixture, http, el } = setup();
      fixture.detectChanges();

      const card = el.querySelector('gbt-list-card')!;
      expect(card.querySelector('.gbt-list-card')!.getAttribute('data-state')).toBe('loading');
      const busy = card.querySelector('[aria-busy="true"]')!;
      expect(busy).toBeTruthy();
      expect(busy.querySelectorAll('gbt-skeleton').length).toBeGreaterThan(0);
      // The polite status sits beside the busy skeleton, never inside it.
      expect(text(card.querySelector('[role="status"]'))).toBe('Chargement des pipelines…');
      expect(busy.querySelector('[role="status"]')).toBeNull();
      expect(el.querySelector('gbt-empty-state')).toBeNull();
      expect(refreshButton(el).disabled).toBe(true);

      http.expectOne(LIST_URL).flush([pipeline()]);
      fixture.detectChanges();

      expect(el.querySelector('gbt-list-card .gbt-list-card')!.getAttribute('data-state')).toBe('ready');
      expect(el.querySelector('gbt-list-card [aria-busy="true"]')).toBeNull();
      expect(text(el.querySelector('gbt-list-card [role="status"]'))).toBe('');
      expect(rows(el).length).toBe(1);
      expect(refreshButton(el).disabled).toBe(false);
    });

    it('shows the illustrated empty state, pointing at .ferrisgit-ci.yml, when there are no pipelines', () => {
      const { fixture, el } = loaded([]);

      const emptyState = el.querySelector('gbt-empty-state');
      expect(emptyState).toBeTruthy();
      expect(fixture.debugElement.query(By.directive(EmptyState)).componentInstance.illustration()).toBe('pipeline');
      expect(el.querySelector('gbt-list-card .gbt-list-card')!.getAttribute('data-state')).toBe('empty');
      expect(text(emptyState)).toContain('Aucun pipeline');
      expect(text(emptyState)).toContain('.ferrisgit-ci.yml');
      expect(el.querySelector('.pipeline-list__tabs')).toBeNull();
      expect(el.querySelector('.pipeline-list__search')).toBeNull();
    });

    it('says the list could not be loaded in one alert (no toast on top of it) and no tabs, when the request fails', () => {
      const { fixture, http, el } = setup();
      fixture.detectChanges();
      http.expectOne(LIST_URL).flush('boom', { status: 500, statusText: 'Server Error' });
      fixture.detectChanges();

      expectFailedCard(el);
      expect(el.querySelector('.pipeline-list__tabs')).toBeNull();
      expect(rows(el)).toHaveLength(0);
    });

    it('reloads on "Actualiser", and a second failure gets a toast (the alert itself does not re-announce)', () => {
      const { fixture, http, el } = setup();
      fixture.detectChanges();
      http.expectOne(LIST_URL).flush('boom', { status: 500, statusText: 'Server Error' });
      fixture.detectChanges();

      el.querySelector<HTMLButtonElement>('gbt-button[text="Actualiser"] button')?.click();
      fixture.detectChanges();
      http.expectOne(LIST_URL).flush('boom again', { status: 500, statusText: 'Server Error' });
      fixture.detectChanges();

      expect(el.querySelector('gbt-list-card .gbt-list-card')!.getAttribute('data-state')).toBe('failed');
      expect(TestBed.inject(GbtToastService).toasts().at(-1)).toMatchObject({ message: 'Impossible de charger les pipelines. Réessayez plus tard.', variant: 'error' });
    });
  });

  describe('rows', () => {
    it('links "#<short id> <commit message>" to the pipeline, with the full message on hover', () => {
      const { el } = loaded([pipeline()]);

      const link = rows(el)[0].querySelector<HTMLAnchorElement>('.gbt-list-row__title > a')!;
      expect(link.getAttribute('href')).toBe('/repositories/acme/widget/-/pipelines/3f2a9c1e-7b4d-4e21-9a0b-5c6d7e8f9a0b');
      expect(text(link)).toBe('#3f2a9c1e Paginer la liste des tickets');
      expect(text(link.querySelector('.pipeline-list__number'))).toBe('#3f2a9c1e');
      expect(link.getAttribute('title')).toBe('Paginer la liste des tickets');
    });

    it('titles the row "#<short id>" alone when the commit message is unknown', () => {
      const { el } = loaded([pipeline({ commitMessage: null })]);

      const link = rows(el)[0].querySelector<HTMLAnchorElement>('.gbt-list-row__title > a')!;
      expect(text(link)).toBe('#3f2a9c1e');
      expect(link.getAttribute('title')).toBe('Pipeline #3f2a9c1e');
      expect(text(rows(el)[0])).not.toContain('null');
    });

    it('shows the short commit SHA as a monospace chip, the full SHA on hover', () => {
      const { el } = loaded([pipeline()]);

      const chip = rows(el)[0].querySelector('gbt-badge.pipeline-list__sha')!;
      const sha = chip.querySelector('.gbt-badge__label')!;
      expect(text(sha)).toBe('a1b2c3d4');
      expect(sha.getAttribute('title')).toBe('a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0');
    });

    it('shows the status as a tinted leading icon and a French status badge, never the raw API status', () => {
      const { el } = loaded([
        pipeline({ id: 'p1', status: 'failed', createdAt: '2026-01-05T00:00:00Z' }),
        pipeline({ id: 'p2', status: 'running', finishedAt: null, createdAt: '2026-01-04T00:00:00Z' }),
        pipeline({ id: 'p3', status: 'success', createdAt: '2026-01-03T00:00:00Z' }),
        pipeline({ id: 'p4', status: 'pending', finishedAt: null, createdAt: '2026-01-02T00:00:00Z' }),
        pipeline({ id: 'p5', status: 'canceled', createdAt: '2026-01-01T00:00:00Z' }),
      ]);

      const leading = rows(el).map((row) => row.querySelector('[row-leading]')!);
      expect(rows(el).map((row) => row.querySelector('.gbt-list-row__leading')!.getAttribute('data-tone'))).toEqual(['error', 'info', 'success', 'neutral', 'neutral']);
      expect(leading.every((icon) => icon.querySelector('gbt-icon'))).toBe(true);
      // The badge carries the status text; the icon repeats it only as a tooltip.
      expect(leading.every((icon) => icon.getAttribute('aria-hidden') === 'true')).toBe(true);
      expect(leading.map((icon) => icon.getAttribute('title'))).toEqual(['Échoué', 'En cours', 'Réussi', 'En attente', 'Annulé']);

      const badges = rows(el).map((row) => row.querySelector('.gbt-list-row__trailing fg-status-badge')!);
      expect(badges.map(text)).toEqual(['Échoué', 'En cours', 'Réussi', 'En attente', 'Annulé']);

      const visibleAndTitles = text(el) + ' ' + Array.from(el.querySelectorAll('[title]'), (node) => node.getAttribute('title')).join(' ');
      expect(visibleAndTitles).not.toMatch(/\b(pending|running|success|failed|canceled)\b/i);
    });

    it('gives every row the same trailing set: exactly one status badge', () => {
      const { el } = loaded([
        pipeline({ id: 'p1', status: 'failed' }),
        pipeline({ id: 'p2', status: 'running', finishedAt: null, commitMessage: null, triggeredBy: null }),
      ]);

      for (const row of rows(el)) {
        const trailing = row.querySelector('.gbt-list-row__trailing')!;
        expect(trailing.children.length).toBe(1);
        expect(trailing.querySelectorAll('fg-status-badge').length).toBe(1);
      }
    });

    it('writes when and by whom the pipeline was triggered, with the exact date on hover', () => {
      const createdAt = '2026-01-10T08:30:00Z';
      const { el } = loaded([pipeline({ createdAt, finishedAt: '2026-01-10T08:31:35Z' })]);

      const meta = rows(el)[0].querySelector('.pipeline-list__meta')!;
      const time = meta.querySelector('time')!;
      expect(time.getAttribute('datetime')).toBe(createdAt);
      expect(time.getAttribute('title')).toBe(absoluteDateTime(createdAt));
      expect(text(time)).toBe(relativeTime(createdAt));
      expect(text(meta.querySelector('.pipeline-list__triggered'))).toBe(`déclenché ${relativeTime(createdAt)} par alice`);
      expect(text(meta)).toBe(`a1b2c3d4 déclenché ${relativeTime(createdAt)} par alice · durée 1m 35s`);
    });

    it('leaves out "par …" when the user who triggered it no longer resolves', () => {
      const { el } = loaded([pipeline({ triggeredBy: null })]);

      const triggered = text(rows(el)[0].querySelector('.pipeline-list__triggered'));
      expect(triggered).toMatch(/^déclenché /);
      expect(triggered).not.toContain('par');
    });

    it('shows how long a finished pipeline took, whatever its outcome', () => {
      const { el } = loaded([
        pipeline({ id: 'p1', status: 'failed', createdAt: '2026-01-02T00:00:00Z', finishedAt: '2026-01-02T00:00:42Z' }),
        pipeline({ id: 'p2', status: 'canceled', createdAt: '2026-01-01T00:00:00Z', finishedAt: '2026-01-01T02:05:00Z' }),
      ]);

      expect(rows(el).map((row) => text(row.querySelector('.pipeline-list__duration')))).toEqual(['· durée 42s', '· durée 2h 5m']);
    });

    it('says since when a running or pending pipeline has been going, as of the last load', () => {
      const { el } = loaded([
        pipeline({ id: 'p1', status: 'running', finishedAt: null, createdAt: new Date(Date.now() - 65_000).toISOString() }),
        pipeline({ id: 'p2', status: 'pending', finishedAt: null, createdAt: new Date(Date.now() - 20 * 60_000).toISOString() }),
      ]);

      const [running, pending] = rows(el).map((row) => text(row.querySelector('.pipeline-list__duration')));
      expect(running).toMatch(/^· en cours depuis 1m \d+s$/);
      expect(pending).toMatch(/^· en attente depuis 20m \d+s$/);
    });

    it('leaves the duration out for a finished pipeline without a finish time (older pipelines)', () => {
      const { el } = loaded([pipeline({ status: 'success', finishedAt: null })]);

      expect(rows(el)[0].querySelector('.pipeline-list__duration')).toBeNull();
      expect(text(rows(el)[0].querySelector('.pipeline-list__meta'))).not.toContain('durée');
    });

    it('marks a pipeline whose file was invalid: failed badge, a label, and no duration', () => {
      const { el } = loaded([
        pipeline({ id: 'p1', status: 'failed', createdAt: '2026-01-01T00:00:00Z', finishedAt: '2026-01-01T00:00:00Z', error: 'invalid YAML: boom' }),
        pipeline({ id: 'p2', status: 'failed', createdAt: '2026-01-02T00:00:00Z', finishedAt: '2026-01-02T00:03:00Z' }),
      ]);

      const [regular, invalid] = rows(el);
      expect(text(invalid.querySelector('fg-status-badge'))).toBe('Échoué');
      expect(text(invalid.querySelector('.pipeline-list__invalid'))).toBe('Fichier de pipeline invalide');
      expect(invalid.querySelector('.pipeline-list__duration')).toBeNull();
      expect(regular.querySelector('.pipeline-list__invalid')).toBeNull();
      expect(text(regular.querySelector('.pipeline-list__duration'))).toBe('· durée 3m 0s');
    });

    it('shows a running pipeline with the "En cours" badge', () => {
      const { el } = loaded([pipeline({ status: 'running', finishedAt: null })]);

      expect(text(rows(el)[0].querySelector('fg-status-badge'))).toBe('En cours');
    });
  });

  describe('status tabs', () => {
    const MIXED = [
      pipeline({ id: 'p1', status: 'success', commitMessage: 'Un', createdAt: '2026-01-01T00:00:00Z' }),
      pipeline({ id: 'p2', status: 'failed', commitMessage: 'Deux', createdAt: '2026-01-02T00:00:00Z' }),
      pipeline({ id: 'p3', status: 'running', finishedAt: null, commitMessage: 'Trois', createdAt: '2026-01-03T00:00:00Z' }),
      pipeline({ id: 'p4', status: 'pending', finishedAt: null, commitMessage: 'Quatre', createdAt: '2026-01-04T00:00:00Z' }),
      pipeline({ id: 'p5', status: 'canceled', commitMessage: 'Cinq', createdAt: '2026-01-05T00:00:00Z' }),
    ];
    const titles = (el: HTMLElement) => rowTitles(el).map((title) => title.replace(/^#\S+ /, ''));

    it('labels the tabs with their counts and shows every pipeline, newest first, on "Tous"', () => {
      const { el } = loaded(MIXED);

      expect(tabButtons(el).map(text)).toEqual(['Tous (5)', 'En cours (2)', 'Réussis (1)', 'Échoués (1)']);
      expect(tabButtons(el)[0].getAttribute('aria-checked')).toBe('true');
      expect(titles(el)).toEqual(['Cinq', 'Quatre', 'Trois', 'Deux', 'Un']);
    });

    it('groups running and pending pipelines under "En cours", and shows only successes and failures on their tabs', () => {
      const { fixture, el } = loaded(MIXED);

      tabButtons(el)[1].click();
      fixture.detectChanges();
      expect(titles(el)).toEqual(['Quatre', 'Trois']);

      tabButtons(el)[2].click();
      fixture.detectChanges();
      expect(titles(el)).toEqual(['Un']);

      tabButtons(el)[3].click();
      fixture.detectChanges();
      expect(titles(el)).toEqual(['Deux']);
    });

    it('counts only the pipelines that match the search', () => {
      const { fixture, el } = loaded(MIXED);

      type(searchInput(el), 'deux', fixture);

      expect(tabButtons(el).map(text)).toEqual(['Tous (1)', 'En cours (0)', 'Réussis (0)', 'Échoués (1)']);
    });

    it('says when a tab is empty, and when nothing matches the search', () => {
      const { fixture, el } = loaded([pipeline({ status: 'success' })]);

      tabButtons(el)[3].click();
      fixture.detectChanges();
      expect(rows(el).length).toBe(0);
      expect(text(el.querySelector('[list-card-message]'))).toBe('Aucun pipeline échoué');

      tabButtons(el)[1].click();
      fixture.detectChanges();
      expect(text(el.querySelector('[list-card-message]'))).toBe('Aucun pipeline en cours');

      tabButtons(el)[0].click();
      type(searchInput(el), 'introuvable', fixture);
      expect(text(el.querySelector('[list-card-message]'))).toBe('Aucun pipeline ne correspond à cette recherche');
    });
  });

  describe('search and order', () => {
    it('filters by commit SHA prefix', () => {
      const { fixture, el } = loaded([
        pipeline({ id: 'p1', commitSha: 'aaaaaaaaaa', commitMessage: null, createdAt: '2026-01-01T00:00:00Z' }),
        pipeline({ id: 'p2', commitSha: 'bbbbbbbbbb', commitMessage: null, createdAt: '2026-01-02T00:00:00Z' }),
        // "bbbb" is inside the SHA, not at its start, so it doesn't match.
        pipeline({ id: 'p3', commitSha: 'ccbbbbcccc', commitMessage: null, createdAt: '2026-01-03T00:00:00Z' }),
      ]);

      type(searchInput(el), 'BBBB', fixture);

      const shown = text(el.querySelector('.pipeline-list__items'));
      expect(shown).toContain('bbbbbbbb');
      expect(shown).not.toContain('aaaaaaaa');
      expect(shown).not.toContain('ccbbbbcc');
    });

    it('filters by commit message, ignoring case', () => {
      const { fixture, el } = loaded([
        pipeline({ id: 'p1', commitMessage: 'Paginer la liste des tickets', createdAt: '2026-01-01T00:00:00Z' }),
        pipeline({ id: 'p2', commitMessage: 'Corriger le webhook', createdAt: '2026-01-02T00:00:00Z' }),
        pipeline({ id: 'p3', commitMessage: null, createdAt: '2026-01-03T00:00:00Z' }),
      ]);

      type(searchInput(el), 'WEBHOOK', fixture);

      expect(rowTitles(el)).toEqual(['#p2 Corriger le webhook']);
    });

    it('sorts newest first by default, oldest first on demand', async () => {
      const { fixture, el } = loaded([
        pipeline({ id: 'p1', commitMessage: 'Premier', createdAt: '2026-01-01T00:00:00Z' }),
        pipeline({ id: 'p3', commitMessage: 'Troisième', createdAt: '2026-01-03T00:00:00Z' }),
        pipeline({ id: 'p2', commitMessage: 'Deuxième', createdAt: '2026-01-02T00:00:00Z' }),
      ]);
      expect(rowTitles(el)).toEqual(['#p3 Troisième', '#p2 Deuxième', '#p1 Premier']);
      await fixture.whenStable(); // the select's ngModel writes its value a microtask later
      fixture.detectChanges();
      expect(text(el.querySelector('.pipeline-list__order .gbt-select__trigger'))).toBe('Plus récents');

      pickOrder(fixture, 'Plus anciens');

      expect(rowTitles(el)).toEqual(['#p1 Premier', '#p2 Deuxième', '#p3 Troisième']);
    });
  });

  describe('pagination', () => {
    it('shows no pager for 25 pipelines', () => {
      const { el } = loaded(manyPipelines(25));

      expect(rows(el).length).toBe(25);
      expect(el.querySelector('gbt-pagination')).toBeNull();
    });

    it('pages 26 pipelines 25 at a time, the oldest on page 2', () => {
      const { fixture, el } = loaded(manyPipelines(26));

      expect(rows(el).length).toBe(25);
      expect(el.querySelector('gbt-pagination')).toBeTruthy();

      goToPage(fixture, el, 2);

      expect(rowTitles(el)).toEqual(['#p1 Commit 1']);
      expect(el.querySelector('gbt-pagination [aria-current="page"]')?.getAttribute('aria-label')).toBe('Page 2');
    });

    it('goes back to page 1 when the search changes', () => {
      const { fixture, el } = loaded(manyPipelines(30));
      goToPage(fixture, el, 2);
      expect(rows(el).length).toBe(5);

      type(searchInput(el), 'Commit', fixture);

      expect(rows(el).length).toBe(25);
      expect(rowTitles(el)[0]).toBe('#p30 Commit 30');
    });

    it('goes back to page 1 when the tab changes', () => {
      const failed = manyPipelines(27).map((p) => ({ ...p, id: `f-${p.id}`, status: 'failed' as const }));
      const { fixture, el } = loaded([...manyPipelines(26), ...failed]);
      goToPage(fixture, el, 2);

      tabButtons(el)[3].click();
      fixture.detectChanges();

      expect(rows(el).length).toBe(25);
      expect(el.querySelector('gbt-pagination [aria-current="page"]')?.getAttribute('aria-label')).toBe('Page 1');
    });

    it('falls back to the last page when a refresh leaves fewer pipelines than the current page needs', async () => {
      const list = manyPipelines(26);
      const { fixture, http, el } = loaded(list);
      goToPage(fixture, el, 2);
      expect(rowTitles(el)).toEqual(['#p1 Commit 1']);

      refreshButton(el).click();
      http.expectOne(LIST_URL).flush(list.slice(1));
      fixture.detectChanges();
      await fixture.whenStable();
      fixture.detectChanges();

      expect(el.querySelector('gbt-pagination')).toBeNull();
      expect(rows(el).length).toBe(25);
      expect(el.querySelector('[list-card-message]')).toBeNull();
    });
  });

  describe('refresh', () => {
    it('re-fetches the pipelines and shows their new statuses, keeping the rows meanwhile', () => {
      const { fixture, http, el } = loaded([pipeline({ status: 'running', finishedAt: null })]);

      refreshButton(el).click();
      fixture.detectChanges();
      expect(rows(el).length).toBe(1);
      expect(el.querySelector('.pipeline-list__card--loading')).toBeNull();

      http.expectOne(LIST_URL).flush([pipeline({ status: 'success' })]);
      fixture.detectChanges();

      expect(text(rows(el)[0].querySelector('fg-status-badge'))).toBe('Réussi');
      expect(text(rows(el)[0].querySelector('.pipeline-list__duration'))).toBe('· durée 1m 35s');
    });

    it('says so, once, when a refresh fails', () => {
      const { fixture, http, el } = loaded([pipeline()]);

      refreshButton(el).click();
      http.expectOne(LIST_URL).flush('boom', { status: 500, statusText: 'Server Error' });
      fixture.detectChanges();

      expectFailedCard(el);
    });
  });
});
