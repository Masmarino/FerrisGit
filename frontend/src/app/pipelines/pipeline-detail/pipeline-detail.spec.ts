import { LOCALE_ID } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController } from '@angular/common/http/testing';
import { provideHttpClientTesting } from '@angular/common/http/testing';
import { Router, provideRouter } from '@angular/router';
import { formatDateTime, formatRelativeTime } from '@masmarino/gabarit';
import { PipelineDetail } from './pipeline-detail';
import { JobSummary, PipelineDetail as PipelineDetailModel } from '../pipelines.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { PageTitleService } from '../../shell/page-title.service';

const RELATIVE_OPTIONS = { style: 'short', maxUnit: 'day', absoluteAfterDays: 30 } as const;
const ABSOLUTE_OPTIONS = { day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' } as const;
const relativeTime = (iso: string) => formatRelativeTime(iso, 'fr', undefined, RELATIVE_OPTIONS);
const absoluteDateTime = (iso: string) => formatDateTime(iso, 'fr', ABSOLUTE_OPTIONS);

const DETAIL_URL = '/api/pipelines/pipeline-1';

function makeJob(overrides: Partial<JobSummary> = {}): JobSummary {
  return {
    id: 'job-1',
    stage: 'build',
    name: 'compile',
    status: 'pending',
    needs: [],
    tags: [],
    logs: '',
    createdAt: '2026-01-01T00:00:00Z',
    startedAt: null,
    finishedAt: null,
    ...overrides,
  };
}

function makePipeline(overrides: Partial<PipelineDetailModel> = {}): PipelineDetailModel {
  return {
    id: 'pipeline-1',
    commitSha: 'abcdef1234567890',
    status: 'running',
    createdAt: '2026-01-01T00:00:00Z',
    finishedAt: null,
    triggeredBy: null,
    commitMessage: null,
    error: null,
    jobs: [],
    ...overrides,
  };
}

describe('PipelineDetail', () => {
  function setup(jobId: string | null = null) {
    TestBed.configureTestingModule({
      providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([]), { provide: LOCALE_ID, useValue: 'fr' }],
    });
    const fixture = TestBed.createComponent(PipelineDetail);
    fixture.componentRef.setInput('repositoryId', 'repo-1');
    fixture.componentRef.setInput('pipelineId', 'pipeline-1');
    fixture.componentRef.setInput('path', ['admin', 'my-repo']);
    fixture.componentRef.setInput('jobId', jobId);
    fixture.detectChanges();
    return { fixture };
  }

  describe('page header', () => {
    const text = (el: Element | null | undefined) => (el?.textContent ?? '').replace(/\s+/g, ' ').trim();

    function load(overrides: Partial<PipelineDetailModel>, role: 'owner' | 'contributor' | 'maintainer' | 'reader' | null = null) {
      TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([]), { provide: LOCALE_ID, useValue: 'fr' }] });
      if (role) {
        TestBed.inject(RepositoryContextService).current.set({ repositoryId: 'repo-1', path: ['admin', 'my-repo'], role, ancestors: [], groupId: null });
      }
      const fixture = TestBed.createComponent(PipelineDetail);
      fixture.componentRef.setInput('repositoryId', 'repo-1');
      fixture.componentRef.setInput('pipelineId', 'pipeline-1');
      fixture.componentRef.setInput('path', ['admin', 'my-repo']);
      fixture.detectChanges();
      TestBed.inject(HttpTestingController).expectOne(DETAIL_URL).flush(makePipeline(overrides));
      fixture.detectChanges();
      return fixture.nativeElement as HTMLElement;
    }

    it('titles the page "Pipeline #<short id>" in the page header (the page h1), with its status badge', () => {
      const el = load({ id: '3f2a9c1e-7b4d-4e21-9a0b-5c6d7e8f9a0b', status: 'failed', finishedAt: '2026-01-01T00:00:10Z' });

      expect(text(el.querySelector('gbt-page-header h1'))).toBe('Pipeline #3f2a9c1e');
      const badge = el.querySelector('gbt-page-header .gbt-page-header__badges fg-status-badge');
      expect(text(badge)).toBe('Échoué');
    });

    it('shows the same title in the shell header once the pipeline is loaded', () => {
      load({ id: '3f2a9c1e-7b4d-4e21-9a0b-5c6d7e8f9a0b' });

      expect(TestBed.inject(PageTitleService).title()).toBe('Pipeline #3f2a9c1e');
    });

    it('writes the short SHA, the commit message, and when and by whom the pipeline was triggered under the title', () => {
      const createdAt = '2026-01-01T00:00:00Z';
      const el = load({ createdAt, status: 'success', finishedAt: '2026-01-01T00:00:10Z', commitMessage: 'Paginer la liste des tickets', triggeredBy: { id: 'u1', username: 'alice' } });

      const meta = el.querySelector('gbt-page-header .pipeline-detail__meta')!;
      const chip = meta.querySelector('gbt-badge.pipeline-detail__sha')!;
      const sha = chip.querySelector('.gbt-badge__label')!;
      expect(text(sha)).toBe('abcdef12');
      expect(sha.getAttribute('title')).toBe('abcdef1234567890');
      expect(text(meta.querySelector('.pipeline-detail__message'))).toBe('Paginer la liste des tickets');
      const time = meta.querySelector('time')!;
      expect(time.getAttribute('datetime')).toBe(createdAt);
      expect(time.getAttribute('title')).toBe(absoluteDateTime(createdAt));
      expect(text(meta)).toBe(`abcdef12 Paginer la liste des tickets · déclenché ${relativeTime(createdAt)} par alice · durée 10s`);
    });

    it('leaves out the commit message and "par …" when they are unknown', () => {
      const el = load({ status: 'success', finishedAt: '2026-01-01T00:00:10Z', commitMessage: null, triggeredBy: null });

      const meta = el.querySelector('gbt-page-header .pipeline-detail__meta')!;
      expect(meta.querySelector('.pipeline-detail__message')).toBeNull();
      expect(text(meta)).toMatch(/^abcdef12 · déclenché .+ · durée 10s$/);
      expect(text(meta)).not.toContain('par');
    });

    it('offers "Annuler le pipeline" in the header actions to a writer while the pipeline runs, not to a reader', () => {
      const writer = load({ status: 'running' }, 'contributor');
      const cancel = Array.from(writer.querySelectorAll('.gbt-page-header__actions button')).find((button) => text(button) === 'Annuler le pipeline');
      expect(cancel).toBeTruthy();

      TestBed.resetTestingModule();
      const reader = load({ status: 'running' }, 'reader');
      expect(reader.querySelector('.gbt-page-header__actions button')).toBeNull();
    });
  });

  describe('views', () => {
    const jobs = [
      makeJob({ id: 'j1', stage: 'prepare', name: 'hello', status: 'success', startedAt: '2026-01-01T00:00:00Z', finishedAt: '2026-01-01T00:00:03Z' }),
      makeJob({ id: 'j2', stage: 'check', name: 'app-health', status: 'failed', logs: 'boom' }),
    ];

    function load(jobId: string | null) {
      const { fixture } = setup(jobId);
      TestBed.inject(HttpTestingController).expectOne(DETAIL_URL).flush(makePipeline({ status: 'failed', finishedAt: '2026-01-01T00:00:10Z', jobs }));
      fixture.detectChanges();
      return fixture.nativeElement as HTMLElement;
    }

    it('renders the sidebar and the summary when no job is selected', () => {
      const el = load(null);
      expect(el.querySelector('fg-pipeline-sidebar')).not.toBeNull();
      expect(el.querySelector('fg-pipeline-summary')).not.toBeNull();
      expect(el.querySelector('fg-pipeline-job')).toBeNull();
    });

    it('renders the job view for the selected job', () => {
      const el = load('j2');
      expect(el.querySelector('fg-pipeline-job')).not.toBeNull();
      expect(el.querySelector('fg-pipeline-summary')).toBeNull();
      expect(el.querySelector('.pipeline-job__logs')?.textContent).toContain('boom');
    });

    it('shows a not-found message with a link back to the summary for an unknown job', () => {
      const el = load('missing');
      expect(el.querySelector('fg-pipeline-job')).toBeNull();
      expect(el.querySelector('.pipeline-detail__not-found')?.textContent).toContain('introuvable');
      expect(el.querySelector('.pipeline-detail__not-found a')?.getAttribute('href')).toBe('/repositories/admin/my-repo/-/pipelines/pipeline-1');
    });

    it('shows the pipeline duration in the header', () => {
      const el = load(null);
      expect(el.querySelector('.pipeline-detail__duration')?.textContent).toContain('10s');
    });
  });

  describe('invalid pipeline file', () => {
    function loadFailed(overrides: Partial<PipelineDetailModel> = {}) {
      const { fixture } = setup();
      TestBed.inject(HttpTestingController).expectOne(DETAIL_URL).flush(
        makePipeline({ status: 'failed', finishedAt: '2026-01-01T00:00:00Z', error: 'job \'a\' needs \'ghost\', which is not a declared job', ...overrides }),
      );
      fixture.detectChanges();
      return fixture.nativeElement as HTMLElement;
    }

    it('explains the failure in an error alert with the parser message in a fixed-width block', () => {
      const el = loadFailed();

      const alert = el.querySelector('gbt-alert.pipeline-detail__error')!;
      expect(alert.textContent).toContain('Fichier de pipeline invalide');
      const message = alert.querySelector('pre.pipeline-detail__error-message')!;
      expect(message.textContent).toBe("job 'a' needs 'ghost', which is not a declared job");
    });

    it('shows the failed badge and no job navigation, since the pipeline has no job', () => {
      const el = loadFailed();

      expect(el.querySelector('gbt-page-header fg-status-badge')?.textContent?.trim()).toBe('Échoué');
      expect(el.querySelector('fg-pipeline-sidebar')).toBeNull();
      expect(el.querySelector('fg-pipeline-summary')).toBeNull();
    });

    it('does not show the alert for a pipeline whose file was fine', () => {
      const { fixture } = setup();
      TestBed.inject(HttpTestingController).expectOne(DETAIL_URL).flush(makePipeline({ status: 'failed', finishedAt: '2026-01-01T00:00:10Z', jobs: [makeJob({ status: 'failed' })] }));
      fixture.detectChanges();

      const el = fixture.nativeElement as HTMLElement;
      expect(el.querySelector('gbt-alert')).toBeNull();
      expect(el.querySelector('fg-pipeline-sidebar')).not.toBeNull();
    });

    it('stops polling: an invalid pipeline is final from the start', () => {
      vi.useFakeTimers();
      try {
        const { fixture } = setup();
        const httpMock = TestBed.inject(HttpTestingController);
        httpMock.expectOne(DETAIL_URL).flush(makePipeline({ status: 'failed', error: 'invalid YAML: boom' }));
        fixture.detectChanges();

        vi.advanceTimersByTime(15000);
        httpMock.expectNone(DETAIL_URL);
      } finally {
        vi.useRealTimers();
      }
    });
  });

  describe('skipped jobs', () => {
    it('shows them as "Ignoré" next to the failed job and keeps the pipeline failed', () => {
      const { fixture } = setup();
      TestBed.inject(HttpTestingController).expectOne(DETAIL_URL).flush(
        makePipeline({
          status: 'failed',
          finishedAt: '2026-01-01T00:00:10Z',
          jobs: [
            makeJob({ id: 'j1', stage: 'build', name: 'compile', status: 'failed', startedAt: '2026-01-01T00:00:00Z', finishedAt: '2026-01-01T00:00:05Z' }),
            makeJob({ id: 'j2', stage: 'test', name: 'unit', status: 'skipped' }),
          ],
        }),
      );
      fixture.detectChanges();

      const el = fixture.nativeElement as HTMLElement;
      const sidebarLink = [...el.querySelectorAll('fg-pipeline-sidebar a')].find((a) => a.textContent?.includes('unit'))!;
      expect(sidebarLink.querySelector('.sr-only')?.textContent?.trim()).toBe('Ignoré');
      expect(el.querySelector('gbt-page-header fg-status-badge')?.textContent?.trim()).toBe('Échoué');
    });
  });

  describe('running pipeline', () => {
    it('shows the "En cours" badge and offers to cancel it', () => {
      const { fixture } = setup();
      TestBed.inject(HttpTestingController).expectOne(DETAIL_URL).flush(makePipeline({ status: 'running', jobs: [makeJob({ status: 'running', startedAt: '2026-01-01T00:00:00Z' })] }));
      fixture.detectChanges();

      const status = fixture.nativeElement.querySelector('.pipeline-detail__status') as HTMLElement;
      expect(status.textContent?.trim()).toBe('En cours');
      expect(status.classList).toContain('pipeline-detail__status--running');
    });
  });

  describe('header', () => {
    function header(overrides: Partial<PipelineDetailModel>) {
      const { fixture } = setup();
      TestBed.inject(HttpTestingController).expectOne(DETAIL_URL).flush(makePipeline(overrides));
      fixture.detectChanges();
      return fixture.nativeElement as HTMLElement;
    }

    it.each(['success', 'failed', 'canceled'] as const)('shows a dash as the duration of a %s pipeline without a finish time', (status) => {
      const el = header({ status, finishedAt: null });
      expect(el.querySelector('.pipeline-detail__duration')?.textContent?.trim()).toBe('—');
    });

    it('shows the real duration of a finished pipeline', () => {
      const el = header({ status: 'success', finishedAt: '2026-01-01T00:00:10Z' });
      expect(el.querySelector('.pipeline-detail__duration')?.textContent?.trim()).toBe('10s');
    });

    it('shows a live duration for a running pipeline', () => {
      const el = header({ status: 'running', createdAt: new Date(Date.now() - 65_000).toISOString() });
      expect(el.querySelector('.pipeline-detail__duration')?.textContent?.trim()).toMatch(/^1m \d+s$/);
    });

    it('shows the French status label while keeping the raw status as CSS modifier', () => {
      const el = header({ status: 'failed', finishedAt: '2026-01-01T00:00:10Z' });
      const status = el.querySelector('.pipeline-detail__status') as HTMLElement;
      expect(status.textContent?.trim()).toBe('Échoué');
      expect(status.classList).toContain('pipeline-detail__status--failed');
    });
  });

  it('selects the stage named in the ?stage query parameter', async () => {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([]), { provide: LOCALE_ID, useValue: 'fr' }] });
    await TestBed.inject(Router).navigateByUrl('/?stage=check');
    const fixture = TestBed.createComponent(PipelineDetail);
    fixture.componentRef.setInput('repositoryId', 'repo-1');
    fixture.componentRef.setInput('pipelineId', 'pipeline-1');
    fixture.componentRef.setInput('path', ['admin', 'my-repo']);
    fixture.detectChanges();
    TestBed.inject(HttpTestingController)
      .expectOne(DETAIL_URL)
      .flush(makePipeline({ jobs: [makeJob({ id: 'j1', stage: 'prepare', name: 'hello' }), makeJob({ id: 'j2', stage: 'check', name: 'a' })] }));
    fixture.detectChanges();

    const steps = fixture.nativeElement.querySelectorAll('.gbt-stepper__step');
    expect(steps[1].hasAttribute('data-selected')).toBe(true);
  });

  describe('polling', () => {
    beforeEach(() => {
      vi.useFakeTimers();
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    it('keeps polling with new HTTP requests while the pipeline is non-terminal', () => {
      const { fixture } = setup();
      const httpMock = TestBed.inject(HttpTestingController);

      httpMock.expectOne(DETAIL_URL).flush(makePipeline({ status: 'running' }));
      fixture.detectChanges();

      vi.advanceTimersByTime(3000);
      httpMock.expectOne(DETAIL_URL).flush(makePipeline({ status: 'running' }));
      fixture.detectChanges();

      vi.advanceTimersByTime(3000);
      httpMock.expectOne(DETAIL_URL).flush(makePipeline({ status: 'pending' }));
      fixture.detectChanges();

      httpMock.verify();
    });

    it('stops polling once a poll response reports a terminal status', () => {
      const { fixture } = setup();
      const httpMock = TestBed.inject(HttpTestingController);

      httpMock.expectOne(DETAIL_URL).flush(makePipeline({ status: 'running' }));
      fixture.detectChanges();

      vi.advanceTimersByTime(3000);
      httpMock.expectOne(DETAIL_URL).flush(makePipeline({ status: 'success' }));
      fixture.detectChanges();

      vi.advanceTimersByTime(15000);
      httpMock.expectNone(DETAIL_URL);
    });

    it('does not restart polling when the initial response is already terminal', () => {
      const { fixture } = setup();
      const httpMock = TestBed.inject(HttpTestingController);

      httpMock.expectOne(DETAIL_URL).flush(makePipeline({ status: 'failed' }));
      fixture.detectChanges();

      vi.advanceTimersByTime(15000);
      httpMock.expectNone(DETAIL_URL);
    });

    it('ngOnDestroy clears the poll interval so no further HTTP request fires after teardown', () => {
      const { fixture } = setup();
      const httpMock = TestBed.inject(HttpTestingController);

      httpMock.expectOne(DETAIL_URL).flush(makePipeline({ status: 'running' }));
      fixture.detectChanges();

      fixture.destroy();

      vi.advanceTimersByTime(15000);
      httpMock.expectNone(DETAIL_URL);
    });

    it('drops a stale response that resolves after a newer request has already been issued', () => {
      const { fixture } = setup();
      const httpMock = TestBed.inject(HttpTestingController);

      // Request A (from ngOnInit) is still in flight when B starts. Calling the private refresh() directly makes the overlap deterministic.
      const [requestA] = httpMock.match(DETAIL_URL);
      expect(requestA).toBeTruthy();
      (fixture.componentInstance as unknown as { refresh(): void })['refresh']();
      const [requestB] = httpMock.match(DETAIL_URL);
      expect(requestB).toBeTruthy();

      requestB.flush(makePipeline({ status: 'running', commitSha: 'from-b' }));
      fixture.detectChanges();

      // A, the older request, resolves after B: its response has to be dropped.
      requestA.flush(makePipeline({ status: 'running', commitSha: 'from-a' }));
      fixture.detectChanges();

      expect(fixture.nativeElement.textContent).toContain('from-b');
      expect(fixture.nativeElement.textContent).not.toContain('from-a');
    });
  });
});
