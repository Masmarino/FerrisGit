import { TestBed } from '@angular/core/testing';
import { pipelineTemplates, PipelineTemplate } from './pipeline-catalog';
import { PipelineStarters } from './pipeline-starters';

describe('PipelineStarters', () => {
  function setup() {
    TestBed.configureTestingModule({});
    const fixture = TestBed.createComponent(PipelineStarters);
    const chosen: PipelineTemplate[] = [];
    fixture.componentInstance.chosen.subscribe((template) => chosen.push(template));
    fixture.detectChanges();
    return { el: fixture.nativeElement as HTMLElement, chosen };
  }

  it('offers every template with its stages', () => {
    const { el } = setup();

    expect(el.querySelectorAll('.starters__card')).toHaveLength(pipelineTemplates().length);
    expect(el.querySelector('[data-template="rust"] .starters__stages')?.textContent).toBe('check → test');
  });

  it('hands over the template that was clicked', () => {
    const { el, chosen } = setup();

    el.querySelector<HTMLButtonElement>('[data-template="node"]')!.click();

    expect(chosen.map((t) => t.id)).toEqual(['node']);
  });
});
