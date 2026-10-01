import { LOCALE_ID } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { MergeRequestTimeline } from './merge-request-timeline';
import { Comment, MergeRequestSummary, TimelineComment, TimelineEvent, TimelineItem, TimelineThread } from '../merge-requests.service';

const alice = { id: 'u1', username: 'alice' };
const bob = { id: 'u2', username: 'bob' };

function makeMergeRequest(overrides: Partial<MergeRequestSummary> = {}): MergeRequestSummary {
  return {
    id: 'mr-1',
    sourceBranch: 'feature',
    targetBranch: 'main',
    title: 'Panier persistant',
    description: 'Conserve le panier entre deux sessions.',
    status: 'open',
    mergeCommitSha: null,
    createdAt: '2026-09-22T08:00:00',
    closedAt: null,
    milestoneId: null,
    labels: [],
    author: alice,
    commentCount: 0,
    ...overrides,
  };
}

function makeComment(overrides: Partial<Comment> = {}): Comment {
  return {
    id: 'root',
    authorId: 'u2',
    author: bob,
    body: 'Pourquoi ce changement ?',
    createdAt: '2026-09-23T10:00:00',
    replyToId: null,
    filePath: 'src/lib.rs',
    lineNumber: 12,
    endLine: null,
    side: 'new',
    outdated: false,
    resolved: false,
    suggestedContent: null,
    appliedAt: null,
    appliedCommitSha: null,
    ...overrides,
  };
}

function makeThread(id: string, resolved: boolean): TimelineThread {
  return {
    type: 'thread',
    id,
    createdAt: '2026-09-23T10:00:00',
    filePath: 'src/lib.rs',
    lineNumber: 12,
    endLine: null,
    side: 'new',
    outdated: false,
    resolved,
    resolvedBy: resolved ? alice : null,
    resolvedAt: resolved ? '2026-09-23T11:00:00' : null,
    excerpt: [],
    root: makeComment({ id, resolved }),
    replies: [],
  };
}

const comment: TimelineComment = { type: 'comment', id: 'c1', createdAt: '2026-09-23T09:00:00', author: alice, body: 'Bonne idée !' };
const event: TimelineEvent = { type: 'event', id: 'e1', createdAt: '2026-09-23T09:30:00', actor: bob, kind: 'closed', payload: {} };

