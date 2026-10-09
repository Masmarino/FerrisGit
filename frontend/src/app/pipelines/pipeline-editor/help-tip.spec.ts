import { TestBed } from '@angular/core/testing';
import { HelpTip } from './help-tip';

describe('HelpTip', () => {
  function setup() {
    TestBed.configureTestingModule({});
    const fixture = TestBed.createComponent(HelpTip);
    fixture.componentRef.setInput('help', { title: 'Une étape', body: "Un palier de la pipeline." });
    fixture.detectChanges();
    return { fixture, el: fixture.nativeElement as HTMLElement };
  }

  it('is a button named after what it explains', () => {
    const { el } = setup();

    expect(el.querySelector('button')!.getAttribute('aria-label')).toBe('Aide : Une étape');
  });

  it('opens its explanation on a click, and closes it on Escape', () => {
    const { fixture, el } = setup();
    const trigger = el.querySelector('button')!;

    trigger.click();
    fixture.detectChanges();
    expect(trigger.getAttribute('aria-expanded')).toBe('true');
    expect(el.querySelector('.help-tip__panel')?.textContent).toContain('Un palier de la pipeline.');

    trigger.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    fixture.detectChanges();
    expect(el.querySelector('.help-tip__panel')).toBeNull();
  });
});
