import { LOCALE_ID } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { provideRouter } from '@angular/router';
import { Select, GbtToastService, formatDateTime, formatRelativeTime } from '@masmarino/gabarit';
import { IssueDetail } from './issue-detail';
import { Issue, IssueComment } from '../issues.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { MeService } from '../../shell/me.service';
import { PageTitleService } from '../../shell/page-title.service';

const RELATIVE_OPTIONS = { style: 'short', maxUnit: 'day', absoluteAfterDays: 30 } as const;
const ABSOLUTE_OPTIONS = { day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' } as const;
const relativeTime = (iso: string) => formatRelativeTime(iso, 'fr', undefined, RELATIVE_OPTIONS);
const absoluteDateTime = (iso: string) => formatDateTime(iso, 'fr', ABSOLUTE_OPTIONS);

const ISSUE_URL = '/api/repositories/repo-1/issues/1';
const COMMENTS_URL = `${ISSUE_URL}/comments`;
const LABELS_URL = '/api/repositories/repo-1/labels';
const MILESTONES_URL = '/api/repositories/repo-1/milestones';

const ALICE = { id: 'u1', username: 'alice' };
const BOB = { id: 'u2', username: 'bob' };
const ME = { id: 'me-1', username: 'florian' };

const LABEL_BUG = { id: 'l1', name: 'Bug', color: '#dc2626', repositoryId: 'repo-1', groupId: null, createdAt: '2026-01-01T00:00:00Z' };
const MILESTONE_V1 = { id: 'm1', title: 'v1.0', description: '', dueDate: null, state: 'open', repositoryId: 'repo-1', groupId: null, createdAt: '2026-01-01T00:00:00Z' };

function issueBody(overrides: Partial<Issue> = {}): Issue {
  return {
    id: 'i1',
    number: 1,
    authorId: 'u1',
    assigneeId: null,
    title: 'Titre',
    description: 'Description',
    status: 'todo',
    kind: 'task',
    parentIssueId: null,
    createdAt: '2026-01-01T00:00:00Z',
    closedAt: null,
    milestoneId: null,
    labels: [],
    author: ALICE,
    assignee: null,
    commentCount: 0,
    ...overrides,
  };
}

function comment(overrides: Partial<IssueComment> = {}): IssueComment {
  return { id: 'c1', authorId: 'u2', author: BOB, body: 'Je confirme.', createdAt: '2026-01-02T00:00:00Z', ...overrides };
}

type Role = 'owner' | 'reader' | 'contributor' | 'maintainer';

describe('IssueDetail', () => {
  function setup(options: { role?: Role; meId?: string } = {}) {
    TestBed.configureTestingModule({
      providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([]), { provide: LOCALE_ID, useValue: 'fr' }],
    });
    if (options.role) {
      setRole(options.role);
    }
    if (options.meId) {
      TestBed.inject(MeService).id.set(options.meId);
    }
    const fixture = TestBed.createComponent(IssueDetail);
    fixture.componentRef.setInput('repositoryId', 'repo-1');
    fixture.componentRef.setInput('number', 1);
    fixture.detectChanges();
    const httpMock = TestBed.inject(HttpTestingController);
    const el = fixture.nativeElement as HTMLElement;
    return { fixture, httpMock, el };
  }

  function loaded(options: { role?: Role; meId?: string; issue?: Partial<Issue>; comments?: IssueComment[]; labels?: unknown[]; milestones?: unknown[] } = {}) {
    const ctx = setup(options);
    ctx.httpMock.expectOne(ISSUE_URL).flush(issueBody(options.issue));
    ctx.httpMock.expectOne(COMMENTS_URL).flush(options.comments ?? []);
    ctx.httpMock.expectOne(LABELS_URL).flush(options.labels ?? []);
    ctx.httpMock.expectOne(MILESTONES_URL).flush(options.milestones ?? []);
    ctx.fixture.detectChanges();
    return ctx;
  }

  /** The pickers need write access, so tests driving them set the repository context's role first. */
  function setRole(role: Role): void {
    TestBed.inject(RepositoryContextService).current.set({ repositoryId: 'repo-1', path: ['acme', 'widget'], role, ancestors: [], groupId: null });
  }

  const text = (el: Element | null | undefined) => (el?.textContent ?? '').replace(/\s+/g, ' ').trim();
  const toasts = () => TestBed.inject(GbtToastService).toasts().map((t) => [t.variant, t.message]);
  const buttonByText = (root: Element, label: string) => Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find((button) => text(button) === label);
  const headerActions = (el: HTMLElement) => el.querySelector('.gbt-page-header__actions')!;
  const panel = (el: HTMLElement, heading: string) =>
    Array.from(el.querySelectorAll<HTMLElement>('.gbt-page-layout__aside gbt-panel')).find((p) => text(p.querySelector('.gbt-panel__heading')) === heading);
  const timelineCards = (el: HTMLElement) => Array.from(el.querySelectorAll<HTMLElement>('.issue-timeline > li article'));
  const composerTextarea = (el: HTMLElement) => el.querySelector<HTMLTextAreaElement>('.issue-detail__composer textarea')!;

  function openSelectAndPick(fixture: ReturnType<typeof setup>['fixture'], root: ParentNode, optionLabel: string): void {
    const trigger = root.querySelector<HTMLButtonElement>('.gbt-select__trigger')!;
    trigger.click();
    fixture.detectChanges();
    const option = Array.from(document.querySelectorAll<HTMLElement>('.gbt-select__option')).find((el) => el.textContent?.trim() === optionLabel);
    option!.click();
    fixture.detectChanges();
  }

  function typeComment(fixture: ReturnType<typeof setup>['fixture'], el: HTMLElement, body: string): void {
    const textarea = composerTextarea(el);
    textarea.value = body;
    textarea.dispatchEvent(new Event('input'));
    fixture.detectChanges();
  }

  function clickCommenter(fixture: ReturnType<typeof setup>['fixture'], el: HTMLElement): void {
    el.querySelector<HTMLButtonElement>('.issue-detail__comment-button button')!.click();
    fixture.detectChanges();
  }

  describe('page frame', () => {
    it('lays the page out as a wide page layout with a 300px aside', () => {
      const { el } = loaded();

      const layout = el.querySelector('gbt-page-layout');
      expect(layout).toBeTruthy();
      expect(layout!.getAttribute('data-width')).toBe('wide');
      expect(layout!.getAttribute('data-aside-width')).toBe('md');
      expect(el.querySelector('.gbt-page-layout__main .issue-timeline')).toBeTruthy();
      expect(el.querySelector('.gbt-page-layout__aside .issue-detail__aside')).toBeTruthy();
    });

    it('titles the page "#<number> <title>" in the page header h1', () => {
      const { el } = loaded({ issue: { number: 1, title: 'Le bouton ne répond pas' } });

      expect(text(el.querySelector('gbt-page-header h1'))).toBe('#1 Le bouton ne répond pas');
    });

    it('shows the same title in the shell header', () => {
      loaded({ issue: { number: 1, title: 'Le bouton ne répond pas' } });
      TestBed.tick();

      expect(TestBed.inject(PageTitleService).title()).toBe('#1 Le bouton ne répond pas');
    });

    it('shows the status badge and the kind next to the title', () => {
      const { el } = loaded({ issue: { status: 'in_progress', kind: 'bug' } });

      const badges = el.querySelector('.gbt-page-header__badges')!;
      expect(text(badges.querySelector('fg-status-badge'))).toBe('En cours');
      expect(text(badges.querySelector('.issue-detail__kind'))).toBe('Bug');
    });

    it('reads "ouvert <relative date> par <author>" under the title, with the exact date on hover', () => {
      const createdAt = '2026-03-04T10:00:00Z';
      const { el } = loaded({ issue: { createdAt } });

      const meta = el.querySelector('.gbt-page-header__meta')!;
      expect(text(meta)).toContain(`ouvert ${relativeTime(createdAt)} par alice`);
      const time = meta.querySelector('time')!;
      expect(time.getAttribute('datetime')).toBe(createdAt);
      expect(time.getAttribute('title')).toBe(absoluteDateTime(createdAt));
    });

    it('counts the loaded comments in the header meta', () => {
      const { el } = loaded({ comments: [comment({ id: 'c1' }), comment({ id: 'c2' })] });

      expect(text(el.querySelector('.gbt-page-header__meta'))).toContain('· 2 commentaires');
    });

    it('drops "par …" when the author no longer resolves', () => {
      const { el } = loaded({ issue: { author: null } });

      const meta = text(el.querySelector('.gbt-page-header__meta'));
      expect(meta).toContain('ouvert');
      expect(meta).not.toContain('par');
    });

    it('adds "fermé <relative date>" to the header meta of a closed issue', () => {
      const closedAt = '2026-02-01T08:00:00Z';
      const { el } = loaded({ issue: { status: 'done', closedAt } });

      expect(text(el.querySelector('.gbt-page-header__meta'))).toContain(`fermé ${relativeTime(closedAt)}`);
      expect(text(el.querySelector('.gbt-page-header__badges fg-status-badge'))).toBe('Terminé');
    });

    it('shows a busy placeholder until the issue arrives', () => {
      const { fixture, httpMock, el } = setup();

      expect(el.querySelector('[aria-busy="true"]')).toBeTruthy();
      expect(el.querySelector('gbt-page-header')).toBeNull();

      httpMock.expectOne(ISSUE_URL).flush(issueBody());
      fixture.detectChanges();
      expect(el.querySelector('[aria-busy="true"]')).toBeNull();
      expect(el.querySelector('gbt-page-header')).toBeTruthy();
    });

    it('shows an error toast and a message when the issue cannot be loaded', () => {
      const { fixture, httpMock, el } = setup();

      httpMock.expectOne(ISSUE_URL).flush('boom', { status: 500, statusText: 'Server Error' });
      fixture.detectChanges();

      expect(toasts()).toContainEqual(['error', 'Impossible de charger ce ticket.']);
      expect(el.querySelector('[aria-busy="true"]')).toBeNull();
      const failed = el.querySelector('gbt-alert .gbt-alert');
      expect(text(failed)).toBe("Ce ticket n'a pas pu être chargé.");
      // The toast announces it, so the inline message stays silent: one live region, not two.
      expect(failed?.getAttribute('data-variant')).toBe('error');
      expect(failed?.getAttribute('role')).toBeNull();
      expect(failed?.getAttribute('aria-live')).toBeNull();
    });
  });

  describe('description card', () => {
    it('opens the rail with the description card: author, "a ouvert ce ticket", relative date and the Auteur marker', () => {
      const createdAt = '2026-03-04T10:00:00Z';
      const { el } = loaded({ issue: { createdAt } });

      const [card] = timelineCards(el);
      expect(card.classList).toContain('issue-comment--description');
      expect(text(card.querySelector('.issue-comment__author'))).toBe('alice');
      expect(text(card.querySelector('.issue-comment__verb'))).toBe('a ouvert ce ticket');
      expect(text(card.querySelector('.issue-comment__badge'))).toBe('Auteur');
      expect(card.querySelector('gbt-avatar')).toBeTruthy();
      const time = card.querySelector('time')!;
      expect(time.getAttribute('datetime')).toBe(createdAt);
      expect(time.getAttribute('title')).toBe(absoluteDateTime(createdAt));
      expect(text(time)).toBe(relativeTime(createdAt));
    });

    it('draws each card of the rail as an outlined card: author, verb and date in its header above the box, the text in the body', () => {
      const { el } = loaded({ issue: {} });

      const [card] = timelineCards(el);
      const host = card.querySelector('gbt-card')!;
      const header = host.querySelector(':scope > .gbt-card__header')!;
      const inner = host.querySelector(':scope > .gbt-card')!;
      expect(Array.from(host.children)).toEqual([header, inner]);
      expect(inner.getAttribute('data-variant')).toBe('outlined');
      expect(header.querySelector('.issue-comment__author')).toBeTruthy();
      expect(header.querySelector('time')).toBeTruthy();
      expect(inner.querySelector('.gbt-card__body .issue-comment__body')).toBeTruthy();
    });

    it('renders the description as markdown, its headings shifted under the page outline (h1 page title, h2 discussion)', () => {
      const { el } = loaded({ issue: { description: '# Contexte\n\nCliquer ne fait **rien**.\n\n```\ncargo test\n```' } });

      const body = timelineCards(el)[0].querySelector('.issue-comment__body')!;
      expect(body.querySelector('fg-markdown-view')).toBeTruthy();
      expect(body.querySelector('h1')).toBeNull();
      expect(text(body.querySelector('h3'))).toBe('Contexte');
      expect(text(body.querySelector('strong'))).toBe('rien');
      expect(text(body.querySelector('pre code'))).toBe('cargo test');
    });

    it('sanitises the rendered description (no script, no inline handler)', () => {
      const { el } = loaded({ issue: { description: 'Salut <script>window.issuePwned = true;</script> <img src="x" onerror="window.issuePwned = true">' } });

      const body = timelineCards(el)[0].querySelector('.issue-comment__body')!;
      expect(body.querySelector('script')).toBeNull();
      expect(body.querySelector('img')?.getAttribute('onerror')).toBeNull();
      expect((window as unknown as { issuePwned?: boolean }).issuePwned).toBeUndefined();
    });

    it('says so when there is no description', () => {
      const { el } = loaded({ issue: { description: '   ' } });

      const body = timelineCards(el)[0].querySelector('.issue-comment__body')!;
      expect(text(body)).toBe('Aucune description.');
      expect(body.querySelector('fg-markdown-view')).toBeNull();
    });

    it('names an author who no longer resolves "Utilisateur inconnu"', () => {
      const { el } = loaded({ issue: { author: null } });

      expect(text(timelineCards(el)[0].querySelector('.issue-comment__author'))).toBe('Utilisateur inconnu');
    });
  });

  describe('comments', () => {
    it('hangs each comment on the rail after the description, with its author, "a commenté" and its date', () => {
      const comments = [
        comment({ id: 'c1', author: BOB, body: 'Je confirme.', createdAt: '2026-01-02T00:00:00Z' }),
        comment({ id: 'c2', authorId: 'u1', author: ALICE, body: 'Merci.', createdAt: '2026-01-03T12:00:00Z' }),
      ];
      const { el } = loaded({ comments });

      const cards = timelineCards(el);
      expect(cards.length).toBe(3);
      const [, first, second] = cards;
      expect(text(first.querySelector('.issue-comment__author'))).toBe('bob');
      expect(text(first.querySelector('.issue-comment__verb'))).toBe('a commenté');
      expect(first.querySelector('time')!.getAttribute('datetime')).toBe('2026-01-02T00:00:00Z');
      expect(first.querySelector('time')!.getAttribute('title')).toBe(absoluteDateTime('2026-01-02T00:00:00Z'));
      expect(text(first.querySelector('time'))).toBe(relativeTime('2026-01-02T00:00:00Z'));
      expect(text(first.querySelector('.issue-comment__body'))).toBe('Je confirme.');
      expect(first.querySelector('gbt-avatar')).toBeTruthy();
      expect(text(second.querySelector('.issue-comment__author'))).toBe('alice');
      expect(text(second.querySelector('.issue-comment__body'))).toBe('Merci.');
    });

    it('marks the comments written by the issue author with "Auteur", not the others', () => {
      const { el } = loaded({ comments: [comment({ id: 'c1', author: BOB }), comment({ id: 'c2', authorId: 'u1', author: ALICE })] });

      const [, bobCard, aliceCard] = timelineCards(el);
      expect(bobCard.querySelector('.issue-comment__badge')).toBeNull();
      expect(text(aliceCard.querySelector('.issue-comment__badge'))).toBe('Auteur');
    });

    it('renders comment bodies as sanitised markdown', () => {
      const { el } = loaded({ comments: [comment({ body: 'Voir `main.rs` **ligne 3** <script>window.commentPwned = true;</script>' })] });

      const body = timelineCards(el)[1].querySelector('.issue-comment__body')!;
      expect(text(body.querySelector('code'))).toBe('main.rs');
      expect(text(body.querySelector('strong'))).toBe('ligne 3');
      expect(body.querySelector('script')).toBeNull();
    });

    it('shifts comment headings under the page outline too (a "# Titre" renders an h3)', () => {
      const { el } = loaded({ comments: [comment({ body: '# Titre\n\n## Sous-titre' })] });

      const body = timelineCards(el)[1].querySelector('.issue-comment__body')!;
      expect(body.querySelector('h1, h2')).toBeNull();
      expect(text(body.querySelector('h3'))).toBe('Titre');
      expect(text(body.querySelector('h4'))).toBe('Sous-titre');
    });

    it('names a comment author who no longer resolves "Utilisateur inconnu"', () => {
      const { el } = loaded({ comments: [comment({ author: null })] });

      expect(text(timelineCards(el)[1].querySelector('.issue-comment__author'))).toBe('Utilisateur inconnu');
    });
  });

  describe('composer', () => {
    it('hangs a decorative pencil marker on the rail beside the composer', () => {
      const { el } = loaded({ role: 'reader' });

      const marker = el.querySelector('.issue-detail__composer > gbt-icon-marker.issue-detail__composer-marker');
      expect(marker?.querySelector('.gbt-icon-marker')?.getAttribute('aria-hidden')).toBe('true');
      expect(marker?.querySelector('.gbt-icon-marker')?.getAttribute('data-appearance')).toBe('outline');
    });

    it('posts the comment, reloads the comments, toasts and clears the draft (the native textarea too)', async () => {
      const { fixture, httpMock, el } = loaded({ role: 'reader' });

      typeComment(fixture, el, 'Je regarde ça.');
      clickCommenter(fixture, el);

      const req = httpMock.expectOne({ method: 'POST', url: COMMENTS_URL });
      expect(req.request.body).toEqual({ body: 'Je regarde ça.' });
      req.flush(comment({ id: 'c9', body: 'Je regarde ça.' }));
      httpMock.expectOne({ method: 'GET', url: COMMENTS_URL }).flush([comment({ id: 'c9', body: 'Je regarde ça.' })]);
      fixture.detectChanges();
      // NgModel pushes the cleared draft into the field on a microtask.
      await fixture.whenStable();
      fixture.detectChanges();

      expect(toasts()).toContainEqual(['success', 'Commentaire ajouté.']);
      expect(composerTextarea(el).value).toBe('');
      expect(text(timelineCards(el)[1].querySelector('.issue-comment__body'))).toBe('Je regarde ça.');
      clickCommenter(fixture, el);
      httpMock.expectNone({ method: 'POST', url: COMMENTS_URL });
    });

    it('keeps the draft when the comment cannot be sent', () => {
      const { fixture, httpMock, el } = loaded();

      typeComment(fixture, el, 'Brouillon précieux');
      clickCommenter(fixture, el);
      httpMock.expectOne({ method: 'POST', url: COMMENTS_URL }).flush('boom', { status: 500, statusText: 'Server Error' });
      fixture.detectChanges();

      expect(toasts()).toContainEqual(['error', "Impossible d'envoyer le commentaire."]);
      expect(composerTextarea(el).value).toBe('Brouillon précieux');
      clickCommenter(fixture, el);
      expect(httpMock.expectOne({ method: 'POST', url: COMMENTS_URL }).request.body).toEqual({ body: 'Brouillon précieux' });
    });

    it('sends nothing for a blank draft', () => {
      const { fixture, httpMock, el } = loaded();

      typeComment(fixture, el, '   ');
      clickCommenter(fixture, el);

      httpMock.expectNone({ method: 'POST', url: COMMENTS_URL });
    });

    it('does not send twice while a comment is on its way', () => {
      const { fixture, httpMock, el } = loaded();

      typeComment(fixture, el, 'Une fois');
      clickCommenter(fixture, el);
      clickCommenter(fixture, el);

      expect(httpMock.match({ method: 'POST', url: COMMENTS_URL }).length).toBe(1);
    });

    it('offers the composer to readers too (the API lets any collaborator comment), with a secondary right-aligned "Commenter"', () => {
      const { el } = loaded({ role: 'reader' });

      const button = el.querySelector<HTMLElement>('.issue-detail__composer .issue-detail__comment-button')!;
      expect(button).toBeTruthy();
      expect(text(button)).toBe('Commenter');
      expect(button.querySelector('.gbt-button--secondary')).toBeTruthy();
      expect(getComputedStyle(button).alignSelf).toBe('flex-end');
    });

    it('explains a failed reload after a successful post', () => {
      const { fixture, httpMock, el } = loaded();

      typeComment(fixture, el, 'Ok');
      clickCommenter(fixture, el);
      httpMock.expectOne({ method: 'POST', url: COMMENTS_URL }).flush(comment());
      httpMock.expectOne({ method: 'GET', url: COMMENTS_URL }).flush('boom', { status: 500, statusText: 'Server Error' });

      expect(toasts()).toContainEqual(['error', 'Commentaire envoyé, mais impossible de recharger la liste.']);
    });
  });

  describe('close and reopen', () => {
    it('closes the issue from a secondary header button, then offers to reopen it', () => {
      const { fixture, httpMock, el } = loaded({ role: 'contributor' });

      const close = buttonByText(headerActions(el), 'Fermer le ticket')!;
      expect(close).toBeTruthy();
      expect(close.classList).toContain('gbt-button--secondary');
      close.click();
      fixture.detectChanges();

      const req = httpMock.expectOne(`${ISSUE_URL}/close`);
      expect(req.request.method).toBe('POST');
      req.flush(issueBody({ status: 'done', closedAt: '2026-01-05T00:00:00Z' }));
      fixture.detectChanges();

      expect(toasts()).toContainEqual(['success', 'Ticket fermé.']);
      expect(text(el.querySelector('.gbt-page-header__badges fg-status-badge'))).toBe('Terminé');
      expect(buttonByText(headerActions(el), 'Fermer le ticket')).toBeUndefined();

      buttonByText(headerActions(el), 'Rouvrir le ticket')!.click();
      fixture.detectChanges();
      const reopen = httpMock.expectOne(`${ISSUE_URL}/reopen`);
      expect(reopen.request.method).toBe('POST');
      reopen.flush(issueBody({ status: 'todo', closedAt: null }));
      fixture.detectChanges();

      expect(toasts()).toContainEqual(['success', 'Ticket rouvert.']);
      expect(text(el.querySelector('.gbt-page-header__badges fg-status-badge'))).toBe('À faire');
      expect(buttonByText(headerActions(el), 'Fermer le ticket')).toBeTruthy();
    });

    it('keeps the same header button across close and reopen, so keyboard focus is not dropped', () => {
      const { fixture, httpMock, el } = loaded({ role: 'contributor' });

      const button = buttonByText(headerActions(el), 'Fermer le ticket')!;
      button.focus();
      expect(document.activeElement).toBe(button);
      button.click();
      httpMock.expectOne(`${ISSUE_URL}/close`).flush(issueBody({ status: 'done', closedAt: '2026-01-05T00:00:00Z' }));
      fixture.detectChanges();

      expect(buttonByText(headerActions(el), 'Rouvrir le ticket')).toBe(button);
      expect(button.isConnected).toBe(true);
      expect(document.activeElement).toBe(button);

      button.click();
      httpMock.expectOne(`${ISSUE_URL}/reopen`).flush(issueBody({ status: 'todo', closedAt: null }));
      fixture.detectChanges();

      expect(buttonByText(headerActions(el), 'Fermer le ticket')).toBe(button);
      expect(document.activeElement).toBe(button);
    });

    it('keeps the issue as it was and toasts when closing fails', () => {
      const { fixture, httpMock, el } = loaded({ role: 'owner' });

      buttonByText(headerActions(el), 'Fermer le ticket')!.click();
      httpMock.expectOne(`${ISSUE_URL}/close`).flush('boom', { status: 500, statusText: 'Server Error' });
      fixture.detectChanges();

      expect(toasts()).toContainEqual(['error', 'Impossible de fermer le ticket.']);
      expect(text(el.querySelector('.gbt-page-header__badges fg-status-badge'))).toBe('À faire');
    });

    it('toasts when reopening fails', () => {
      const { fixture, httpMock, el } = loaded({ role: 'maintainer', issue: { status: 'done', closedAt: '2026-01-05T00:00:00Z' } });

      buttonByText(headerActions(el), 'Rouvrir le ticket')!.click();
      httpMock.expectOne(`${ISSUE_URL}/reopen`).flush('boom', { status: 500, statusText: 'Server Error' });
      fixture.detectChanges();

      expect(toasts()).toContainEqual(['error', 'Impossible de rouvrir le ticket.']);
    });

    it('has no primary button (at most one per view, and none is needed here)', () => {
      const { el } = loaded({ role: 'owner' });

      expect(el.querySelectorAll('.gbt-button--primary').length).toBe(0);
    });
  });

  describe('aside panels', () => {
    it('lists Statut, Assigné, Labels, Milestone, Dates and Participants as level-2 panels', () => {
      const { el } = loaded({ role: 'contributor' });

      const headings = Array.from(el.querySelectorAll('.gbt-page-layout__aside .gbt-panel__heading'));
      expect(headings.map((h) => text(h))).toEqual(['Statut', 'Assigné', 'Labels', 'Milestone', 'Dates', 'Participants']);
      expect(headings.every((h) => h.tagName === 'H2')).toBe(true);
    });

    it('shows the status badge in the Statut panel', () => {
      const { el } = loaded({ issue: { status: 'in_review' } });

      expect(text(panel(el, 'Statut')!.querySelector('fg-status-badge'))).toBe('En revue');
    });

    it('shows the assignee as a user chip, or "Personne"', () => {
      const { el } = loaded({ issue: { assignee: BOB, assigneeId: BOB.id } });
      expect(text(panel(el, 'Assigné')!.querySelector('gbt-user-chip .gbt-user-chip__name'))).toBe('bob');

      TestBed.resetTestingModule();
      const unassigned = loaded();
      const assigned = panel(unassigned.el, 'Assigné')!;
      expect(assigned.querySelector('gbt-user-chip')).toBeNull();
      expect(text(assigned.querySelector('.issue-detail__empty'))).toBe('Personne');
    });

    it('assigns the issue to the current user from the Assigné panel', () => {
      const { fixture, httpMock, el } = loaded({ role: 'contributor', meId: ME.id });

      buttonByText(panel(el, 'Assigné')!, "M'assigner")!.click();
      fixture.detectChanges();

      const req = httpMock.expectOne(`${ISSUE_URL}/assign`);
      expect(req.request.method).toBe('POST');
      expect(req.request.body).toEqual({ assigneeId: ME.id });
      req.flush(issueBody({ assigneeId: ME.id, assignee: ME }));
      fixture.detectChanges();

      expect(toasts()).toContainEqual(['success', 'Ticket assigné.']);
      expect(text(panel(el, 'Assigné')!.querySelector('gbt-user-chip .gbt-user-chip__name'))).toBe('florian');
      expect(buttonByText(panel(el, 'Assigné')!, "M'assigner")).toBeUndefined();
    });

    it('toasts when the assignment fails', () => {
      const { fixture, httpMock, el } = loaded({ role: 'contributor', meId: ME.id });

      buttonByText(panel(el, 'Assigné')!, "M'assigner")!.click();
      httpMock.expectOne(`${ISSUE_URL}/assign`).flush('boom', { status: 500, statusText: 'Server Error' });
      fixture.detectChanges();

      expect(toasts()).toContainEqual(['error', 'Impossible de vous assigner le ticket. Réessayez plus tard.']);
      expect(panel(el, 'Assigné')!.querySelector('gbt-user-chip')).toBeNull();
    });

    it('still offers "M\'assigner" when someone else is assigned', () => {
      const { el } = loaded({ role: 'contributor', meId: ME.id, issue: { assignee: BOB, assigneeId: BOB.id } });

      expect(buttonByText(panel(el, 'Assigné')!, "M'assigner")).toBeTruthy();
    });

    it('shows the dates: created, and closed once closed', () => {
      const createdAt = '2026-01-01T00:00:00Z';
      const closedAt = '2026-02-01T08:00:00Z';
      const { el } = loaded({ issue: { createdAt } });

      const rows = (root: HTMLElement) => Array.from(panel(root, 'Dates')!.querySelectorAll('dt')).map((term) => [text(term), text(term.nextElementSibling)]);
      expect(rows(el)).toEqual([['Créé', relativeTime(createdAt)]]);
      const time = panel(el, 'Dates')!.querySelector('time')!;
      expect(time.getAttribute('datetime')).toBe(createdAt);
      expect(time.getAttribute('title')).toBe(absoluteDateTime(createdAt));

      TestBed.resetTestingModule();
      const closed = loaded({ issue: { createdAt, status: 'done', closedAt } });
      expect(rows(closed.el)).toEqual([
        ['Créé', relativeTime(createdAt)],
        ['Fermé', relativeTime(closedAt)],
      ]);
      const closedTime = panel(closed.el, 'Dates')!.querySelectorAll('time')[1];
      expect(closedTime.getAttribute('datetime')).toBe(closedAt);
      expect(closedTime.getAttribute('title')).toBe(absoluteDateTime(closedAt));
    });

    it('lists the participants once each: author, assignee, then commenters', () => {
      const carol = { id: 'u3', username: 'carol' };
      const { el } = loaded({
        issue: { assignee: carol, assigneeId: carol.id },
        comments: [comment({ id: 'c1', author: BOB }), comment({ id: 'c2', author: ALICE }), comment({ id: 'c3', author: BOB }), comment({ id: 'c4', author: null })],
      });

      const names = Array.from(panel(el, 'Participants')!.querySelectorAll('li gbt-user-chip .gbt-user-chip__name')).map((chip) => text(chip));
      expect(names).toEqual(['alice', 'carol', 'bob']);
    });

    it('hides the Participants panel when nobody resolves', () => {
      const { el } = loaded({ issue: { author: null } });

      expect(panel(el, 'Participants')).toBeUndefined();
    });
  });

  describe('labels and milestone', () => {
    it('assigns a label via the label picker, sending a PUT and updating local state on success', () => {
      const { fixture, httpMock, el } = loaded({ role: 'contributor', labels: [LABEL_BUG] });

      openSelectAndPick(fixture, panel(el, 'Labels')!, 'Bug');

      const req = httpMock.expectOne(`${ISSUE_URL}/labels`);
      expect(req.request.method).toBe('PUT');
      expect(req.request.body).toEqual({ labelIds: ['l1'] });
      req.flush([LABEL_BUG]);
      fixture.detectChanges();

      expect(panel(el, 'Labels')!.querySelector('.gbt-select__chips')?.textContent).toContain('Bug');
      expect(toasts()).toContainEqual(['success', 'Labels mis à jour.']);
    });

    it('toasts when the labels cannot be updated', () => {
      const { fixture, httpMock, el } = loaded({ role: 'contributor', labels: [LABEL_BUG] });

      openSelectAndPick(fixture, panel(el, 'Labels')!, 'Bug');
      httpMock.expectOne(`${ISSUE_URL}/labels`).flush('boom', { status: 500, statusText: 'Server Error' });

      expect(toasts()).toContainEqual(['error', 'Impossible de mettre à jour les labels.']);
    });

    it('assigns a milestone via the milestone picker, sending a PATCH with milestoneId', () => {
      const { fixture, httpMock, el } = loaded({ role: 'contributor', milestones: [MILESTONE_V1] });

      openSelectAndPick(fixture, panel(el, 'Milestone')!, 'v1.0');

      const req = httpMock.expectOne(ISSUE_URL);
      expect(req.request.method).toBe('PATCH');
      expect(req.request.body).toEqual({ title: 'Titre', description: 'Description', kind: 'task', milestoneId: 'm1' });
      req.flush(issueBody({ milestoneId: 'm1' }));
      fixture.detectChanges();

      expect(panel(el, 'Milestone')!.querySelector('.gbt-select__trigger')!.textContent).toContain('v1.0');
      expect(toasts()).toContainEqual(['success', 'Milestone mis à jour.']);
    });

    it('toasts when the milestone cannot be updated', () => {
      const { fixture, httpMock, el } = loaded({ role: 'contributor', milestones: [MILESTONE_V1] });

      openSelectAndPick(fixture, panel(el, 'Milestone')!, 'v1.0');
      httpMock.expectOne(ISSUE_URL).flush('boom', { status: 500, statusText: 'Server Error' });

      expect(toasts()).toContainEqual(['error', 'Impossible de mettre à jour le milestone.']);
    });

    it('does not rewrite the label picker value on every change detection pass (a fresh array each time made NgModel re-apply it forever and froze the page)', async () => {
      const writeValue = vi.spyOn(Select.prototype, 'writeValue');
      const { fixture } = loaded({ role: 'contributor', labels: [LABEL_BUG], issue: { labels: [LABEL_BUG] } });
      await fixture.whenStable();
      writeValue.mockClear();

      fixture.detectChanges();
      fixture.detectChanges();
      await fixture.whenStable();

      expect(writeValue).not.toHaveBeenCalled();
      writeValue.mockRestore();
    });
  });

  describe('read-only (reader)', () => {
    it('hides the label and milestone pickers for a reader (no write access), same as the Merge Request detail page', () => {
      const { el } = loaded({ role: 'reader', labels: [LABEL_BUG], milestones: [MILESTONE_V1] });

      expect(el.querySelectorAll('.gbt-select__trigger').length).toBe(0);
      expect(el.textContent).not.toContain('Aucun milestone');
      expect(el.textContent).toContain('Titre');
    });

    it('shows a reader the labels as tags and the milestone as text, or "Aucun"', () => {
      const { el } = loaded({ role: 'reader', labels: [LABEL_BUG], milestones: [MILESTONE_V1], issue: { labels: [LABEL_BUG], milestoneId: 'm1' } });

      expect(text(panel(el, 'Labels')!.querySelector('gbt-tag'))).toBe('Bug');
      expect(text(panel(el, 'Milestone'))).toContain('v1.0');

      TestBed.resetTestingModule();
      const bare = loaded({ role: 'reader', labels: [LABEL_BUG], milestones: [MILESTONE_V1] });
      expect(text(panel(bare.el, 'Labels')!.querySelector('.issue-detail__empty'))).toBe('Aucun');
      expect(text(panel(bare.el, 'Milestone')!.querySelector('.issue-detail__empty'))).toBe('Aucun');
    });

    it('withholds close/reopen and assignment from a reader', () => {
      const { el } = loaded({ role: 'reader', meId: ME.id });

      expect(buttonByText(headerActions(el), 'Fermer le ticket')).toBeUndefined();
      expect(buttonByText(el, "M'assigner")).toBeUndefined();
      expect(el.querySelector('.gbt-page-header__actions button')).toBeNull();
    });

    it('treats an unknown role (context not loaded yet) as read-only', () => {
      const { el } = loaded();

      expect(el.querySelector('.gbt-page-header__actions button')).toBeNull();
      expect(el.querySelectorAll('.gbt-select__trigger').length).toBe(0);
    });
  });
});
