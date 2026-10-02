import { LOCALE_ID } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController } from '@angular/common/http/testing';
import { provideHttpClientTesting } from '@angular/common/http/testing';
import { provideRouter } from '@angular/router';
import { MergeRequestDetail } from './merge-request-detail';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { MergeRequestTimeline } from '../merge-request-timeline/merge-request-timeline';
import { MrApprovalsPanel } from '../mr-approvals-panel/mr-approvals-panel';
import { FileDiffView } from '../file-diff-view/file-diff-view';
import { Select, GbtToastService } from '@masmarino/gabarit';

const alice = { id: 'u1', username: 'alice' };
const bob = { id: 'u2', username: 'bob' };
const carol = { id: 'u3', username: 'carol' };
const dave = { id: 'u4', username: 'dave' };

function mergeRequestBody(overrides: Record<string, unknown> = {}) {
  return {
    id: 'mr-1',
    title: 'Fix bug',
    description: '',
    status: 'open',
    sourceBranch: 'feature',
    targetBranch: 'main',
    mergeCommitSha: null,
    createdAt: '2026-01-01T00:00:00Z',
    closedAt: null,
    milestoneId: null,
    labels: [],
    author: null,
    commentCount: 0,
    ...overrides,
  };
}

const noReviews = { reviews: [], liveApprovalCount: 0, requiredApprovals: 0, blocked: false };
const emptyTimeline = { author: null, items: [] };
const bugLabel = { id: 'l1', name: 'Bug', color: '#dc2626', repositoryId: 'repo-1', groupId: null, createdAt: '2026-01-01T00:00:00Z' };
const v1 = { id: 'm1', title: 'v1.0', description: '', dueDate: null, state: 'open', repositoryId: 'repo-1', groupId: null, createdAt: '2026-01-01T00:00:00Z' };
const oneFileDiff = [{ path: 'src/main.rs', change: 'modified', hunks: [{ rows: [{ oldLine: 1, oldContent: 'a\n', newLine: 1, newContent: 'b\n', kind: 'modified' }] }] }];

