import { LOCALE_ID } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { MrSystemNote } from './mr-system-note';
import { TimelineEvent } from '../merge-requests.service';
import { describeEvent } from '../merge-request-events';
import { eventFixture } from '../merge-request-fixtures';

describe('MrSystemNote', () => {
  // Freeze "now" so the relative date can't flip at a minute or hour boundary. Only Date is faked.
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(new Date(2026, 8, 24, 15, 0, 0));
    TestBed.configureTestingModule({ providers: [{ provide: LOCALE_ID, useValue: 'fr' }] });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  function makeEvent(overrides: Partial<TimelineEvent> = {}): TimelineEvent {
    return eventFixture({ id: 'e1', createdAt: '2026-09-23T10:05:00', kind: 'review_submitted', payload: { decision: 'approved' }, ...overrides });
  }

  function setup(event: TimelineEvent) {
    const fixture = TestBed.createComponent(MrSystemNote);
    fixture.componentRef.setInput('event', event);
    fixture.detectChanges();
    return fixture;
  }

  it('renders the actor username in bold followed by the described sentence', () => {
    const event = makeEvent();
    const fixture = setup(event);

    const note: HTMLElement = fixture.nativeElement.querySelector('.mr-system-note');
    expect(note.querySelector('strong')?.textContent).toBe('alice');
    expect(note.textContent).toContain(describeEvent(event));
    expect(note.textContent).toContain('a approuvé cette demande de fusion');
  });

  it('shows the date relative to now, with the exact date-time in the title', () => {
    const fixture = setup(makeEvent());

    const time: HTMLTimeElement = fixture.nativeElement.querySelector('time.mr-system-note__date');
    expect(time.getAttribute('datetime')).toBe('2026-09-23T10:05:00');
    expect(time.getAttribute('title')).toBe('23/09/2026 10:05');
    expect(time.textContent?.trim()).toBe('hier');
  });

  it('renders label names as tags in their colour, and a label without colour as a neutral badge', () => {
    const fixture = setup(
      makeEvent({
        kind: 'labels_changed',
        payload: { added: [{ id: 'l1', name: 'bug', color: '#d73a4a' }], removed: [{ id: 'l2', name: 'vieux' }] },
      }),
    );

    const note: HTMLElement = fixture.nativeElement.querySelector('.mr-system-note');
    expect(note.querySelector('gbt-tag.mr-system-note__label')?.textContent?.trim()).toBe('bug');
    expect(note.querySelector('gbt-badge.mr-system-note__label')?.textContent?.trim()).toBe('vieux');
    expect(note.querySelector('.mr-system-note__text')?.textContent?.replace(/\s+/g, ' ').trim()).toBe('alice a ajouté le label bug et retiré le label vieux');
  });

  it('tints the marker by tone: success for an approval, error for requested changes, neutral otherwise', () => {
    const tone = (event: TimelineEvent) => setup(event).nativeElement.querySelector('gbt-icon-marker .gbt-icon-marker').getAttribute('data-tone');

    expect(tone(makeEvent())).toBe('success');
    expect(tone(makeEvent({ payload: { decision: 'changes_requested' } }))).toBe('error');
    expect(tone(makeEvent({ kind: 'closed', payload: {} }))).toBe('neutral');
  });

  it('hangs a small decorative outline marker on the rail: the sentence says what happened, the marker only repeats it', () => {
    const marker = setup(makeEvent()).nativeElement.querySelector('.mr-system-note__marker') as HTMLElement;

    expect(marker.tagName).toBe('GBT-ICON-MARKER');
    const disc = marker.querySelector('.gbt-icon-marker')!;
    expect(disc.getAttribute('data-size')).toBe('sm');
    expect(disc.getAttribute('data-appearance')).toBe('outline');
    expect(disc.getAttribute('aria-hidden')).toBe('true');
    expect(disc.hasAttribute('role')).toBe(false);
  });

  it('renders only the sentence, without a <strong>, when there is no actor', () => {
    const event = makeEvent({ actor: null, kind: 'merged', payload: { mergeCommitSha: 'abcdef123456' } });
    const fixture = setup(event);

    const note: HTMLElement = fixture.nativeElement.querySelector('.mr-system-note');
    expect(note.querySelector('strong')).toBeNull();
    expect(note.textContent).toContain('Fusionnée');
    expect(note.querySelector('gbt-badge .gbt-badge__label')?.textContent).toBe('abcdef1');
  });
});
