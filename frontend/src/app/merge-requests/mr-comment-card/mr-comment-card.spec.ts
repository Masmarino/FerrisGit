import { LOCALE_ID } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { MrCommentCard } from './mr-comment-card';

describe('MrCommentCard', () => {
  // Freeze "now" so the relative date cannot flip at a minute or hour boundary. Only Date is faked.
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['Date'] });
    vi.setSystemTime(new Date(2026, 8, 24, 15, 0, 0));
    TestBed.configureTestingModule({ providers: [{ provide: LOCALE_ID, useValue: 'fr' }] });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  function setup(inputs: { author?: { id: string; username: string } | null; createdAt?: string; body?: string; badge?: string | null; verb?: string }) {
    const fixture = TestBed.createComponent(MrCommentCard);
    fixture.componentRef.setInput('author', inputs.author === undefined ? { id: 'u1', username: 'alice' } : inputs.author);
    fixture.componentRef.setInput('createdAt', inputs.createdAt ?? '2026-09-23T10:05:00');
    fixture.componentRef.setInput('body', inputs.body ?? 'Un commentaire');
    if (inputs.verb !== undefined) {
      fixture.componentRef.setInput('verb', inputs.verb);
    }
    if (inputs.badge !== undefined) {
      fixture.componentRef.setInput('badge', inputs.badge);
    }
    fixture.detectChanges();
    return fixture;
  }

  it('shows the author username, the default verb and the body', () => {
    const fixture = setup({});

    const text = fixture.nativeElement.textContent as string;
    expect(text).toContain('alice');
    expect(fixture.nativeElement.querySelector('.mr-comment__verb')?.textContent?.trim()).toBe('a commenté');
    expect(text).toContain('Un commentaire');
  });

  it('shows the date relative to now, with the exact date-time in the title', () => {
    const fixture = setup({ createdAt: '2026-09-23T10:05:00' });

    const time: HTMLTimeElement = fixture.nativeElement.querySelector('time.mr-comment__date');
    expect(time.getAttribute('datetime')).toBe('2026-09-23T10:05:00');
    expect(time.getAttribute('title')).toBe('23/09/2026 10:05');
    expect(time.textContent?.trim()).toBe('hier');
  });

  it('shows the given verb', () => {
    const fixture = setup({ verb: 'a ouvert cette demande de fusion' });

    expect(fixture.nativeElement.querySelector('.mr-comment__verb')?.textContent?.trim()).toBe('a ouvert cette demande de fusion');
  });

  it('falls back to "Utilisateur supprimé" when the author\'s account was deleted', () => {
    const fixture = setup({ author: null });

    expect(fixture.nativeElement.textContent).toContain('Utilisateur supprimé');
  });

  it('shows the badge text when given', () => {
    const fixture = setup({ badge: 'Auteur' });

    const badge = fixture.nativeElement.querySelector('.mr-comment__badge');
    expect(badge?.textContent?.trim()).toBe('Auteur');
  });

  it('draws the comment as an outlined card: byline and date in its header above the box, the text in the body', () => {
    const fixture = setup({ badge: 'Auteur' });

    const host: HTMLElement = fixture.nativeElement.querySelector('gbt-card');
    const header = host.querySelector(':scope > .gbt-card__header')!;
    const card = host.querySelector(':scope > .gbt-card')!;
    expect(Array.from(host.children)).toEqual([header, card]);
    expect(card.getAttribute('data-variant')).toBe('outlined');
    expect(header.querySelector('.mr-comment__author')).not.toBeNull();
    expect(header.querySelector('time.mr-comment__date')).not.toBeNull();
    expect(card.querySelector('.gbt-card__body .mr-comment__body')).not.toBeNull();
  });

  it('renders no badge by default', () => {
    const fixture = setup({});

    expect(fixture.nativeElement.querySelector('.mr-comment__badge')).toBeNull();
  });
});
