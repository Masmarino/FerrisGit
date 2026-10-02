import { LOCALE_ID } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { MrThreadCard } from './mr-thread-card';
import { Comment, TimelineThread } from '../merge-requests.service';
import { ALICE, commentFixture } from '../merge-request-fixtures';

function makeComment(overrides: Partial<Comment> = {}): Comment {
  return commentFixture({
    id: 'root',
    author: ALICE,
    body: 'Pourquoi ce changement ?',
    createdAt: '2026-09-23T10:00:00',
    filePath: 'src/lib.rs',
    lineNumber: 12,
    side: 'new',
    ...overrides,
  });
}

function makeThread(overrides: Partial<TimelineThread> = {}): TimelineThread {
  return {
    type: 'thread',
    id: 'root',
    createdAt: '2026-09-23T10:00:00',
    filePath: 'src/lib.rs',
    lineNumber: 12,
    endLine: null,
    side: 'new',
    outdated: false,
    resolved: false,
    resolvedBy: null,
    resolvedAt: null,
    excerpt: [
      { line: 11, kind: 'context', content: 'let a = 1;' },
      { line: 12, kind: 'added', content: 'let b = 2;' },
      { line: null, kind: 'removed', content: 'let c = 3;' },
    ],
    root: makeComment(),
    replies: [],
    ...overrides,
  };
}