describe('MergeRequestTimeline', () => {
  beforeEach(() => {
    // The children's pipes (gbtDateTime/gbtRelativeTime) fall back to LOCALE_ID, 'en-US' in a bare TestBed; the app sets it in app.config.ts.
    TestBed.configureTestingModule({ providers: [{ provide: LOCALE_ID, useValue: 'fr' }] });
  });

  function setup(inputs: { items?: TimelineItem[]; mergeRequest?: MergeRequestSummary; canWrite?: boolean } = {}) {
    const fixture = TestBed.createComponent(MergeRequestTimeline);
    fixture.componentRef.setInput('mergeRequest', inputs.mergeRequest ?? makeMergeRequest());
    fixture.componentRef.setInput('author', alice);
    fixture.componentRef.setInput('items', inputs.items ?? [event, comment, makeThread('t-open', false), makeThread('t-open-2', false), makeThread('t-done', true)]);
    fixture.componentRef.setInput('canWrite', inputs.canWrite ?? false);
    fixture.detectChanges();
    return fixture;
  }

  const el = (fixture: { nativeElement: HTMLElement }) => fixture.nativeElement;
  const buttonByText = (root: HTMLElement, text: string) => Array.from(root.querySelectorAll('button')).find((b) => b.textContent?.trim() === text) as HTMLButtonElement | undefined;
  const entries = (fixture: { nativeElement: HTMLElement }) => Array.from(el(fixture).querySelectorAll('ol.mr-timeline > li'));
  const kinds = (fixture: { nativeElement: HTMLElement }) => entries(fixture).map((li) => li.firstElementChild?.tagName.toLowerCase());
  const counter = (fixture: { nativeElement: HTMLElement }) => el(fixture).querySelector('.mr-timeline__counter-text');
  const counterBadges = (fixture: { nativeElement: HTMLElement }) =>
    Array.from(el(fixture).querySelectorAll('.mr-timeline__counter gbt-badge')).map((badge) => [badge.textContent?.trim(), badge.querySelector('.gbt-badge')?.getAttribute('data-variant')]);

  it('renders the description card first, then one entry per item in the given order', () => {
    const fixture = setup();

    expect(kinds(fixture)).toEqual([
      'fg-mr-comment-card',
      'fg-mr-system-note',
      'fg-mr-comment-card',
      'fg-mr-thread-card',
      'fg-mr-thread-card',
      'fg-mr-thread-card',
    ]);
    const description = entries(fixture)[0];
    expect(description.textContent).toContain('alice');
    expect(description.textContent).toContain('Conserve le panier entre deux sessions.');
    expect(description.textContent).toContain('a ouvert cette demande de fusion');
    expect(description.querySelector('time')?.getAttribute('datetime')).toBe('2026-09-22T08:00:00');
    expect(description.querySelector('time')?.getAttribute('title')).toBe('22/09/2026 08:00');
    expect(description.querySelector('.mr-comment__badge')?.textContent?.trim()).toBe('Auteur');
  });

  it('omits the description card when the description is empty', () => {
    const fixture = setup({ mergeRequest: makeMergeRequest({ description: '' }) });

    expect(kinds(fixture)[0]).toBe('fg-mr-system-note');
    expect(el(fixture).querySelector('.mr-comment__badge')).toBeNull();
  });

  describe('discussion counter', () => {
    it('counts open and resolved threads with plurals', () => {
      const fixture = setup();

      expect(counter(fixture)?.textContent?.trim()).toBe('2 discussions ouvertes · 1 résolue');
      expect(counterBadges(fixture)).toEqual([
        ['2 ouvertes', 'warning'],
        ['1 résolue', 'success'],
      ]);
    });

    it('uses the singular for a single open thread and pluralises resolved threads', () => {
      const fixture = setup({ items: [makeThread('a', false), makeThread('b', true), makeThread('c', true)] });

      expect(counter(fixture)?.textContent?.trim()).toBe('1 discussion ouverte · 2 résolues');
      expect(counterBadges(fixture).map(([text]) => text)).toEqual(['1 ouverte', '2 résolues']);
    });

    it('uses the singular for zero open threads', () => {
      const fixture = setup({ items: [makeThread('b', true)] });

      expect(counter(fixture)?.textContent?.trim()).toBe('0 discussion ouverte · 1 résolue');
      expect(counterBadges(fixture)).toEqual([
        ['0 ouverte', 'neutral'],
        ['1 résolue', 'success'],
      ]);
    });

    it('is hidden when there are no threads', () => {
      const fixture = setup({ items: [comment, event] });

      expect(el(fixture).querySelector('.mr-timeline__counter')).toBeNull();
    });
  });

  describe('filter', () => {
    const radio = (fixture: { nativeElement: HTMLElement }, label: string) =>
      Array.from(el(fixture).querySelectorAll<HTMLButtonElement>('[role="radio"]')).find((b) => b.textContent?.trim() === label)!;

    it('offers Tout (selected by default), Discussions and Activité', () => {
      const fixture = setup();

      const radios = Array.from(el(fixture).querySelectorAll<HTMLButtonElement>('[role="radio"]'));
      expect(radios.map((b) => b.textContent?.trim())).toEqual(['Tout', 'Discussions', 'Activité']);
      expect(radios.map((b) => b.getAttribute('aria-checked'))).toEqual(['true', 'false', 'false']);
    });

    it('Discussions shows only comments and threads, plus the description card', () => {
      const fixture = setup();

      radio(fixture, 'Discussions').click();
      fixture.detectChanges();

      expect(kinds(fixture)).toEqual(['fg-mr-comment-card', 'fg-mr-comment-card', 'fg-mr-thread-card', 'fg-mr-thread-card', 'fg-mr-thread-card']);
    });

    it('Activité shows only events and hides the description card', () => {
      const fixture = setup();

      radio(fixture, 'Activité').click();
      fixture.detectChanges();

      expect(kinds(fixture)).toEqual(['fg-mr-system-note']);
    });

    it('Tout shows everything again', () => {
      const fixture = setup();
      radio(fixture, 'Activité').click();
      fixture.detectChanges();

      radio(fixture, 'Tout').click();
      fixture.detectChanges();

      expect(entries(fixture).length).toBe(6);
    });
  });

  describe('empty state', () => {
    it('shows "Aucune activité pour l\'instant." when there is nothing to show', () => {
      const fixture = setup({ items: [], mergeRequest: makeMergeRequest({ description: '' }) });

      expect(el(fixture).querySelector('.mr-timeline__empty')?.textContent?.trim()).toBe("Aucune activité pour l'instant.");
      expect(el(fixture).querySelector('ol.mr-timeline')).toBeNull();
    });

    it('shows it when the filter leaves nothing (Activité with only comments and no events)', () => {
      const fixture = setup({ items: [comment] });

      Array.from(el(fixture).querySelectorAll<HTMLButtonElement>('[role="radio"]')).find((b) => b.textContent?.trim() === 'Activité')!.click();
      fixture.detectChanges();

      expect(el(fixture).querySelector('.mr-timeline__empty')?.textContent?.trim()).toBe("Aucune activité pour l'instant.");
    });
  });

  describe('composer', () => {
    it('hangs a decorative pencil marker on the rail beside the composer', () => {
      const fixture = setup({ canWrite: true });
      fixture.detectChanges();

      const marker = el(fixture).querySelector('.mr-timeline__composer > gbt-icon-marker.mr-timeline__composer-marker');
      expect(marker?.querySelector('.gbt-icon-marker')?.getAttribute('aria-hidden')).toBe('true');
      expect(marker?.querySelector('.gbt-icon-marker')?.getAttribute('data-appearance')).toBe('outline');
    });

    async function typeInComposer(fixture: ReturnType<typeof setup>, value: string) {
      fixture.detectChanges();
      await fixture.whenStable();
      const textarea: HTMLTextAreaElement = el(fixture).querySelector('.mr-timeline__composer textarea')!;
      textarea.value = value;
      textarea.dispatchEvent(new Event('input'));
      fixture.detectChanges();
    }

    it('is absent without write access', () => {
      const fixture = setup({ canWrite: false });

      expect(el(fixture).querySelector('.mr-timeline__composer')).toBeNull();
      expect(buttonByText(el(fixture), 'Commenter')).toBeUndefined();
    });

    it('renders a labelled textarea and a comment button aligned to the end of the flex column (not stretched)', async () => {
      const fixture = setup({ canWrite: true });
      fixture.detectChanges();
      await fixture.whenStable();

      expect(el(fixture).querySelector('.mr-timeline__composer gbt-textarea label')?.textContent).toContain('Ajouter un commentaire');
      const button: HTMLElement = el(fixture).querySelector('.comment-button')!;
      expect(button.textContent?.trim()).toBe('Commenter');
      expect(getComputedStyle(button).alignSelf).toBe('flex-end');
    });

    it('emits commentAdded with the trimmed body and keeps the draft until the page says the POST succeeded', async () => {
      const fixture = setup({ canWrite: true });
      const added = vi.fn();
      fixture.componentInstance.commentAdded.subscribe(added);
      await typeInComposer(fixture, '  Bonjour tout le monde  ');

      buttonByText(el(fixture), 'Commenter')!.click();
      fixture.detectChanges();

      expect(added).toHaveBeenCalledExactlyOnceWith('Bonjour tout le monde');
      expect((el(fixture).querySelector('.mr-timeline__composer textarea') as HTMLTextAreaElement).value).toBe('  Bonjour tout le monde  ');

      buttonByText(el(fixture), 'Commenter')!.click();
      expect(added).toHaveBeenCalledTimes(2);
    });

    it('clearDraft() empties the draft signal and the native textarea', async () => {
      const fixture = setup({ canWrite: true });
      const added = vi.fn();
      fixture.componentInstance.commentAdded.subscribe(added);
      await typeInComposer(fixture, 'Bonjour');
      buttonByText(el(fixture), 'Commenter')!.click();

      fixture.componentInstance.clearDraft();
      fixture.detectChanges();
      // NgModel pushes the cleared draft into the field on a microtask.
      await fixture.whenStable();
      fixture.detectChanges();

      expect((el(fixture).querySelector('.mr-timeline__composer textarea') as HTMLTextAreaElement).value).toBe('');
      buttonByText(el(fixture), 'Commenter')!.click();
      expect(added).toHaveBeenCalledTimes(1);
    });

    it('ignores an empty or blank draft', async () => {
      const fixture = setup({ canWrite: true });
      const added = vi.fn();
      fixture.componentInstance.commentAdded.subscribe(added);

      buttonByText(el(fixture), 'Commenter')!.click();
      await typeInComposer(fixture, '   ');
      buttonByText(el(fixture), 'Commenter')!.click();

      expect(added).not.toHaveBeenCalled();
    });
  });

  describe('thread outputs', () => {
    it('re-emits replyAdded, resolveToggled and applySuggestionClicked unchanged', async () => {
      const thread = makeThread('t-open', false);
      thread.root = makeComment({ id: 't-open', suggestedContent: 'let b = 3;' });
      const fixture = setup({ items: [thread], canWrite: true });
      const replyAdded = vi.fn();
      const resolveToggled = vi.fn();
      const applySuggestionClicked = vi.fn();
      fixture.componentInstance.replyAdded.subscribe(replyAdded);
      fixture.componentInstance.resolveToggled.subscribe(resolveToggled);
      fixture.componentInstance.applySuggestionClicked.subscribe(applySuggestionClicked);
      fixture.detectChanges();
      await fixture.whenStable();

      const card = el(fixture).querySelector('fg-mr-thread-card')!;
      const textarea: HTMLTextAreaElement = card.querySelector('textarea')!;
      textarea.value = 'Merci';
      textarea.dispatchEvent(new Event('input'));
      buttonByText(card as HTMLElement, 'Répondre')!.click();
      buttonByText(card as HTMLElement, 'Résoudre')!.click();
      buttonByText(card as HTMLElement, 'Appliquer la suggestion')!.click();

      expect(replyAdded).toHaveBeenCalledExactlyOnceWith({ replyToId: 't-open', body: 'Merci' });
      expect(resolveToggled).toHaveBeenCalledExactlyOnceWith({ commentId: 't-open', resolved: true });
      expect(applySuggestionClicked).toHaveBeenCalledExactlyOnceWith({ commentId: 't-open' });
    });
  });
});