describe('MergeRequestDetail', () => {
  function setup(role: 'owner' | 'reader' | 'contributor' | 'maintainer' = 'owner') {
    TestBed.configureTestingModule({
      providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([]), { provide: LOCALE_ID, useValue: 'fr' }],
    });
    const fixture = TestBed.createComponent(MergeRequestDetail);
    fixture.componentRef.setInput('repositoryId', 'repo-1');
    fixture.componentRef.setInput('mergeRequestId', 'mr-1');
    TestBed.inject(RepositoryContextService).current.set({ repositoryId: 'repo-1', path: ['acme', 'widget'], role, ancestors: [], groupId: null });
    fixture.detectChanges();
    const httpMock = TestBed.inject(HttpTestingController);
    return { fixture, httpMock };
  }

  type Fixture = ReturnType<typeof setup>['fixture'];

  // Answers the initial requests on load. `repo` also flushes the label and milestone lists (specs that don't care leave them unanswered).
  function flushInitial(
    httpMock: HttpTestingController,
    options: { mr?: Record<string, unknown>; diff?: unknown[]; comments?: unknown[]; reviews?: unknown; timeline?: unknown; labels?: unknown[]; milestones?: unknown[] } = {},
  ) {
    httpMock.expectOne('/api/merge-requests/mr-1').flush(mergeRequestBody(options.mr));
    httpMock.expectOne('/api/merge-requests/mr-1/diff').flush(options.diff ?? []);
    httpMock.expectOne('/api/merge-requests/mr-1/comments').flush(options.comments ?? []);
    httpMock.expectOne('/api/merge-requests/mr-1/reviews').flush(options.reviews ?? noReviews);
    httpMock.expectOne('/api/merge-requests/mr-1/timeline').flush(options.timeline ?? emptyTimeline);
    httpMock.expectOne('/api/repositories/repo-1/labels').flush(options.labels ?? []);
    httpMock.expectOne('/api/repositories/repo-1/milestones').flush(options.milestones ?? []);
  }

  function loaded(role: 'owner' | 'reader' | 'contributor' | 'maintainer' = 'owner', options: Parameters<typeof flushInitial>[1] = {}) {
    const ctx = setup(role);
    flushInitial(ctx.httpMock, options);
    ctx.fixture.detectChanges();
    return ctx;
  }

  const el = (fixture: Fixture): HTMLElement => fixture.nativeElement;
  const text = (node: Element | null | undefined) => (node?.textContent ?? '').replace(/\s+/g, ' ').trim();
  const toasts = () => TestBed.inject(GbtToastService).toasts().map((t) => [t.variant, t.message]);
  const timelineComponent = (fixture: Fixture) => fixture.debugElement.query(By.directive(MergeRequestTimeline)).componentInstance as MergeRequestTimeline;
  const approvalsPanel = (fixture: Fixture) => fixture.debugElement.query(By.directive(MrApprovalsPanel)).componentInstance as MrApprovalsPanel;
  const meta = (fixture: Fixture) => text(el(fixture).querySelector('.mr-detail__meta'));
  const main = (fixture: Fixture) => el(fixture).querySelector('.gbt-page-layout__main')!;
  const aside = (fixture: Fixture) => el(fixture).querySelector('.gbt-page-layout__aside')!;
  const asidePanel = (fixture: Fixture, heading: string) =>
    Array.from(aside(fixture).querySelectorAll('gbt-panel')).find((panel) => text(panel.querySelector('.gbt-panel__heading')) === heading) ?? null;
  const headerButton = (fixture: Fixture, label: string) =>
    Array.from<HTMLButtonElement>(el(fixture).querySelectorAll('.gbt-page-header__actions .mr-detail__actions button')).find((b) => b.textContent?.trim() === label);
  const clickTab = (fixture: Fixture, label: string) => {
    Array.from<HTMLButtonElement>(el(fixture).querySelectorAll('[role="tab"]'))
      .find((tab) => tab.textContent?.trim() === label)!
      .click();
    fixture.detectChanges();
  };

  describe('page frame', () => {
    it('shows a busy placeholder while the merge request loads, then a wide page layout with a 300px aside', () => {
      const { fixture, httpMock } = setup();

      const busy = el(fixture).querySelector('[aria-busy="true"]');
      expect(busy).toBeTruthy();
      expect(text(busy?.querySelector('[role="status"]'))).toBe('Chargement de la demande de fusion…');
      expect(el(fixture).querySelector('gbt-page-layout')).toBeNull();

      flushInitial(httpMock);
      fixture.detectChanges();

      const layout = el(fixture).querySelector('gbt-page-layout')!;
      expect(layout.getAttribute('data-width')).toBe('wide');
      expect(layout.getAttribute('data-aside-width')).toBe('md');
      expect(el(fixture).querySelector('[aria-busy="true"]')).toBeNull();
    });

    it('shows the error toast and a message when the merge request cannot be loaded', () => {
      const { fixture, httpMock } = setup();
      httpMock.expectOne('/api/merge-requests/mr-1').flush('boom', { status: 500, statusText: 'Server Error' });
      fixture.detectChanges();

      expect(toasts()).toContainEqual(['error', 'Impossible de charger cette demande de fusion.']);
      const failed = el(fixture).querySelector('gbt-alert .gbt-alert');
      expect(text(failed)).toBe("Cette demande de fusion n'a pas pu être chargée.");
      // The toast announces it, so the inline message stays silent (one live region, not two).
      expect(failed?.getAttribute('data-variant')).toBe('error');
      expect(failed?.getAttribute('role')).toBeNull();
      expect(failed?.getAttribute('aria-live')).toBeNull();
      expect(el(fixture).querySelector('[aria-busy="true"]')).toBeNull();
      expect(el(fixture).querySelector('[role="tab"]')).toBeNull();
    });

    it('has exactly two tabs, "Vue d\'ensemble" and "Modifications" (no "Revues" nor "Discussion")', () => {
      const { fixture } = loaded();

      const tabs = Array.from<HTMLElement>(el(fixture).querySelectorAll('[role="tab"]')).map((tab) => tab.textContent?.trim());
      expect(tabs).toEqual(["Vue d'ensemble", 'Modifications']);
    });

    it('puts the tabs in the main column', () => {
      const { fixture } = loaded();

      expect(main(fixture).querySelector('gbt-tabs')).toBeTruthy();
      expect(aside(fixture).querySelector('gbt-tabs')).toBeNull();
    });
  });

  describe('header', () => {
    it('shows the title as the h1 and the status as a badge', () => {
      const { fixture } = loaded();

      const h1s = el(fixture).querySelectorAll('h1');
      expect(h1s.length).toBe(1);
      expect(text(h1s[0])).toBe('Fix bug');
      expect(text(el(fixture).querySelector('.gbt-page-header fg-status-badge'))).toBe('Ouverte');
    });

    it('shows the branches and "ouverte <created date> par <author>" in the meta line', () => {
      const { fixture } = loaded('owner', { mr: { createdAt: '2026-03-04T10:00:00Z' }, timeline: { author: alice, items: [] } });

      expect(meta(fixture)).toContain('feature → main');
      expect(meta(fixture)).toContain('ouverte 04/03/2026 par alice');
      const time = el(fixture).querySelector<HTMLTimeElement>('.mr-detail__meta time')!;
      expect(time.getAttribute('datetime')).toBe('2026-03-04T10:00:00Z');
      expect(time.title).toMatch(/2026/);
    });

    it('takes the author from the merge request itself, without waiting for the timeline', () => {
      const { fixture } = loaded('owner', { mr: { author: bob } });

      expect(meta(fixture)).toContain('par bob');
    });

    it('shows no "par ..." segment before the timeline has arrived', () => {
      const { fixture } = loaded();

      expect(meta(fixture)).not.toContain('par');
      expect(meta(fixture)).not.toContain('inconnu');
      expect(meta(fixture)).toContain('feature → main');
      expect(meta(fixture)).toContain('ouverte 01/01/2026');
    });

    it('adds when the merge request was merged, or closed', () => {
      const merged = loaded('owner', { mr: { status: 'merged', closedAt: '2026-02-02T10:00:00Z' } });
      expect(meta(merged.fixture)).toContain('· fusionnée 02/02/2026');
      expect(text(el(merged.fixture).querySelector('.gbt-page-header fg-status-badge'))).toBe('Fusionnée');
      TestBed.resetTestingModule();

      const closed = loaded('owner', { mr: { status: 'closed', closedAt: '2026-02-03T10:00:00Z' } });
      expect(meta(closed.fixture)).toContain('· fermée 03/02/2026');
      expect(text(el(closed.fixture).querySelector('.gbt-page-header fg-status-badge'))).toBe('Fermée');
    });

    // Counted from GET /comments, which the page reloads after each comment it posts: the detail's own `commentCount` would go stale.
    it('counts the comments (none, one, several) and follows new ones', () => {
      const comment = (id: string) => ({ id, authorId: 'u1', author: alice, body: 'x', createdAt: '2026-01-02T00:00:00Z', replyToId: null, filePath: null });
      const none = loaded('owner', { mr: { commentCount: 0 } });
      expect(meta(none.fixture)).not.toContain('commentaire');
      TestBed.resetTestingModule();

      const one = loaded('owner', { mr: { commentCount: 1 }, comments: [comment('c1')] });
      expect(meta(one.fixture)).toContain('· 1 commentaire');
      expect(meta(one.fixture)).not.toContain('commentaires');

      timelineComponent(one.fixture).commentAdded.emit('Another');
      one.httpMock.expectOne((r) => r.method === 'POST' && r.url === '/api/merge-requests/mr-1/comments').flush({});
      one.httpMock.expectOne((r) => r.method === 'GET' && r.url === '/api/merge-requests/mr-1/comments').flush([comment('c1'), comment('c2'), comment('c3')]);
      one.httpMock.expectOne('/api/merge-requests/mr-1/timeline').flush(emptyTimeline);
      one.fixture.detectChanges();
      expect(meta(one.fixture)).toContain('· 3 commentaires');
    });

    it('offers Fusionner (the only primary button) and Fermer in the header for a writer on an open merge request', () => {
      const { fixture } = loaded();

      expect(headerButton(fixture, 'Fusionner')).toBeTruthy();
      expect(headerButton(fixture, 'Fermer')).toBeTruthy();
      const primaries = el(fixture).querySelectorAll('.gbt-button--primary');
      expect(primaries.length).toBe(1);
      expect(primaries[0].textContent?.trim()).toBe('Fusionner');
    });

    it('offers Fusionner to a maintainer like the owner, since the server merges only for them', () => {
      const { fixture } = loaded('maintainer');

      expect(headerButton(fixture, 'Fusionner')).toBeTruthy();
      expect(headerButton(fixture, 'Fermer')).toBeTruthy();
    });

    it('offers a contributor Fermer and the review actions but no Fusionner, which the server would refuse', () => {
      const { fixture } = loaded('contributor');

      expect(headerButton(fixture, 'Fusionner')).toBeFalsy();
      expect(headerButton(fixture, 'Fermer')).toBeTruthy();
      expect(el(fixture).querySelector('.gbt-button--primary')).toBeNull();
      expect(approvalsPanel(fixture).canWrite()).toBe(true);
    });

    it('keeps the blocked Merge button disabled while approvals are missing', () => {
      const { fixture } = loaded('owner', { reviews: { reviews: [], liveApprovalCount: 0, requiredApprovals: 1, blocked: true } });

      const merge = Array.from<HTMLButtonElement>(el(fixture).querySelectorAll('.mr-detail__actions button')).find((button) => button.textContent?.trim() === 'Fusionner');
      expect(merge!.disabled).toBe(true);
    });
  });

  it('shows the error toast but still renders the tabs, labels and actions when the timeline request fails', () => {
    const { fixture, httpMock } = setup();
    httpMock.expectOne('/api/merge-requests/mr-1').flush(mergeRequestBody());
    httpMock.expectOne('/api/merge-requests/mr-1/diff').flush([]);
    httpMock.expectOne('/api/merge-requests/mr-1/comments').flush([]);
    httpMock.expectOne('/api/merge-requests/mr-1/reviews').flush(noReviews);
    httpMock.expectOne('/api/merge-requests/mr-1/timeline').flush('boom', { status: 500, statusText: 'Server Error' });
    httpMock.expectOne('/api/repositories/repo-1/labels').flush([]);
    httpMock.expectOne('/api/repositories/repo-1/milestones').flush([]);
    fixture.detectChanges();

    expect(toasts()).toContainEqual(['error', 'Impossible de charger cette demande de fusion.']);
    const tabs = Array.from<HTMLElement>(el(fixture).querySelectorAll('[role="tab"]')).map((tab) => tab.textContent?.trim());
    expect(tabs).toEqual(["Vue d'ensemble", 'Modifications']);
    expect(el(fixture).querySelector('gbt-select')).toBeTruthy();
    expect(el(fixture).querySelector('.mr-detail__actions')).toBeTruthy();
    expect(meta(fixture)).not.toContain('par');
    expect(meta(fixture)).not.toContain('inconnu');
  });

  it('renders the timeline overview, fed with the description and the timeline items', () => {
    const items = [{ type: 'comment', id: 'c1', createdAt: '2026-01-02T00:00:00Z', author: alice, body: 'Looks good' }];
    const { fixture } = loaded('owner', { mr: { description: 'Fixes the crash' }, timeline: { author: alice, items } });

    const timeline = timelineComponent(fixture);
    expect(timeline.mergeRequest().description).toBe('Fixes the crash');
    expect(timeline.author()).toEqual(alice);
    expect(timeline.items()).toEqual(items);
    expect(timeline.canWrite()).toBe(true);
    expect(el(fixture).querySelector('fg-merge-request-timeline')!.textContent).toContain('Looks good');
  });

  it('keeps the timeline composer\'s "Commenter" button right-aligned, not stretched to the full width', () => {
    const { fixture } = loaded('owner', { mr: { status: 'closed' } });

    const button: HTMLElement = el(fixture).querySelector('fg-merge-request-timeline .comment-button')!;
    expect(button).toBeTruthy();
    expect(button.textContent?.trim()).toBe('Commenter');
    expect(getComputedStyle(button).alignSelf).toBe('flex-end');
  });

  describe('aside', () => {
    it('stacks the Approbations, Labels, Milestone, Auteur and Participants panels, all h2', () => {
      const { fixture } = loaded('owner', { mr: { author: alice } });

      const headings = Array.from(aside(fixture).querySelectorAll('.gbt-panel__heading'));
      expect(headings.map((h) => text(h))).toEqual(['Approbations', 'Labels', 'Milestone', 'Auteur', 'Participants']);
      expect(headings.every((h) => h.tagName === 'H2')).toBe(true);
    });

    it('shows the approvals panel in the aside, not above the tabs, for an open merge request', () => {
      const reviews = { reviews: [], liveApprovalCount: 1, requiredApprovals: 2, blocked: true };
      const { fixture } = loaded('owner', { reviews });

      const panel = asidePanel(fixture, 'Approbations')!.querySelector('fg-mr-approvals-panel');
      expect(panel).toBeTruthy();
      expect(main(fixture).querySelector('fg-mr-approvals-panel')).toBeNull();
      expect(approvalsPanel(fixture).summary()).toEqual(reviews);
      expect(approvalsPanel(fixture).status()).toBe('open');
      expect(approvalsPanel(fixture).canWrite()).toBe(true);
      expect(panel!.textContent).toContain('1/2 approbations requises.');
    });

    it('does not show the approvals panel once the merge request is merged', () => {
      const { fixture } = loaded('owner', { mr: { status: 'merged' } });

      expect(el(fixture).querySelector('.mr-approvals')).toBeNull();
      expect(asidePanel(fixture, 'Approbations')).toBeNull();
      expect(el(fixture).querySelector('.mr-detail__actions')).toBeNull();
    });

    it('does not show the approvals panel once the merge request is closed', () => {
      const { fixture } = loaded('owner', { mr: { status: 'closed' } });

      expect(asidePanel(fixture, 'Approbations')).toBeNull();
      expect(el(fixture).querySelector('.mr-detail__actions')).toBeNull();
    });

    it('shows the author as a user chip', () => {
      const { fixture } = loaded('owner', { mr: { author: alice } });

      expect(text(asidePanel(fixture, 'Auteur')!.querySelector('gbt-user-chip .gbt-user-chip__name'))).toBe('alice');
    });

    it('falls back to the timeline author, then to "Utilisateur supprimé"', () => {
      const fromTimeline = loaded('owner', { timeline: { author: carol, items: [] } });
      expect(text(asidePanel(fromTimeline.fixture, 'Auteur')!.querySelector('.gbt-user-chip__name'))).toBe('carol');
      TestBed.resetTestingModule();

      const unknown = loaded();
      const panel = asidePanel(unknown.fixture, 'Auteur')!;
      expect(panel.querySelector('gbt-user-chip')).toBeNull();
      expect(text(panel.querySelector('.gbt-panel__body'))).toBe('Utilisateur supprimé');
    });

    it('lists the participants once each: the author, then commenters and reviewers in order of appearance', () => {
      const threadComment = (id: string, author: typeof alice | null) => ({
        id,
        authorId: author?.id ?? 'gone',
        author,
        body: 'x',
        createdAt: '2026-01-03T00:00:00Z',
        replyToId: id === 't1' ? null : 't1',
        filePath: 'src/main.rs',
        lineNumber: 1,
        endLine: null,
        side: 'new',
        outdated: false,
        resolved: false,
        suggestedContent: null,
        appliedAt: null,
        appliedCommitSha: null,
      });
      const thread = {
        type: 'thread',
        id: 't1',
        createdAt: '2026-01-03T00:00:00Z',
        filePath: 'src/main.rs',
        lineNumber: 1,
        endLine: null,
        side: 'new',
        outdated: false,
        resolved: false,
        resolvedBy: null,
        resolvedAt: null,
        excerpt: [],
        root: threadComment('t1', carol),
        replies: [threadComment('r1', alice), threadComment('r2', null)],
      };
      const timeline = {
        author: alice,
        items: [
          { type: 'comment', id: 'c1', createdAt: '2026-01-02T00:00:00Z', author: bob, body: 'Hi' },
          { type: 'event', id: 'e0', createdAt: '2026-01-02T01:00:00Z', actor: dave, kind: 'labels_changed', payload: { added: [], removed: [] } },
          thread,
          { type: 'comment', id: 'c2', createdAt: '2026-01-04T00:00:00Z', author: null, body: 'Gone' },
          { type: 'event', id: 'e1', createdAt: '2026-01-05T00:00:00Z', actor: bob, kind: 'review_submitted', payload: { decision: 'approved' } },
        ],
      };
      const reviews = { ...noReviews, reviews: [{ userId: 'u4', username: 'dave', decision: 'changes_requested', stale: false, createdAt: '2026-01-06T00:00:00Z' }] };
      const { fixture } = loaded('owner', { mr: { author: alice }, timeline, reviews });

      const names = Array.from(asidePanel(fixture, 'Participants')!.querySelectorAll('li .gbt-user-chip__name')).map((n) => text(n));
      expect(names).toEqual(['alice', 'bob', 'carol', 'dave']);
    });

    it('hides the Participants panel when nobody resolves', () => {
      const { fixture } = loaded();

      expect(asidePanel(fixture, 'Participants')).toBeNull();
    });
  });

  describe('labels and milestone', () => {
    it('does not rewrite the label picker value on every change detection pass (a fresh array each time made NgModel re-apply it forever and froze the page)', async () => {
      const writeValue = vi.spyOn(Select.prototype, 'writeValue');
      const { fixture, httpMock } = setup('owner');
      flushInitial(httpMock, { mr: { labels: [{ id: 'l1', name: 'bug', color: '#ff0000' }] }, labels: [{ id: 'l1', name: 'bug', color: '#ff0000' }] });
      fixture.detectChanges();
      await fixture.whenStable();

      const settled = writeValue.mock.calls.length;
      for (let i = 0; i < 5; i++) {
        fixture.detectChanges();
        await fixture.whenStable();
      }

      expect(writeValue.mock.calls.length).toBe(settled);
      writeValue.mockRestore();
    });

    function openSelectAndPick(fixture: Fixture, panelHeading: string, optionLabel: string): void {
      const trigger = asidePanel(fixture, panelHeading)!.querySelector<HTMLButtonElement>('.gbt-select__trigger')!;
      trigger.click();
      fixture.detectChanges();
      const option = Array.from<HTMLButtonElement>(el(fixture).querySelectorAll('.gbt-select__option')).find((node) => node.textContent?.trim() === optionLabel);
      option!.click();
      fixture.detectChanges();
    }

    it('assigns a label via the label picker in the aside, sending a PUT, updating local state and reloading the timeline', () => {
      const { fixture, httpMock } = loaded('owner', { labels: [bugLabel] });

      openSelectAndPick(fixture, 'Labels', 'Bug');

      const req = httpMock.expectOne('/api/merge-requests/mr-1/labels');
      expect(req.request.method).toBe('PUT');
      expect(req.request.body).toEqual({ labelIds: ['l1'] });
      req.flush([bugLabel]);
      fixture.detectChanges();

      httpMock.expectOne('/api/merge-requests/mr-1/timeline').flush(emptyTimeline);
      expect(asidePanel(fixture, 'Labels')!.querySelector('.gbt-select__chips')?.textContent).toContain('Bug');
      expect(toasts()).toContainEqual(['success', 'Labels mis à jour.']);
    });

    it('shows an error toast when the labels cannot be saved', () => {
      const { fixture, httpMock } = loaded('owner', { labels: [bugLabel] });

      openSelectAndPick(fixture, 'Labels', 'Bug');
      httpMock.expectOne('/api/merge-requests/mr-1/labels').flush('boom', { status: 500, statusText: 'Server Error' });

      expect(toasts()).toContainEqual(['error', 'Impossible de mettre à jour les labels.']);
      httpMock.expectNone('/api/merge-requests/mr-1/timeline');
    });

    it('assigns a milestone via the milestone picker in the aside, sending a PATCH with milestoneId and reloading the timeline', () => {
      const { fixture, httpMock } = loaded('owner', { mr: { description: 'desc' }, milestones: [v1] });

      openSelectAndPick(fixture, 'Milestone', 'v1.0');

      const req = httpMock.expectOne('/api/merge-requests/mr-1');
      expect(req.request.method).toBe('PATCH');
      expect(req.request.body).toEqual({ title: 'Fix bug', description: 'desc', milestoneId: 'm1' });
      req.flush(mergeRequestBody({ description: 'desc', milestoneId: 'm1' }));
      fixture.detectChanges();

      httpMock.expectOne('/api/merge-requests/mr-1/timeline').flush(emptyTimeline);
      expect(asidePanel(fixture, 'Milestone')!.querySelector('.gbt-select__trigger')!.textContent).toContain('v1.0');
      expect(toasts()).toContainEqual(['success', 'Milestone mis à jour.']);
    });

    it('shows an error toast when the milestone cannot be saved', () => {
      const { fixture, httpMock } = loaded('owner', { milestones: [v1] });

      openSelectAndPick(fixture, 'Milestone', 'v1.0');
      httpMock.expectOne('/api/merge-requests/mr-1').flush('boom', { status: 500, statusText: 'Server Error' });

      expect(toasts()).toContainEqual(['error', 'Impossible de mettre à jour le milestone.']);
    });

    it('hides the label and milestone pickers for a reader (no write access), same as the Merge/Close actions', () => {
      const { fixture } = loaded('reader');

      expect(el(fixture).querySelectorAll('.gbt-select__trigger').length).toBe(0);
      expect(el(fixture).textContent).not.toContain('Aucun milestone');
      expect(el(fixture).querySelector('.mr-detail__actions')).toBeNull();
      expect(approvalsPanel(fixture).canWrite()).toBe(false);
      expect(timelineComponent(fixture).canWrite()).toBe(false);
    });

    it('shows a reader the labels as tags and the milestone as text, or "Aucun"', () => {
      const tagged = loaded('reader', { mr: { labels: [bugLabel], milestoneId: 'm1' }, labels: [bugLabel], milestones: [v1] });
      expect(Array.from(asidePanel(tagged.fixture, 'Labels')!.querySelectorAll('li gbt-tag')).map((tag) => text(tag))).toEqual(['Bug']);
      expect(text(asidePanel(tagged.fixture, 'Milestone')!.querySelector('.gbt-panel__body'))).toBe('v1.0');
      TestBed.resetTestingModule();

      const bare = loaded('reader');
      expect(text(asidePanel(bare.fixture, 'Labels')!.querySelector('.gbt-panel__body'))).toBe('Aucun');
      expect(text(asidePanel(bare.fixture, 'Milestone')!.querySelector('.gbt-panel__body'))).toBe('Aucun');
    });
  });

  describe('Modifications tab', () => {
    it('gives the diff the whole frame: the aside is hidden while the tab is active, and comes back with the overview', () => {
      const { fixture } = loaded('owner', { diff: oneFileDiff });
      expect(aside(fixture).querySelector('fg-mr-approvals-panel')).toBeTruthy();

      clickTab(fixture, 'Modifications');

      expect(aside(fixture).children.length).toBe(0);
      expect(el(fixture).querySelector('fg-mr-approvals-panel')).toBeNull();
      expect(el(fixture).querySelector('gbt-page-layout')!.getAttribute('data-width')).toBe('wide');
      expect(headerButton(fixture, 'Fusionner')).toBeTruthy();

      clickTab(fixture, "Vue d'ensemble");

      expect(asidePanel(fixture, 'Approbations')!.querySelector('fg-mr-approvals-panel')).toBeTruthy();
    });

    it('keeps the Modifications tab rendering a file diff per file, with the comments (and their author) from GET /comments', () => {
      const comment = {
        id: 'c1',
        authorId: 'u1',
        author: alice,
        body: 'Why here?',
        createdAt: '2026-01-02T00:00:00Z',
        replyToId: null,
        filePath: 'src/main.rs',
        lineNumber: 1,
        endLine: null,
        side: 'new',
        outdated: false,
        resolved: false,
        suggestedContent: null,
        appliedAt: null,
        appliedCommitSha: null,
      };
      const general = { ...comment, id: 'c2', filePath: null, lineNumber: null, side: null };
      const { fixture } = loaded('owner', { diff: oneFileDiff, comments: [comment, general] });

      const views = fixture.debugElement.queryAll(By.directive(FileDiffView));
      expect(views.length).toBe(1);
      const view = views[0].componentInstance as FileDiffView;
      expect(view.file().path).toBe('src/main.rs');
      expect(view.comments()).toEqual([comment]);
      expect(view.comments()[0].author).toEqual(alice);
    });

    it('heads each file with its path and its change in French, and counts the files', () => {
      const { fixture } = loaded('owner', {
        diff: [
          oneFileDiff[0],
          { path: 'README.md', change: 'added', hunks: [] },
          { path: 'old.txt', change: 'deleted', hunks: [] },
          { path: 'logo.png', change: 'binary', hunks: [] },
        ],
      });

      const files = Array.from(el(fixture).querySelectorAll('.mr-diff__file'));
      expect(files.map((file) => text(file.querySelector('.mr-diff__path')))).toEqual(['src/main.rs', 'README.md', 'old.txt', 'logo.png']);
      expect(files.map((file) => text(file.querySelector('.mr-diff__change')))).toEqual(['Modifié', 'Ajouté', 'Supprimé', 'Binaire']);
      expect(text(files[3])).toContain('Fichier binaire modifié.');
      expect(files[3].querySelector('fg-file-diff-view')).toBeNull();
      expect(text(el(fixture).querySelector('.mr-diff__summary'))).toBe('4 fichiers modifiés');
    });

    it('says "1 fichier modifié" in the singular, and "Aucune modification." when the diff is empty', () => {
      const one = loaded('owner', { diff: oneFileDiff });
      expect(text(el(one.fixture).querySelector('.mr-diff__summary'))).toBe('1 fichier modifié');
      TestBed.resetTestingModule();

      const none = loaded();
      expect(el(none.fixture).querySelector('.mr-diff__file')).toBeNull();
      expect(text(el(none.fixture).querySelector('.mr-diff__empty'))).toBe('Aucune modification.');
    });

    it('posts an inline comment from the diff tab, then reloads comments and the timeline', () => {
      const { fixture, httpMock } = loaded('owner', { diff: oneFileDiff });

      (fixture.debugElement.query(By.directive(FileDiffView)).componentInstance as FileDiffView).commentAdded.emit({ filePath: 'src/main.rs', lineNumber: 1, side: 'new', body: 'Inline' });

      const post = httpMock.expectOne('/api/merge-requests/mr-1/comments');
      expect(post.request.method).toBe('POST');
      post.flush({});
      httpMock.expectOne((r) => r.method === 'GET' && r.url === '/api/merge-requests/mr-1/comments').flush([]);
      httpMock.expectOne('/api/merge-requests/mr-1/timeline').flush(emptyTimeline);
    });
  });

  it('keeps a typed timeline draft and the loaded diff across a tab switch (no second /diff request)', async () => {
    const { fixture, httpMock } = loaded('owner', { diff: oneFileDiff });
    fixture.detectChanges();
    await fixture.whenStable();
    const textarea: HTMLTextAreaElement = el(fixture).querySelector('.mr-timeline__composer textarea')!;
    textarea.value = 'Work in progress';
    textarea.dispatchEvent(new Event('input'));
    fixture.detectChanges();

    clickTab(fixture, 'Modifications');
    expect(el(fixture).querySelectorAll('.mr-diff__file').length).toBe(1);
    clickTab(fixture, "Vue d'ensemble");

    expect(el(fixture).querySelector<HTMLTextAreaElement>('.mr-timeline__composer textarea')).toBe(textarea);
    expect(textarea.value).toBe('Work in progress');
    httpMock.expectNone('/api/merge-requests/mr-1/diff');
    Array.from<HTMLButtonElement>(el(fixture).querySelectorAll('.mr-timeline__composer button'))
      .find((b) => b.textContent?.trim() === 'Commenter')!
      .click();
    expect(httpMock.expectOne((r) => r.method === 'POST' && r.url === '/api/merge-requests/mr-1/comments').request.body).toEqual({ body: 'Work in progress' });
  });

  describe('review bar on the Modifications tab', () => {
    const bar = (fixture: Fixture) => el(fixture).querySelector('.mr-diff gbt-alert[data-review-bar]');
    const barButton = (fixture: Fixture, label: string) => Array.from<HTMLButtonElement>(bar(fixture)?.querySelectorAll('button') ?? []).find((b) => b.textContent?.trim() === label);

    it('sits at the top of the diff for an open merge request, with the status and both review actions (secondary) for a writer', () => {
      const { fixture } = loaded('owner', { diff: oneFileDiff, reviews: { reviews: [], liveApprovalCount: 1, requiredApprovals: 2, blocked: true } });
      clickTab(fixture, 'Modifications');

      const reviewBar = bar(fixture)!;
      expect(reviewBar).toBeTruthy();
      expect(reviewBar.compareDocumentPosition(el(fixture).querySelector('.mr-diff__file')!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
      expect(text(reviewBar.querySelector('[data-review-status]'))).toBe('1/2 approbations requises.');
      expect(barButton(fixture, 'Approuver')).toBeTruthy();
      expect(barButton(fixture, 'Demander des changements')).toBeTruthy();
      expect(reviewBar.querySelector('.gbt-button--primary')).toBeNull();
      // A status line, not a live region: the toast of a review is what announces a change.
      const box = reviewBar.querySelector('.gbt-alert')!;
      expect(box.getAttribute('data-variant')).toBe('neutral');
      expect(box.getAttribute('role')).toBeNull();
      expect(box.getAttribute('aria-live')).toBeNull();
      expect(el(fixture).querySelectorAll('.gbt-button--primary').length).toBe(1);
    });

    it('says when changes were requested', () => {
      const reviews = { reviews: [{ userId: 'u2', username: 'bob', decision: 'changes_requested', stale: false, createdAt: '2026-01-02T00:00:00Z' }], liveApprovalCount: 0, requiredApprovals: 1, blocked: true };
      const { fixture } = loaded('owner', { reviews });
      clickTab(fixture, 'Modifications');

      expect(text(bar(fixture)!.querySelector('[data-review-status]'))).toBe('Des changements ont été demandés.');
      expect(bar(fixture)!.querySelector('.gbt-alert')?.getAttribute('data-variant')).toBe('neutral');
    });

    it('shows a reader the status but no review buttons', () => {
      const { fixture } = loaded('reader', { reviews: { reviews: [], liveApprovalCount: 0, requiredApprovals: 1, blocked: true } });
      clickTab(fixture, 'Modifications');

      expect(text(bar(fixture)!.querySelector('[data-review-status]'))).toBe('0/1 approbations requises.');
      expect(bar(fixture)!.querySelector('button')).toBeNull();
    });

    it('is absent once the merge request is merged or closed', () => {
      const merged = loaded('owner', { mr: { status: 'merged' }, diff: oneFileDiff });
      clickTab(merged.fixture, 'Modifications');
      expect(el(merged.fixture).querySelector('[data-review-bar]')).toBeNull();
      TestBed.resetTestingModule();

      const closed = loaded('owner', { mr: { status: 'closed' }, diff: oneFileDiff });
      clickTab(closed.fixture, 'Modifications');
      expect(el(closed.fixture).querySelector('[data-review-bar]')).toBeNull();
    });

    it('approves from the diff tab: POST /reviews, then the same reloads and toast as from the aside', () => {
      const { fixture, httpMock } = loaded('owner', { diff: oneFileDiff });
      clickTab(fixture, 'Modifications');

      barButton(fixture, 'Approuver')!.click();

      const post = httpMock.expectOne((r) => r.method === 'POST' && r.url === '/api/merge-requests/mr-1/reviews');
      expect(post.request.body).toEqual({ decision: 'approved' });
      post.flush({});
      httpMock
        .expectOne((r) => r.method === 'GET' && r.url === '/api/merge-requests/mr-1/reviews')
        .flush({ reviews: [{ userId: 'u9', username: 'me', decision: 'approved', stale: false, createdAt: '2026-01-02T00:00:00Z' }], liveApprovalCount: 1, requiredApprovals: 1, blocked: false });
      httpMock.expectOne('/api/merge-requests/mr-1/timeline').flush(emptyTimeline);
      fixture.detectChanges();

      expect(toasts()).toContainEqual(['success', 'Revue envoyée : approuvée.']);
      expect(text(bar(fixture)!.querySelector('[data-review-status]'))).toBe('1 approbation');
    });

    it('requests changes from the diff tab', () => {
      const { fixture, httpMock } = loaded('owner', { diff: oneFileDiff });
      clickTab(fixture, 'Modifications');

      barButton(fixture, 'Demander des changements')!.click();

      expect(httpMock.expectOne((r) => r.method === 'POST' && r.url === '/api/merge-requests/mr-1/reviews').request.body).toEqual({ decision: 'changes_requested' });
    });
  });

  describe('mutations reload the timeline', () => {
    const setupLoaded = (role: 'owner' | 'reader' | 'contributor' | 'maintainer' = 'owner') => loaded(role);

    it('posts a general comment from the timeline, then reloads comments and the timeline', () => {
      const { fixture, httpMock } = setupLoaded();

      timelineComponent(fixture).commentAdded.emit('Nice work');

      const post = httpMock.expectOne('/api/merge-requests/mr-1/comments');
      expect(post.request.method).toBe('POST');
      expect(post.request.body).toEqual({ body: 'Nice work' });
      post.flush({});

      httpMock.expectOne((r) => r.method === 'GET' && r.url === '/api/merge-requests/mr-1/comments').flush([]);
      httpMock.expectOne('/api/merge-requests/mr-1/timeline').flush(emptyTimeline);
    });

    describe('the general comment draft', () => {
      async function typeDraft(fixture: Fixture, value: string) {
        fixture.detectChanges();
        await fixture.whenStable();
        const textarea: HTMLTextAreaElement = el(fixture).querySelector('.mr-timeline__composer textarea')!;
        textarea.value = value;
        textarea.dispatchEvent(new Event('input'));
        fixture.detectChanges();
        return textarea;
      }

      const clickComment = (fixture: Fixture) =>
        Array.from<HTMLButtonElement>(el(fixture).querySelectorAll('.mr-timeline__composer button'))
          .find((b) => b.textContent?.trim() === 'Commenter')!
          .click();

      it('is cleared once the POST succeeded', async () => {
        const { fixture, httpMock } = setupLoaded();
        const textarea = await typeDraft(fixture, 'Nice work');

        clickComment(fixture);
        const post = httpMock.expectOne('/api/merge-requests/mr-1/comments');
        expect(post.request.body).toEqual({ body: 'Nice work' });
        expect(textarea.value).toBe('Nice work');
        post.flush({});
        httpMock.expectOne((r) => r.method === 'GET' && r.url === '/api/merge-requests/mr-1/comments').flush([]);
        httpMock.expectOne('/api/merge-requests/mr-1/timeline').flush(emptyTimeline);
        fixture.detectChanges();
        // NgModel pushes the cleared draft into the field on a microtask.
        await fixture.whenStable();
        fixture.detectChanges();

        expect(textarea.value).toBe('');
        clickComment(fixture);
        httpMock.expectNone('/api/merge-requests/mr-1/comments');
      });

      it('is kept when the POST fails, so the user can retry', async () => {
        const { fixture, httpMock } = setupLoaded();
        const textarea = await typeDraft(fixture, 'Nice work');

        clickComment(fixture);
        httpMock.expectOne('/api/merge-requests/mr-1/comments').flush('boom', { status: 500, statusText: 'Server Error' });
        fixture.detectChanges();

        expect(toasts()).toContainEqual(['error', 'Impossible d’ajouter le commentaire.']);
        expect(textarea.value).toBe('Nice work');
        clickComment(fixture);
        expect(httpMock.expectOne('/api/merge-requests/mr-1/comments').request.body).toEqual({ body: 'Nice work' });
      });
    });

    it('reloads comments and the timeline after a reply', () => {
      const { fixture, httpMock } = setupLoaded();

      timelineComponent(fixture).replyAdded.emit({ replyToId: 'c1', body: 'Thanks' });

      const post = httpMock.expectOne('/api/merge-requests/mr-1/comments');
      expect(post.request.method).toBe('POST');
      expect(post.request.body).toEqual({ body: 'Thanks', replyToId: 'c1' });
      post.flush({});

      httpMock.expectOne((r) => r.method === 'GET' && r.url === '/api/merge-requests/mr-1/comments').flush([]);
      httpMock.expectOne('/api/merge-requests/mr-1/timeline').flush(emptyTimeline);
    });

    it('resolves a thread, then reloads comments and the timeline', () => {
      const { fixture, httpMock } = setupLoaded();

      timelineComponent(fixture).resolveToggled.emit({ commentId: 'c1', resolved: true });

      const post = httpMock.expectOne('/api/merge-requests/mr-1/comments/c1/resolve');
      expect(post.request.method).toBe('POST');
      post.flush(null);

      httpMock.expectOne((r) => r.method === 'GET' && r.url === '/api/merge-requests/mr-1/comments').flush([]);
      httpMock.expectOne('/api/merge-requests/mr-1/timeline').flush(emptyTimeline);
    });

    it('reopens a thread, then reloads comments and the timeline', () => {
      const { fixture, httpMock } = setupLoaded();

      timelineComponent(fixture).resolveToggled.emit({ commentId: 'c1', resolved: false });

      httpMock.expectOne('/api/merge-requests/mr-1/comments/c1/unresolve').flush(null);

      httpMock.expectOne((r) => r.method === 'GET' && r.url === '/api/merge-requests/mr-1/comments').flush([]);
      httpMock.expectOne('/api/merge-requests/mr-1/timeline').flush(emptyTimeline);
    });

    it('applies a suggestion, then reloads comments and the timeline', () => {
      const { fixture, httpMock } = setupLoaded();

      timelineComponent(fixture).applySuggestionClicked.emit({ commentId: 'c1' });

      const post = httpMock.expectOne('/api/merge-requests/mr-1/comments/c1/apply-suggestion');
      expect(post.request.method).toBe('POST');
      post.flush({});

      httpMock.expectOne((r) => r.method === 'GET' && r.url === '/api/merge-requests/mr-1/comments').flush([]);
      httpMock.expectOne('/api/merge-requests/mr-1/timeline').flush(emptyTimeline);
    });

    it('submits an approval, then reloads the reviews and the timeline', () => {
      const { fixture, httpMock } = setupLoaded();

      approvalsPanel(fixture).approve.emit();

      const post = httpMock.expectOne('/api/merge-requests/mr-1/reviews');
      expect(post.request.method).toBe('POST');
      expect(post.request.body).toEqual({ decision: 'approved' });
      post.flush({});

      httpMock.expectOne((r) => r.method === 'GET' && r.url === '/api/merge-requests/mr-1/reviews').flush(noReviews);
      httpMock.expectOne('/api/merge-requests/mr-1/timeline').flush(emptyTimeline);
      expect(toasts()).toContainEqual(['success', 'Revue envoyée : approuvée.']);
    });

    it('approves from the button in the aside', () => {
      const { fixture, httpMock } = setupLoaded();

      Array.from<HTMLButtonElement>(asidePanel(fixture, 'Approbations')!.querySelectorAll('button'))
        .find((b) => b.textContent?.trim() === 'Approuver')!
        .click();

      expect(httpMock.expectOne('/api/merge-requests/mr-1/reviews').request.body).toEqual({ decision: 'approved' });
    });

    it('requests changes, then reloads the reviews and the timeline', () => {
      const { fixture, httpMock } = setupLoaded();

      approvalsPanel(fixture).requestChanges.emit();

      const post = httpMock.expectOne('/api/merge-requests/mr-1/reviews');
      expect(post.request.body).toEqual({ decision: 'changes_requested' });
      post.flush({});

      httpMock.expectOne((r) => r.method === 'GET' && r.url === '/api/merge-requests/mr-1/reviews').flush(noReviews);
      httpMock.expectOne('/api/merge-requests/mr-1/timeline').flush(emptyTimeline);
      expect(toasts()).toContainEqual(['success', 'Revue envoyée : changements demandés.']);
    });

    it('merges, updates the header from the result and reloads the timeline', () => {
      const { fixture, httpMock } = setupLoaded();

      Array.from<HTMLButtonElement>(el(fixture).querySelectorAll('.mr-detail__actions button')).find((b) => b.textContent?.trim() === 'Fusionner')!.click();

      const post = httpMock.expectOne('/api/merge-requests/mr-1/merge');
      expect(post.request.method).toBe('POST');
      post.flush({ outcome: 'merged', ...mergeRequestBody({ status: 'merged', mergeCommitSha: 'abc' }) });
      fixture.detectChanges();

      httpMock.expectOne('/api/merge-requests/mr-1/timeline').flush(emptyTimeline);
      fixture.detectChanges();
      expect(el(fixture).querySelector('.gbt-page-header fg-status-badge')!.textContent).toContain('Fusionnée');
      expect(el(fixture).querySelector('.mr-detail__actions')).toBeNull();
      expect(toasts()).toContainEqual(['success', 'Demande de fusion fusionnée.']);
    });

    it('shows an error toast when the merge request fails', () => {
      const { fixture, httpMock } = setupLoaded();

      headerButton(fixture, 'Fusionner')!.click();
      httpMock.expectOne('/api/merge-requests/mr-1/merge').flush('boom', { status: 500, statusText: 'Server Error' });

      expect(toasts()).toContainEqual(['error', 'Une erreur est survenue pendant la fusion.']);
    });

    it('shows the conflict alert above the tabs and does not reload the timeline when the merge conflicts', () => {
      const { fixture, httpMock } = setupLoaded();

      Array.from<HTMLButtonElement>(el(fixture).querySelectorAll('.mr-detail__actions button')).find((b) => b.textContent?.trim() === 'Fusionner')!.click();

      httpMock.expectOne('/api/merge-requests/mr-1/merge').flush({ outcome: 'conflicting' });
      fixture.detectChanges();

      httpMock.expectNone('/api/merge-requests/mr-1/timeline');
      expect(el(fixture).textContent).toContain('Impossible de fusionner automatiquement');
      const alert = main(fixture).querySelector('gbt-alert')!;
      expect(alert).toBeTruthy();
      expect(alert.compareDocumentPosition(main(fixture).querySelector('gbt-tabs')!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    });

    it('drops the conflict alert once the merge request is closed', () => {
      const { fixture, httpMock } = setupLoaded();

      Array.from<HTMLButtonElement>(el(fixture).querySelectorAll('.mr-detail__actions button')).find((b) => b.textContent?.trim() === 'Fusionner')!.click();
      httpMock.expectOne('/api/merge-requests/mr-1/merge').flush({ outcome: 'conflicting' });
      fixture.detectChanges();
      expect(el(fixture).textContent).toContain('Impossible de fusionner automatiquement');

      Array.from<HTMLButtonElement>(el(fixture).querySelectorAll('.mr-detail__actions button')).find((b) => b.textContent?.trim() === 'Fermer')!.click();
      httpMock.expectOne('/api/merge-requests/mr-1/close').flush(null);
      httpMock.expectOne('/api/merge-requests/mr-1').flush(mergeRequestBody({ status: 'closed' }));
      httpMock.expectOne('/api/merge-requests/mr-1/timeline').flush(emptyTimeline);
      fixture.detectChanges();

      expect(el(fixture).querySelector('.gbt-page-header fg-status-badge')!.textContent).toContain('Fermée');
      expect(el(fixture).textContent).not.toContain('Impossible de fusionner automatiquement');
    });

    it('closes the merge request, then re-fetches its detail and reloads the timeline', () => {
      const { fixture, httpMock } = setupLoaded();

      Array.from<HTMLButtonElement>(el(fixture).querySelectorAll('.mr-detail__actions button')).find((b) => b.textContent?.trim() === 'Fermer')!.click();

      const post = httpMock.expectOne('/api/merge-requests/mr-1/close');
      expect(post.request.method).toBe('POST');
      post.flush(null);

      httpMock.expectOne('/api/merge-requests/mr-1').flush(mergeRequestBody({ status: 'closed' }));
      httpMock.expectOne('/api/merge-requests/mr-1/timeline').flush(emptyTimeline);
      fixture.detectChanges();
      expect(el(fixture).querySelector('.gbt-page-header fg-status-badge')!.textContent).toContain('Fermée');
      expect(toasts()).toContainEqual(['success', 'Demande de fusion fermée.']);
    });

    it('shows an error toast when the merge request cannot be closed', () => {
      const { fixture, httpMock } = setupLoaded();

      headerButton(fixture, 'Fermer')!.click();
      httpMock.expectOne('/api/merge-requests/mr-1/close').flush('boom', { status: 500, statusText: 'Server Error' });

      expect(toasts()).toContainEqual(['error', 'Impossible de fermer la demande de fusion.']);
    });
  });
});