describe('MrThreadCard', () => {
  beforeEach(() => {
    TestBed.configureTestingModule({ providers: [{ provide: LOCALE_ID, useValue: 'fr' }] });
  });

  function setup(thread: TimelineThread, canWrite = false) {
    const fixture = TestBed.createComponent(MrThreadCard);
    fixture.componentRef.setInput('thread', thread);
    fixture.componentRef.setInput('canWrite', canWrite);
    fixture.detectChanges();
    return fixture;
  }

  const el = (fixture: { nativeElement: HTMLElement }) => fixture.nativeElement;
  const buttonByText = (root: HTMLElement, text: string) => Array.from(root.querySelectorAll('button')).find((b) => b.textContent?.trim() === text) as HTMLButtonElement | undefined;

  it('shows file:line for a single-line thread and file:line-endLine for a range', () => {
    const single = setup(makeThread());
    expect(el(single).querySelector('.mr-thread__location')?.textContent?.trim()).toBe('src/lib.rs:12');

    const range = setup(makeThread({ endLine: 15 }));
    expect(el(range).querySelector('.mr-thread__location')?.textContent?.trim()).toBe('src/lib.rs:12-15');
  });

  it('renders excerpt lines with a +/-/space prefix per kind', () => {
    const fixture = setup(makeThread());

    const lines = Array.from(el(fixture).querySelectorAll('.mr-thread__excerpt-line')).map((l) => l.textContent);
    expect(lines).toEqual([' let a = 1;', '+let b = 2;', '-let c = 3;']);
  });

  it('shows the line numbers in the gutter and tints added / removed rows', () => {
    const fixture = setup(
      makeThread({
        excerpt: [
          { line: 11, kind: 'context', content: 'let a = 1;' },
          { line: 12, kind: 'added', content: 'let b = 2;' },
          { line: 13, kind: 'removed', content: 'let c = 3;' },
        ],
      }),
    );

    const rows = Array.from(el(fixture).querySelectorAll('tr.mr-thread__excerpt-row')) as HTMLElement[];
    expect(rows.map((r) => r.querySelector('td.mr-thread__excerpt-num')?.textContent?.trim())).toEqual(['11', '12', '13']);
    expect(rows.map((r) => r.classList.contains('mr-thread__excerpt-row--added'))).toEqual([false, true, false]);
    expect(rows.map((r) => r.classList.contains('mr-thread__excerpt-row--removed'))).toEqual([false, false, true]);
    expect(rows[0].classList.contains('mr-thread__excerpt-row--context')).toBe(true);
  });

  it('shows the Ouverte badge for an unresolved thread and Résolue for a resolved one', () => {
    const open = setup(makeThread());
    expect(el(open).querySelector('.mr-thread__badge--open')?.textContent?.trim()).toBe('Ouverte');
    expect(el(open).textContent).not.toContain('Résolue');

    const resolved = setup(makeThread({ resolved: true, root: makeComment({ resolved: true }) }));
    buttonByText(el(resolved), 'Afficher')!.click();
    resolved.detectChanges();
    expect(el(resolved).querySelector('.mr-thread__badge--resolved')?.textContent?.trim()).toBe('Résolue');
    expect(el(resolved).querySelector('.mr-thread__badge--open')).toBeNull();
  });

  it('shows the Périmée badge when the thread is outdated', () => {
    const fixture = setup(makeThread({ outdated: true }));

    expect(el(fixture).querySelector('.mr-thread__badge--outdated')?.textContent?.trim()).toBe('Périmée');
    expect(el(setup(makeThread())).querySelector('.mr-thread__badge--outdated')).toBeNull();
  });

  it('renders the root and reply bodies with their author names', () => {
    const fixture = setup(
      makeThread({
        replies: [makeComment({ id: 'r1', replyToId: 'root', author: { id: 'u2', username: 'bob' }, body: 'Pour corriger un bug' }), makeComment({ id: 'r2', replyToId: 'root', author: null, body: 'Merci' })],
      }),
    );

    const text = el(fixture).textContent as string;
    expect(text).toContain('alice');
    expect(text).toContain('Pourquoi ce changement ?');
    expect(text).toContain('bob');
    expect(text).toContain('Pour corriger un bug');
    expect(text).toContain('Utilisateur supprimé');
    expect(text).toContain('Merci');
  });

  it('renders each date as a relative <time> carrying the exact date-time on hover', () => {
    const fixture = setup(makeThread());

    const header = el(fixture).querySelector('.mr-thread__date')!;
    expect(header.getAttribute('datetime')).toBe('2026-09-23T10:00:00');
    expect(header.getAttribute('title')).toBe('23/09/2026 10:00');
    const comment = el(fixture).querySelector('.mr-thread__comment-date')!;
    expect(comment.tagName).toBe('TIME');
    expect(comment.getAttribute('title')).toBe('23/09/2026 10:00');
  });

  it('hangs the root author avatar on the rail while open, and a check marker once resolved', () => {
    const open = setup(makeThread());
    expect(el(open).querySelector('.mr-thread__avatar .gbt-avatar__initials')?.getAttribute('aria-label')).toBe('alice');
    expect(el(open).querySelector('.mr-thread__marker')).toBeNull();

    const resolved = setup(makeThread({ resolved: true, root: makeComment({ resolved: true }) }));
    const marker = el(resolved).querySelector('gbt-icon-marker.mr-thread__marker');
    expect(marker).not.toBeNull();
    const disc = marker!.querySelector('.gbt-icon-marker')!;
    expect(disc.getAttribute('data-tone')).toBe('success');
    expect(disc.getAttribute('data-appearance')).toBe('outline');
    expect(disc.getAttribute('aria-hidden')).toBe('true');
    expect(el(resolved).querySelector('.mr-thread__avatar')).toBeNull();
  });

  it('draws an open discussion as an outlined card under its header, flagged by a warning "Ouverte" badge; a resolved one by a success "Résolue" badge', () => {
    const open = setup(makeThread());
    const openCard = el(open).querySelector('gbt-card')!;
    const header = openCard.querySelector(':scope > .gbt-card__header')!;
    const box = openCard.querySelector(':scope > .gbt-card')!;
    expect(Array.from(openCard.children)).toEqual([header, box]);
    expect(box.getAttribute('data-variant')).toBe('outlined');
    expect(header.querySelector('.mr-thread__location')?.textContent?.trim()).toBe('src/lib.rs:12');
    const openBadge = header.querySelector('.mr-thread__badge--open .gbt-badge');
    expect(openBadge?.textContent?.trim()).toBe('Ouverte');
    expect(openBadge?.getAttribute('data-variant')).toBe('warning');

    const resolved = setup(makeThread({ resolved: true, root: makeComment({ resolved: true }) }));
    buttonByText(el(resolved), 'Afficher')!.click();
    resolved.detectChanges();
    const resolvedHeader = el(resolved).querySelector('gbt-card > .gbt-card__header')!;
    expect(resolvedHeader.querySelector('.mr-thread__badge--open')).toBeNull();
    expect(resolvedHeader.querySelector('.mr-thread__badge--resolved .gbt-badge')?.getAttribute('data-variant')).toBe('success');
  });

  describe('resolved thread', () => {
    const resolvedThread = (resolvedBy: { id: string; username: string } | null) => makeThread({ resolved: true, resolvedBy, root: makeComment({ resolved: true }) });

    const summary = (fixture: ReturnType<typeof setup>) => (el(fixture).querySelector('.mr-thread-collapsed__text')?.textContent ?? '').replace(/\s+/g, ' ').trim();

    it('starts collapsed to a "Discussion résolue par bob · file:line" bar with an Afficher button and no body', () => {
      const fixture = setup(resolvedThread({ id: 'u2', username: 'bob' }));

      expect(summary(fixture)).toBe('Discussion résolue par bob · src/lib.rs:12');
      const show = buttonByText(el(fixture), 'Afficher');
      expect(show?.getAttribute('aria-label')).toBe('Afficher la discussion sur src/lib.rs:12');
      expect(el(fixture).textContent).not.toContain('Pourquoi ce changement ?');
    });

    it('omits "par …" when the resolver is unknown', () => {
      const fixture = setup(resolvedThread(null));

      expect(summary(fixture)).toBe('Discussion résolue · src/lib.rs:12');
    });

    it('reveals the body once Afficher is clicked, and collapses again from Masquer', () => {
      const fixture = setup(resolvedThread({ id: 'u2', username: 'bob' }));

      buttonByText(el(fixture), 'Afficher')!.click();
      fixture.detectChanges();

      expect(el(fixture).textContent).toContain('Pourquoi ce changement ?');
      expect(el(fixture).querySelector('.mr-thread-collapsed')).toBeNull();

      buttonByText(el(fixture), 'Masquer')!.click();
      fixture.detectChanges();

      expect(el(fixture).textContent).not.toContain('Pourquoi ce changement ?');
      expect(buttonByText(el(fixture), 'Afficher')).toBeDefined();
    });

    it('emits resolved:false from Rouvrir once expanded', () => {
      const fixture = setup(resolvedThread(null), true);
      let emitted: { commentId: string; resolved: boolean } | undefined;
      fixture.componentInstance.resolveToggled.subscribe((e) => (emitted = e));

      buttonByText(el(fixture), 'Afficher')!.click();
      fixture.detectChanges();
      buttonByText(el(fixture), 'Rouvrir')!.click();

      expect(emitted).toEqual({ commentId: 'root', resolved: false });
    });
  });

  describe('suggestion', () => {
    it('renders the suggestion and emits applySuggestionClicked from Appliquer la suggestion', () => {
      const fixture = setup(makeThread({ root: makeComment({ suggestedContent: 'let b = 3;\n' }) }), true);
      let emitted: { commentId: string } | undefined;
      fixture.componentInstance.applySuggestionClicked.subscribe((e) => (emitted = e));

      expect(el(fixture).querySelector('pre.mr-thread__suggestion')?.textContent).toBe('let b = 3;\n');
      buttonByText(el(fixture), 'Appliquer la suggestion')!.click();

      expect(emitted).toEqual({ commentId: 'root' });
    });

    it('shows "Suggestion appliquée dans <short sha>", and no apply button, once applied', () => {
      const fixture = setup(makeThread({ root: makeComment({ suggestedContent: 'x', appliedAt: '2026-09-23T11:00:00', appliedCommitSha: 'a1b2c3d4e5f60718293a4b5c6d7e8f9012345678' }) }), true);

      expect(el(fixture).querySelector('.mr-thread__applied')?.textContent?.replace(/\s+/g, ' ').trim()).toBe('Suggestion appliquée dans a1b2c3d');
      expect(el(fixture).textContent).not.toContain('a1b2c3d4');
      expect(buttonByText(el(fixture), 'Appliquer la suggestion')).toBeUndefined();
    });

    it('hides the apply button for read-only viewers', () => {
      const fixture = setup(makeThread({ root: makeComment({ suggestedContent: 'x' }) }), false);

      expect(el(fixture).querySelector('pre.mr-thread__suggestion')).not.toBeNull();
      expect(buttonByText(el(fixture), 'Appliquer la suggestion')).toBeUndefined();
    });
  });

  describe('write actions', () => {
    async function typeReply(fixture: ReturnType<typeof setup>, value: string) {
      await fixture.whenStable();
      const textarea: HTMLTextAreaElement = el(fixture).querySelector('.mr-thread__reply textarea')!;
      textarea.value = value;
      textarea.dispatchEvent(new Event('input'));
      fixture.detectChanges();
    }

    it('emits replyAdded with the trimmed body and clears the draft', async () => {
      const fixture = setup(makeThread(), true);
      let emitted: { replyToId: string; body: string } | undefined;
      fixture.componentInstance.replyAdded.subscribe((e) => (emitted = e));

      await typeReply(fixture, '  D’accord  ');
      buttonByText(el(fixture), 'Répondre')!.click();
      fixture.detectChanges();
      // NgModel pushes the cleared draft into the field on a microtask.
      await fixture.whenStable();
      fixture.detectChanges();

      expect(emitted).toEqual({ replyToId: 'root', body: 'D’accord' });
      expect(fixture.componentInstance['replyBody']()).toBe('');
      expect((el(fixture).querySelector('.mr-thread__reply textarea') as HTMLTextAreaElement).value).toBe('');
    });

    it('emits nothing for an empty reply', async () => {
      const fixture = setup(makeThread(), true);
      let emitted = false;
      fixture.componentInstance.replyAdded.subscribe(() => (emitted = true));

      await typeReply(fixture, '   ');
      buttonByText(el(fixture), 'Répondre')!.click();

      expect(emitted).toBe(false);
    });

    it('emits resolveToggled with resolved:true from Résoudre', () => {
      const fixture = setup(makeThread(), true);
      let emitted: { commentId: string; resolved: boolean } | undefined;
      fixture.componentInstance.resolveToggled.subscribe((e) => (emitted = e));

      buttonByText(el(fixture), 'Résoudre')!.click();

      expect(emitted).toEqual({ commentId: 'root', resolved: true });
    });

    it('keeps a "Répondre" label on the reply field for screen readers', async () => {
      const fixture = setup(makeThread(), true);
      await fixture.whenStable();

      const textarea = el(fixture).querySelector('.mr-thread__reply textarea') as HTMLTextAreaElement;
      const label = el(fixture).querySelector('.mr-thread__reply label') as HTMLLabelElement;
      expect(textarea.id).toBe('mr-reply-root');
      expect(label.getAttribute('for')).toBe('mr-reply-root');
      expect(label.textContent?.trim()).toBe('Répondre');
      expect(textarea.labels?.[0]).toBe(label);
      expect(label.classList).toContain('gbt-textarea__label--hidden');
    });

    it('renders no reply box and no resolve button without write access', () => {
      const fixture = setup(makeThread(), false);

      expect(el(fixture).querySelector('textarea')).toBeNull();
      expect(buttonByText(el(fixture), 'Répondre')).toBeUndefined();
      expect(buttonByText(el(fixture), 'Résoudre')).toBeUndefined();
    });
  });
});
