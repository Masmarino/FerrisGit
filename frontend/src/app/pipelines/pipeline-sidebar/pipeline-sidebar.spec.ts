import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { PipelineSidebar } from './pipeline-sidebar';
import { groupByStage } from '../pipeline-helpers';
import { JobSummary } from '../pipelines.service';

const job = (id: string, stage: string, status: JobSummary['status'], extra: Partial<JobSummary> = {}): JobSummary => ({
  id,
  stage,
  name: id,
  status,
  needs: [],
  tags: [],
  logs: '',
  createdAt: '2026-01-01T00:00:00Z',
  startedAt: null,
  finishedAt: null,
  ...extra,
});

function setup(selectedJobId: string | null = null, jobs: JobSummary[] = [
  job('hello', 'prepare', 'success', { startedAt: '2026-01-01T00:00:00Z', finishedAt: '2026-01-01T00:00:03Z' }),
  job('a', 'check', 'running'),
]) {
  TestBed.configureTestingModule({ providers: [provideRouter([])] });
  const fixture = TestBed.createComponent(PipelineSidebar);
  fixture.componentRef.setInput('path', ['admin', 'my-repo']);
  fixture.componentRef.setInput('pipelineId', 'pipe-1');
  fixture.componentRef.setInput('groups', groupByStage(jobs));
  fixture.componentRef.setInput('selectedJobId', selectedJobId);
  fixture.componentRef.setInput('now', Date.parse('2026-01-01T00:01:00Z'));
  fixture.detectChanges();
  return fixture.nativeElement as HTMLElement;
}

describe('PipelineSidebar', () => {
  it('links the summary and every job with absolute repository URLs', () => {
    const el = setup();
    const hrefs = [...el.querySelectorAll('a')].map((a) => a.getAttribute('href'));
    expect(hrefs).toEqual([
      '/repositories/admin/my-repo/-/pipelines/pipe-1',
      '/repositories/admin/my-repo/-/pipelines/pipe-1/jobs/hello',
      '/repositories/admin/my-repo/-/pipelines/pipe-1/jobs/a',
    ]);
  });

  it('groups the jobs under their stage headings', () => {
    const el = setup();
    expect([...el.querySelectorAll('h3')].map((h) => h.textContent?.trim())).toEqual(['prepare', 'check']);
  });

  it('marks the summary as the current page when no job is selected', () => {
    const el = setup(null);
    expect(el.querySelector('a')?.getAttribute('aria-current')).toBe('page');
  });

  it('marks the selected job as the current page and not the summary', () => {
    const el = setup('a');
    const links = [...el.querySelectorAll('a')];
    expect(links[0].getAttribute('aria-current')).toBeNull();
    expect(links[2].getAttribute('aria-current')).toBe('page');
  });

  it('shows each job duration, a dash when it has not started', () => {
    const el = setup();
    const durations = [...el.querySelectorAll('.pipeline-sidebar__duration')].map((d) => d.textContent?.trim());
    expect(durations).toEqual(['3s', '—']);
  });

  it.each([
    ['success', 'Réussi'],
    ['failed', 'Échoué'],
    ['canceled', 'Annulé'],
    ['running', 'En cours'],
    ['pending', 'En attente'],
  ] as const)('shows a %s job with Gabarit\'s status glyph, named "%s" for screen readers', (status, label) => {
    const el = setup(null, [job('build', 'check', status)]);
    const glyph = el.querySelector('.pipeline-sidebar__link gbt-job-status');
    expect(glyph?.getAttribute('data-status')).toBe(status);
    expect(glyph?.hasAttribute('aria-hidden')).toBe(false);
    expect(glyph?.querySelector('.sr-only')?.textContent?.trim()).toBe(label);
  });

  it('shows a skipped job with the label "Ignoré" and "ignoré" as its duration', () => {
    const el = setup(null, [job('build', 'prepare', 'failed'), job('deploy', 'report', 'skipped')]);
    const link = [...el.querySelectorAll('a')].find((a) => a.textContent?.includes('deploy'))!;
    expect(link.querySelector('gbt-job-status .sr-only')?.textContent?.trim()).toBe('Ignoré');
    expect(link.querySelector('.pipeline-sidebar__duration')?.textContent?.trim()).toBe('ignoré');
  });
});
