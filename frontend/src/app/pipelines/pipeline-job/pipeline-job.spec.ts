import { TestBed } from '@angular/core/testing';
import { PipelineJob } from './pipeline-job';
import { activeStageIndex, groupByStage } from '../pipeline-helpers';
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

function setup(target: JobSummary) {
  const jobs = [job('hello', 'prepare', 'success'), target];
  const groups = groupByStage(jobs);
  const fixture = TestBed.createComponent(PipelineJob);
  fixture.componentRef.setInput('job', target);
  fixture.componentRef.setInput('groups', groups);
  fixture.componentRef.setInput('activeStageIndex', activeStageIndex(groups));
  fixture.componentRef.setInput('now', Date.parse('2026-01-01T00:01:00Z'));
  fixture.detectChanges();
  return fixture;
}

describe('PipelineJob', () => {
  it('shows the job name, status and duration', () => {
    const el = setup(job('app-health', 'check', 'running', { startedAt: '2026-01-01T00:00:00Z' })).nativeElement as HTMLElement;
    expect(el.querySelector('h2')?.textContent).toContain('app-health');
    expect(el.textContent).toContain('En cours');
    expect(el.querySelector('.pipeline-job__duration')?.textContent).toContain('1m 0s');
  });

  it.each([
    ['success', 'Réussi'],
    ['failed', 'Échoué'],
    ['canceled', 'Annulé'],
    ['running', 'En cours'],
    ['pending', 'En attente'],
  ] as const)('puts the status glyph of a %s job in the heading, named "%s" for screen readers', (status, label) => {
    const el = setup(job('app-health', 'check', status)).nativeElement as HTMLElement;
    const glyph = el.querySelector('h2 gbt-job-status');
    expect(glyph?.getAttribute('data-status')).toBe(status);
    expect(glyph?.querySelector('.sr-only')?.textContent?.trim()).toBe(label);
  });

  it('lists the jobs it depends on', () => {
    const el = setup(job('summary', 'report', 'pending', { needs: ['a', 'b'] })).nativeElement as HTMLElement;
    expect(el.querySelector('.pipeline-job__needs')?.textContent).toContain('a, b');
  });

  it('selects the stepper step of the job stage and emits the stage on click', () => {
    const fixture = setup(job('app-health', 'check', 'running'));
    const steps = fixture.nativeElement.querySelectorAll('.gbt-stepper__step');
    expect(steps[1].hasAttribute('data-selected')).toBe(true);
    const emitted: string[] = [];
    fixture.componentInstance.stageSelected.subscribe((s) => emitted.push(s));
    (fixture.nativeElement.querySelectorAll('.gbt-stepper__button')[0] as HTMLButtonElement).click();
    expect(emitted).toEqual(['prepare']);
  });

  it('renders the logs, or a placeholder when empty', () => {
    expect(setup(job('a', 'check', 'success', { logs: 'hello\nworld' })).nativeElement.querySelector('.pipeline-job__logs').textContent).toContain('hello');
    expect(setup(job('b', 'check', 'pending')).nativeElement.querySelector('.pipeline-job__logs').textContent).toContain('(pas encore de logs)');
  });

  it('exposes the logs as a named, focusable region', () => {
    const logs = setup(job('app-health', 'check', 'running', { logs: 'x' })).nativeElement.querySelector('.pipeline-job__logs') as HTMLElement;
    expect(logs.getAttribute('role')).toBe('region');
    expect(logs.getAttribute('aria-label')).toBe('Logs de app-health');
    expect(logs.getAttribute('tabindex')).toBe('0');
  });

  it('stops following the bottom once the user scrolls up, and resumes when back at the bottom', () => {
    const fixture = setup(job('a', 'check', 'running', { logs: 'x' }));
    const component = fixture.componentInstance as unknown as { follow(): boolean; onScroll(el: { scrollHeight: number; scrollTop: number; clientHeight: number }): void };
    expect(component.follow()).toBe(true);
    component.onScroll({ scrollHeight: 1000, scrollTop: 0, clientHeight: 100 });
    expect(component.follow()).toBe(false);
    component.onScroll({ scrollHeight: 1000, scrollTop: 900, clientHeight: 100 });
    expect(component.follow()).toBe(true);
  });
  describe('log following', () => {
    const logsOf = (fixture: ReturnType<typeof setup>) => fixture.nativeElement.querySelector('.pipeline-job__logs') as HTMLElement;
    const stubGeometry = (el: HTMLElement) => {
      Object.defineProperty(el, 'scrollHeight', { value: 1000, configurable: true });
      Object.defineProperty(el, 'clientHeight', { value: 100, configurable: true });
    };
    const update = (fixture: ReturnType<typeof setup>, next: JobSummary) => {
      fixture.componentRef.setInput('job', next);
      fixture.detectChanges();
    };

    it('scrolls to the bottom when a running job receives new logs', () => {
      const fixture = setup(job('a', 'check', 'running', { logs: 'x' }));
      const el = logsOf(fixture);
      stubGeometry(el);
      update(fixture, job('a', 'check', 'running', { logs: 'x\ny' }));
      expect(el.scrollTop).toBe(1000);
    });

    it('does not scroll a job that is not running', () => {
      const fixture = setup(job('a', 'check', 'success', { logs: 'x' }));
      const el = logsOf(fixture);
      stubGeometry(el);
      update(fixture, job('a', 'check', 'success', { logs: 'x\ny' }));
      expect(el.scrollTop).toBe(0);
    });

    it('resets following when switching to another job', () => {
      const fixture = setup(job('a', 'check', 'running', { logs: 'x' }));
      const component = fixture.componentInstance as unknown as { follow(): boolean; onScroll(el: { scrollHeight: number; scrollTop: number; clientHeight: number }): void };
      const el = logsOf(fixture);
      stubGeometry(el);
      component.onScroll({ scrollHeight: 1000, scrollTop: 0, clientHeight: 100 });
      expect(component.follow()).toBe(false);
      update(fixture, job('b', 'check', 'running', { logs: 'z' }));
      expect(component.follow()).toBe(true);
      expect(el.scrollTop).toBe(1000);
    });
  });
});
