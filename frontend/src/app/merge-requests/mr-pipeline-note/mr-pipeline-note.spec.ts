import { LOCALE_ID } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { MrPipelineNote } from './mr-pipeline-note';
import { TimelinePipeline } from '../merge-requests.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { ALICE } from '../merge-request-fixtures';

const PIPELINE: TimelinePipeline = {
  type: 'pipeline',
  id: 'p1',
  status: 'failed',
  commitSha: 'merge0000000000000000000000000000000000000',
  sourceSha: 'abcdef1234567890abcdef1234567890abcdef12',
  createdAt: '2026-09-23T10:05:00',
  finishedAt: null,
  triggeredBy: ALICE,
};

describe('MrPipelineNote', () => {
  function setup(path: string[] | null) {
    TestBed.configureTestingModule({
      providers: [
        provideRouter([]),
        { provide: LOCALE_ID, useValue: 'fr' },
        { provide: RepositoryContextService, useValue: { current: () => (path ? { path } : null) } },
      ],
    });
    const fixture = TestBed.createComponent(MrPipelineNote);
    fixture.componentRef.setInput('pipeline', PIPELINE);
    fixture.detectChanges();
    return fixture.nativeElement as HTMLElement;
  }

  it('says who started it, on which source commit, and how it is doing', () => {
    const el = setup(null);

    expect(el.querySelector('strong')?.textContent).toBe('alice');
    expect(el.textContent).toContain('a lancé la pipeline de la fusion, sur le commit');
    expect(el.textContent).toContain('abcdef1');
    expect(el.textContent).toContain('Échouée');
    expect(el.querySelector('a')).toBeNull();
  });

  it('links to the pipeline page once the repository path is known', () => {
    const el = setup(['acme', 'widget']);

    expect(el.querySelector('a')?.getAttribute('href')).toBe('/repositories/acme/widget/-/pipelines/p1');
  });
});
