import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { PipelineSummary } from './pipeline-summary';
import { activeStageIndex, groupByStage } from '../pipeline-helpers';
import { JobSummary } from '../pipelines.service';

const job = (id: string, stage: string, status: JobSummary['status'], needs: string[] = []): JobSummary => ({
  id,
  stage,
  name: id,
  status,
  needs,
  tags: [],
  logs: '',
  createdAt: '2026-01-01T00:00:00Z',
  startedAt: null,
  finishedAt: null,
});

const defaultJobs: JobSummary[] = [
  job('hello', 'prepare', 'success'),
  job('a', 'check', 'failed', ['hello']),
  job('b', 'check', 'running', ['hello']),
  job('s', 'report', 'pending', ['a', 'b']),
];

function setup(selectedStage: string | null = null, jobs: JobSummary[] = defaultJobs) {
  TestBed.configureTestingModule({ providers: [provideRouter([])] });
  const fixture = TestBed.createComponent(PipelineSummary);
  const groups = groupByStage(jobs);
  fixture.componentRef.setInput('groups', groups);
  fixture.componentRef.setInput('activeStageIndex', activeStageIndex(groups));
  fixture.componentRef.setInput('selectedStage', selectedStage);
  fixture.componentRef.setInput('path', ['admin', 'my-repo']);
  fixture.componentRef.setInput('pipelineId', 'pipe-1');
  fixture.componentRef.setInput('now', Date.parse('2026-01-01T00:01:00Z'));
  fixture.detectChanges();
  return fixture;
}

describe('PipelineSummary', () => {
  it('renders an interactive stepper with one step per stage', () => {
    const el = setup().nativeElement as HTMLElement;
    expect(el.querySelectorAll('.gbt-stepper__button').length).toBe(3);
  });

  it('marks the stage with a failed job as an error', () => {
    const el = setup().nativeElement as HTMLElement;
    const statuses = [...el.querySelectorAll('.gbt-stepper__step')].map((s) => s.getAttribute('data-status'));
    expect(statuses).toEqual(['completed', 'error', 'upcoming']);
  });

  it('emits the stage name when a step is clicked', () => {
    const fixture = setup();
    const emitted: string[] = [];
    fixture.componentInstance.stageSelected.subscribe((name) => emitted.push(name));
    (fixture.nativeElement.querySelectorAll('.gbt-stepper__button')[2] as HTMLButtonElement).click();
    expect(emitted).toEqual(['report']);
  });

  it('marks the selected stage in the stepper', () => {
    const el = setup('check').nativeElement as HTMLElement;
    expect(el.querySelectorAll('.gbt-stepper__step')[1].hasAttribute('data-selected')).toBe(true);
  });

  it('renders the job graph with one node per job', () => {
    const el = setup().nativeElement as HTMLElement;
    expect(el.querySelectorAll('button.gbt-job-graph__node').length).toBe(4);
  });

  it('emits the job id when a graph node is clicked', () => {
    const fixture = setup();
    const emitted: string[] = [];
    fixture.componentInstance.jobOpened.subscribe((id) => emitted.push(id));
    (fixture.nativeElement.querySelector('button.gbt-job-graph__node[data-job-name="a"]') as HTMLButtonElement).click();
    expect(emitted).toEqual(['a']);
  });

  it('lists the selected stage jobs as links to their job pages', () => {
    const el = setup('check').nativeElement as HTMLElement;
    const hrefs = [...el.querySelectorAll('.pipeline-summary__stage-jobs a')].map((a) => a.getAttribute('href'));
    expect(hrefs).toEqual([
      '/repositories/admin/my-repo/-/pipelines/pipe-1/jobs/a',
      '/repositories/admin/my-repo/-/pipelines/pipe-1/jobs/b',
    ]);
  });

  it('marks each job of the selected stage with its status glyph, named in French for screen readers', () => {
    const el = setup('check').nativeElement as HTMLElement;
    const glyphs = [...el.querySelectorAll('.pipeline-summary__stage-jobs a gbt-job-status')];
    expect(glyphs.map((g) => g.getAttribute('data-status'))).toEqual(['failed', 'running']);
    expect(glyphs.map((g) => g.querySelector('.sr-only')?.textContent?.trim())).toEqual(['Échoué', 'En cours']);
  });

  it('shows no stage job list when no stage is selected', () => {
    const el = setup(null).nativeElement as HTMLElement;
    expect(el.querySelector('.pipeline-summary__stage-jobs')).toBeNull();
  });
});
