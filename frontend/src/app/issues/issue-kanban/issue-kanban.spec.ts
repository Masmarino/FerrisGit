import { TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { provideRouter, Router } from '@angular/router';
import { CdkDrag, CdkDragDrop, CdkDropList } from '@angular/cdk/drag-drop';
import { GbtInput } from '@masmarino/gabarit/input';
import { Select } from '@masmarino/gabarit/select';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { IssueKanban } from './issue-kanban';
import { Issue } from '../issues.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { PageTitleService } from '../../shell/page-title.service';

const ISSUES_URL = '/api/repositories/repo-1/issues';

const LABEL_URGENT = { id: 'label-1', name: 'Urgent', color: '#dc2626', repositoryId: 'repo-1', groupId: null, createdAt: '2026-01-01T00:00:00Z' };
const MILESTONE_V1 = { id: 'm1', title: 'v1.0', description: '', dueDate: null, state: 'open', repositoryId: 'repo-1', groupId: null, createdAt: '2026-01-01T00:00:00Z' };

function issue(overrides: Partial<Issue> = {}): Issue {
  return {
    id: 'i1',
    number: 1,
    authorId: 'u1',
    assigneeId: null,
    title: 'Bug',
    description: '',
    status: 'todo',
    kind: 'bug',
    parentIssueId: null,
    createdAt: '2026-01-01T00:00:00Z',
    closedAt: null,
    milestoneId: null,
    labels: [],
    author: { id: 'u1', username: 'alice' },
    assignee: null,
    commentCount: 0,
    ...overrides,
  };
}

type Role = 'owner' | 'contributor' | 'maintainer' | 'reader';

describe('IssueKanban', () => {
  function setup(options: { role?: Role } = {}) {
    TestBed.configureTestingModule({
      providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([])],
    });
    if (options.role) {
      TestBed.inject(RepositoryContextService).current.set({ repositoryId: 'repo-1', path: ['acme', 'widget'], role: options.role, ancestors: [], groupId: null });
    }
    const fixture = TestBed.createComponent(IssueKanban);
    fixture.componentRef.setInput('repositoryId', 'repo-1');
    fixture.componentRef.setInput('path', ['acme', 'widget']);
    const http = TestBed.inject(HttpTestingController);
    const el = fixture.nativeElement as HTMLElement;
    return { fixture, http, el };
  }

  function loaded(issues: Issue[], options: { role?: Role; labels?: unknown[]; milestones?: unknown[] } = {}) {
    const ctx = setup({ role: options.role ?? 'contributor' });
    ctx.fixture.detectChanges();
    ctx.http.expectOne(ISSUES_URL).flush(issues);
    ctx.http.expectOne('/api/repositories/repo-1/labels').flush(options.labels ?? [LABEL_URGENT]);
    ctx.http.expectOne('/api/repositories/repo-1/milestones').flush(options.milestones ?? [MILESTONE_V1]);
    ctx.fixture.detectChanges();
    return ctx;
  }

  const text = (el: Element | null | undefined) => (el?.textContent ?? '').replace(/\s+/g, ' ').trim();
  const lane = (el: HTMLElement, status: string) => el.querySelector<HTMLElement>(`.issue-kanban__lane[data-status="${status}"]`)!;
  const cards = (root: Element) => Array.from(root.querySelectorAll<HTMLElement>('.issue-kanban__card'));
  const cardTitles = (root: Element) => cards(root).map((card) => text(card.querySelector('.issue-kanban__card-link')));
  const laneCount = (el: HTMLElement, status: string) => text(lane(el, status).querySelector('.issue-kanban__lane-header gbt-badge'));
  const buttonByText = (root: Element, label: string) => Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find((button) => text(button) === label);
  const searchInput = (el: HTMLElement) => el.querySelector<HTMLInputElement>('.issue-kanban__search input')!;

  function type(input: HTMLInputElement, value: string, fixture: { detectChanges(): void }) {
    input.value = value;
    input.dispatchEvent(new Event('input'));
    fixture.detectChanges();
  }

  function openSelectAndPick(fixture: { detectChanges(): void; nativeElement: HTMLElement }, selectIndex: number, optionLabel: string): void {
    const trigger = fixture.nativeElement.querySelectorAll<HTMLButtonElement>('.gbt-select__trigger')[selectIndex];
    trigger.click();
    fixture.detectChanges();
    const option = Array.from(document.querySelectorAll<HTMLElement>('.gbt-select__option')).find((el) => el.textContent?.trim() === optionLabel);
    option!.click();
    fixture.detectChanges();
  }

  function openMoveMenu(fixture: { detectChanges(): void }, card: HTMLElement): HTMLButtonElement[] {
    card.querySelector<HTMLButtonElement>('.issue-kanban__move .gbt-menu__trigger')!.click();
    fixture.detectChanges();
    return Array.from(card.querySelectorAll<HTMLButtonElement>('[role="menuitem"]'));
  }

  function drop(fixture: ReturnType<typeof setup>['fixture'], from: string, to: string, previousIndex: number, currentIndex: number): void {
    const lists = fixture.debugElement.queryAll(By.directive(CdkDropList)).map((debug) => debug.injector.get(CdkDropList));
    const list = (status: string) => lists.find((candidate) => candidate.id === `column-${status}`)!;
    const source = list(from);
    const dragged = fixture.debugElement
      .queryAll(By.directive(CdkDrag))
      .map((debug) => debug.injector.get(CdkDrag))
      .find((drag) => drag.data === source.data[previousIndex])!;
    const event = {
      previousContainer: source,
      container: list(to),
      previousIndex,
      currentIndex,
      item: dragged,
      isPointerOverContainer: true,
      distance: { x: 0, y: 0 },
      dropPoint: { x: 0, y: 0 },
      event: new MouseEvent('mouseup'),
    } as unknown as CdkDragDrop<Issue[]>;
    list(to).dropped.emit(event);
    fixture.detectChanges();
  }

  describe('page frame', () => {
    it('puts "Tickets" in the shell header', () => {
      loaded([issue()]);

      expect(TestBed.inject(PageTitleService).title()).toBe('Tickets');
    });

    it('lays the board out on a full-width page layout titled "Tickets"', () => {
      const { el } = loaded([issue()]);

      const layout = el.querySelector('gbt-page-layout');
      expect(layout?.getAttribute('data-width')).toBe('full');
      expect(text(el.querySelector('gbt-page-header h1'))).toBe('Tickets');
      expect(el.querySelector('.gbt-page-layout__main .issue-kanban__board')).toBeTruthy();
    });

    it('links back to the list view with a secondary "Vue liste" button, and has no primary button', () => {
      const { fixture, el } = loaded([issue()]);
      const navigate = vi.spyOn(TestBed.inject(Router), 'navigate').mockResolvedValue(true);

      const button = buttonByText(el.querySelector('.gbt-page-header__actions')!, 'Vue liste');
      expect(button?.classList).toContain('gbt-button--secondary');
      button!.click();
      fixture.detectChanges();

      expect(navigate).toHaveBeenCalledWith(['/repositories', 'acme', 'widget', '-', 'issues']);
      expect(el.querySelector('.gbt-button--primary')).toBeNull();
    });
  });

  describe('lanes', () => {
    it('shows the four status lanes in order, headed by their French label', () => {
      const { el } = loaded([issue()]);

      const lanes = Array.from(el.querySelectorAll<HTMLElement>('.issue-kanban__lane'));
      expect(lanes.map((section) => section.dataset['status'])).toEqual(['todo', 'in_progress', 'in_review', 'done']);
      expect(lanes.map((section) => text(section.querySelector('h2.issue-kanban__lane-title')))).toEqual(['À faire', 'En cours', 'En revue', 'Terminé']);
      for (const section of lanes) {
        const heading = section.querySelector('h2')!;
        expect(section.getAttribute('aria-labelledby')).toBe(heading.id);
      }
    });

    it('places each issue in the lane of its status and counts the cards of every lane', () => {
      const { el } = loaded([
        issue({ id: 'a', number: 1, status: 'todo' }),
        issue({ id: 'b', number: 2, status: 'todo' }),
        issue({ id: 'c', number: 3, status: 'in_progress' }),
        issue({ id: 'd', number: 4, status: 'done', closedAt: '2026-01-03T00:00:00Z' }),
      ]);

      expect(cards(lane(el, 'todo')).length).toBe(2);
      expect(cards(lane(el, 'in_progress')).length).toBe(1);
      expect(cards(lane(el, 'in_review')).length).toBe(0);
      expect(cards(lane(el, 'done')).length).toBe(1);
      expect(['todo', 'in_progress', 'in_review', 'done'].map((status) => laneCount(el, status))).toEqual(['2', '1', '0', '1']);
      expect(lane(el, 'todo').querySelectorAll('ul#column-todo > li.issue-kanban__card').length).toBe(2);
    });

    it('shows a muted "Aucun ticket" drop zone in an empty lane, and none in a lane with cards', () => {
      const { el } = loaded([issue({ status: 'todo' })]);

      expect(lane(el, 'todo').querySelector('.issue-kanban__empty')).toBeNull();
      for (const status of ['in_progress', 'in_review', 'done']) {
        expect(text(lane(el, status).querySelector('.issue-kanban__empty'))).toBe('Aucun ticket');
      }
    });

    it('shows skeleton cards in a busy board until the issues arrive', () => {
      const { fixture, http, el } = setup({ role: 'contributor' });
      fixture.detectChanges();

      expect(el.querySelector('.issue-kanban__board')?.getAttribute('aria-busy')).toBe('true');
      expect(el.querySelectorAll('.issue-kanban__skeleton').length).toBeGreaterThan(0);
      expect(cards(el).length).toBe(0);
      expect(el.querySelector('.issue-kanban__empty')).toBeNull();

      http.expectOne(ISSUES_URL).flush([issue()]);
      http.expectOne('/api/repositories/repo-1/labels').flush([]);
      http.expectOne('/api/repositories/repo-1/milestones').flush([]);
      fixture.detectChanges();

      expect(el.querySelector('.issue-kanban__board')?.getAttribute('aria-busy')).toBeNull();
      expect(el.querySelector('.issue-kanban__skeleton')).toBeNull();
      expect(cards(el).length).toBe(1);
    });

    it('says the board could not be loaded, with an error toast, when the request fails', () => {
      const { fixture, http, el } = setup({ role: 'contributor' });
      fixture.detectChanges();
      http.expectOne(ISSUES_URL).flush('boom', { status: 500, statusText: 'Internal Server Error' });
      http.expectOne('/api/repositories/repo-1/labels').flush([]);
      http.expectOne('/api/repositories/repo-1/milestones').flush([]);
      fixture.detectChanges();

      expect(TestBed.inject(GbtToastService).toasts().at(-1)).toMatchObject({ message: 'Impossible de charger les tickets.', variant: 'error' });
      const failed = el.querySelector('gbt-alert .gbt-alert');
      expect(text(failed)).toBe("Les tickets n'ont pas pu être chargés.");
      // The toast announces it, so the inline message stays silent: one live region, not two.
      expect(failed?.getAttribute('data-variant')).toBe('error');
      expect(failed?.getAttribute('role')).toBeNull();
      expect(failed?.getAttribute('aria-live')).toBeNull();
      expect(el.querySelector('.issue-kanban__lane')).toBeNull();
    });
  });

  describe('cards', () => {
    it('links "#n title" to the issue page, with the full title on hover', () => {
      const { el } = loaded([issue({ number: 12, title: 'La pagination saute une page' })]);

      const link = el.querySelector<HTMLAnchorElement>('.issue-kanban__card .issue-kanban__card-link')!;
      expect(link.getAttribute('href')).toBe('/repositories/acme/widget/-/issues/12');
      expect(text(link)).toBe('#12 La pagination saute une page');
      expect(link.getAttribute('title')).toBe('La pagination saute une page');
    });

    it('shows the kind, the label tags and the milestone on the card', () => {
      const { el } = loaded([issue({ kind: 'feature', labels: [LABEL_URGENT], milestoneId: 'm1' })]);

      const card = cards(el)[0];
      expect(text(card.querySelector('.issue-kanban__kind'))).toBe('Fonctionnalité');
      expect(Array.from(card.querySelectorAll('gbt-tag'), text)).toEqual(['Urgent']);
      expect(text(card.querySelector('.issue-kanban__milestone'))).toBe('v1.0');
    });

    it('renders no label tag (and no milestone) for a card without any', () => {
      const { el } = loaded([issue({ labels: [], milestoneId: null })]);

      const card = cards(el)[0];
      expect(card.querySelector('gbt-tag')).toBeNull();
      expect(card.querySelector('.issue-kanban__milestone')).toBeNull();
      expect(text(card.querySelector('.issue-kanban__kind'))).toBe('Bug');
    });

    it("shows the assignee's avatar chip, named for screen readers, and nothing when unassigned", () => {
      const { el } = loaded([
        issue({ id: 'a', number: 1, assignee: { id: 'u2', username: 'bastien' } }),
        issue({ id: 'b', number: 2, assignee: null }),
      ]);

      const [assigned, unassigned] = cards(el);
      expect(text(assigned.querySelector('.issue-kanban__assignee gbt-user-chip .gbt-user-chip__name'))).toBe('bastien');
      expect(assigned.querySelector('.issue-kanban__assignee .gbt-user-chip')?.getAttribute('title')).toBe('bastien');
      expect(text(assigned.querySelector('.issue-kanban__assignee .sr-only'))).toBe('Assigné à');
      expect(unassigned.querySelector('.issue-kanban__assignee')).toBeNull();
    });

    it('shows the comment count (read out in words) only when there are comments', () => {
      const { el } = loaded([
        issue({ id: 'a', number: 1, commentCount: 3 }),
        issue({ id: 'b', number: 2, commentCount: 1 }),
        issue({ id: 'c', number: 3, commentCount: 0 }),
      ]);

      const [three, one, none] = cards(el);
      expect(text(three.querySelector('.issue-kanban__comments .sr-only'))).toBe('3 commentaires');
      expect(three.querySelector('.issue-kanban__comments')?.getAttribute('title')).toBe('3 commentaires');
      expect(text(three.querySelector('.issue-kanban__comments .issue-kanban__comment-count'))).toBe('3');
      expect(text(one.querySelector('.issue-kanban__comments .sr-only'))).toBe('1 commentaire');
      expect(none.querySelector('.issue-kanban__comments')).toBeNull();
    });
  });

  describe('filters', () => {
    it('names its three toolbar fields by labels that stay hidden (the placeholders and the magnifier say it on screen)', () => {
      const { el, fixture } = loaded([issue()]);

      const search = fixture.debugElement.query(By.css('.issue-kanban__search')).componentInstance as GbtInput;
      expect(search.hideLabel()).toBe(true);
      expect(search.leadingIcon()).toBe('search');
      const selects = fixture.debugElement.queryAll(By.directive(Select)).map((d) => d.componentInstance as Select);
      expect(selects.map((s) => [s.label(), s.hideLabel(), s.fullWidth()])).toEqual([
        ['Filtrer par label', true, true],
        ['Filtrer par milestone', true, true],
      ]);
      expect(Array.from(el.querySelectorAll('.issue-kanban__toolbar label')).map((l) => text(l))).toEqual(['Rechercher un ticket', 'Filtrer par label', 'Filtrer par milestone']);
    });

    it('filters the cards of every lane by title via the search field, and the counts follow', () => {
      const { fixture, el } = loaded([
        issue({ id: 'a', number: 1, title: 'Fix login', status: 'todo' }),
        issue({ id: 'b', number: 2, title: 'Add export', status: 'todo' }),
        issue({ id: 'c', number: 3, title: 'Export CSV', status: 'in_review' }),
      ]);

      type(searchInput(el), 'EXPORT', fixture);

      expect(cardTitles(lane(el, 'todo'))).toEqual(['#2 Add export']);
      expect(cardTitles(lane(el, 'in_review'))).toEqual(['#3 Export CSV']);
      expect(laneCount(el, 'todo')).toBe('1');
      expect(text(el.querySelector('.issue-kanban__summary'))).toBe('2 tickets sur 3');
    });

    it('says how many tickets the board shows when no filter is active', () => {
      const { el } = loaded([issue({ id: 'a', number: 1 }), issue({ id: 'b', number: 2 })]);
      expect(text(el.querySelector('.issue-kanban__summary'))).toBe('2 tickets');
    });

    it('re-fetches the board with the selected label id as a query parameter', () => {
      const { fixture, http } = loaded([issue()]);

      openSelectAndPick(fixture, 0, 'Urgent');

      const req = http.expectOne((r) => r.url === ISSUES_URL && r.params.get('labelIds') === 'label-1');
      req.flush([]);
    });

    it('re-fetches the board with the selected milestone id as a query parameter', () => {
      const { fixture, http } = loaded([issue()]);

      openSelectAndPick(fixture, 1, 'v1.0');

      const req = http.expectOne((r) => r.url === ISSUES_URL && r.params.get('milestoneId') === 'm1');
      req.flush([]);
    });

    it('clears the milestone filter via "Tous les milestones", re-fetching without a milestoneId param', () => {
      const { fixture, http } = loaded([issue()]);

      openSelectAndPick(fixture, 1, 'v1.0');
      http.expectOne((r) => r.url === ISSUES_URL && r.params.get('milestoneId') === 'm1').flush([]);

      openSelectAndPick(fixture, 1, 'Tous les milestones');

      const req = http.expectOne((r) => r.url === ISSUES_URL);
      expect(req.request.params.has('milestoneId')).toBe(false);
      req.flush([]);
    });

    it('shows "Réinitialiser" only while a filter is active', () => {
      const { fixture, el } = loaded([issue()]);
      const resetButton = () => buttonByText(el.querySelector('.issue-kanban__toolbar')!, 'Réinitialiser');
      expect(resetButton()).toBeUndefined();

      type(searchInput(el), 'zzz', fixture);
      expect(resetButton()).toBeTruthy();
      expect(cards(el).length).toBe(0);

      resetButton()!.click();
      fixture.detectChanges();
      expect(resetButton()).toBeUndefined();
      expect(cards(el).length).toBe(1);
    });

    it('resets the search, labels and milestone at once, re-fetching without any filter', async () => {
      const { fixture, http, el } = loaded([issue()]);
      type(searchInput(el), 'zzz', fixture);
      openSelectAndPick(fixture, 0, 'Urgent');
      http.expectOne((r) => r.url === ISSUES_URL && r.params.get('labelIds') === 'label-1').flush([]);
      openSelectAndPick(fixture, 1, 'v1.0');
      http.expectOne((r) => r.url === ISSUES_URL && r.params.get('milestoneId') === 'm1').flush([]);

      buttonByText(el.querySelector('.issue-kanban__toolbar')!, 'Réinitialiser')!.click();
      fixture.detectChanges();

      const req = http.expectOne((r) => r.url === ISSUES_URL);
      expect(req.request.params.keys()).toEqual([]);
      req.flush([issue()]);
      fixture.detectChanges();
      await fixture.whenStable();
      fixture.detectChanges();

      expect(cards(el).length).toBe(1);
      expect(searchInput(el).value).toBe('');
    });

    it('hides the filters while the repository has no issue at all, but keeps the empty lanes', () => {
      const { el } = loaded([]);

      expect(el.querySelector('.issue-kanban__toolbar')).toBeNull();
      expect(el.querySelectorAll('.issue-kanban__lane').length).toBe(4);
      expect(el.querySelectorAll('.issue-kanban__empty').length).toBe(4);
    });

    it('keeps the filters when a server filter returns nothing', () => {
      const { fixture, http, el } = loaded([issue()]);
      openSelectAndPick(fixture, 0, 'Urgent');
      http.expectOne((r) => r.url === ISSUES_URL && r.params.get('labelIds') === 'label-1').flush([]);
      fixture.detectChanges();

      expect(el.querySelector('.issue-kanban__toolbar')).toBeTruthy();
      expect(el.querySelectorAll('.issue-kanban__empty').length).toBe(4);
    });

    it('keeps the filters when refetching with a server filter fails, so the filter can be cleared', async () => {
      const { fixture, http, el } = loaded([issue()]);
      openSelectAndPick(fixture, 0, 'Urgent');
      http.expectOne((r) => r.url === ISSUES_URL && r.params.get('labelIds') === 'label-1').flush('boom', { status: 500, statusText: 'Internal Server Error' });
      fixture.detectChanges();

      expect(TestBed.inject(GbtToastService).toasts().at(-1)).toMatchObject({ message: 'Impossible de charger les tickets.', variant: 'error' });
      const failed = el.querySelector('gbt-alert .gbt-alert');
      expect(text(failed)).toBe("Les tickets n'ont pas pu être chargés.");
      // The toast announces it, so the inline message stays silent: one live region, not two.
      expect(failed?.getAttribute('data-variant')).toBe('error');
      expect(failed?.getAttribute('role')).toBeNull();
      expect(failed?.getAttribute('aria-live')).toBeNull();
      const toolbar = el.querySelector('.issue-kanban__toolbar');
      expect(toolbar).toBeTruthy();
      expect(el.querySelector('.issue-kanban__summary')).toBeNull();

      buttonByText(toolbar!, 'Réinitialiser')!.click();
      fixture.detectChanges();
      const req = http.expectOne((r) => r.url === ISSUES_URL);
      expect(req.request.params.keys()).toEqual([]);
      req.flush([issue()]);
      fixture.detectChanges();
      await fixture.whenStable();
      fixture.detectChanges();

      expect(el.querySelector('gbt-alert')).toBeNull();
      expect(cards(el).length).toBe(1);
      expect(el.querySelector('.issue-kanban__toolbar')).toBeTruthy();
    });
  });

  describe('moving a card by drag and drop', () => {
    it('moves the card to the lane it is dropped on and saves the new status', () => {
      const { fixture, http, el } = loaded([issue({ id: 'a', number: 1, status: 'todo' }), issue({ id: 'b', number: 2, status: 'in_progress' })]);

      drop(fixture, 'todo', 'in_progress', 0, 0);

      const req = http.expectOne(`${ISSUES_URL}/1/status`);
      expect(req.request.method).toBe('PATCH');
      expect(req.request.body).toEqual({ status: 'in_progress' });
      req.flush(issue({ id: 'a', number: 1, status: 'in_progress' }));
      fixture.detectChanges();

      expect(cards(lane(el, 'todo')).length).toBe(0);
      expect(cardTitles(lane(el, 'in_progress'))).toEqual(['#1 Bug', '#2 Bug']);
      expect(laneCount(el, 'in_progress')).toBe('2');
    });

    it('puts the card back where it was, with an error toast, when saving the status fails', () => {
      const { fixture, http, el } = loaded([
        issue({ id: 'a', number: 1, title: 'A', status: 'todo' }),
        issue({ id: 'b', number: 2, title: 'B', status: 'todo' }),
        issue({ id: 'c', number: 3, title: 'C', status: 'todo' }),
      ]);

      drop(fixture, 'todo', 'done', 1, 0);
      expect(cardTitles(lane(el, 'done'))).toEqual(['#2 B']);

      http.expectOne(`${ISSUES_URL}/2/status`).flush('boom', { status: 500, statusText: 'Internal Server Error' });
      fixture.detectChanges();

      expect(cardTitles(lane(el, 'todo'))).toEqual(['#1 A', '#2 B', '#3 C']);
      expect(cards(lane(el, 'done')).length).toBe(0);
      expect(TestBed.inject(GbtToastService).toasts().at(-1)).toMatchObject({ message: 'Impossible de déplacer le ticket.', variant: 'error' });
    });

    it('reorders the cards of a lane without saving anything when dropped in its own lane', () => {
      const { fixture, http, el } = loaded([
        issue({ id: 'a', number: 1, title: 'A' }),
        issue({ id: 'b', number: 2, title: 'B' }),
        issue({ id: 'c', number: 3, title: 'C' }),
      ]);

      drop(fixture, 'todo', 'todo', 2, 0);

      http.expectNone(() => true);
      expect(cardTitles(lane(el, 'todo'))).toEqual(['#3 C', '#1 A', '#2 B']);
    });

    it('drops at the right place among the hidden cards while a search is active', () => {
      const { fixture, http, el } = loaded([
        issue({ id: 'a', number: 1, title: 'Export A', status: 'todo' }),
        issue({ id: 'x', number: 9, title: 'Hidden', status: 'in_progress' }),
        issue({ id: 'b', number: 2, title: 'Export B', status: 'in_progress' }),
      ]);
      type(searchInput(el), 'export', fixture);
      expect(cardTitles(lane(el, 'in_progress'))).toEqual(['#2 Export B']);

      drop(fixture, 'todo', 'in_progress', 0, 0);
      http.expectOne(`${ISSUES_URL}/1/status`).flush(issue({ id: 'a', number: 1, status: 'in_progress' }));
      fixture.detectChanges();

      type(searchInput(el), '', fixture);
      expect(cardTitles(lane(el, 'in_progress'))).toEqual(['#9 Hidden', '#1 Export A', '#2 Export B']);
    });

    it('lets a writer drag the cards', () => {
      const { el } = loaded([issue()]);
      expect(cards(el)[0].classList).not.toContain('cdk-drag-disabled');
      expect(cards(el)[0].classList).toContain('issue-kanban__card--draggable');
    });

    it('does not let a reader drag the cards', () => {
      const { el } = loaded([issue()], { role: 'reader' });
      expect(cards(el)[0].classList).toContain('cdk-drag-disabled');
      expect(cards(el)[0].classList).not.toContain('issue-kanban__card--draggable');
    });

    // A swipe that starts on a card must scroll the board (or the page), not pick it up: touch drags start after a
    // 250ms press. The mouse still drags immediately.
    const dragsOf = (fixture: ReturnType<typeof setup>['fixture']) => fixture.debugElement.queryAll(By.directive(CdkDrag)).map((debug) => debug.injector.get(CdkDrag));

    it('starts a touch drag only after a long press, and a mouse drag at once', () => {
      const { fixture, el } = loaded([issue({ id: 'a', number: 1 }), issue({ id: 'b', number: 2, status: 'done' })]);

      const drags = dragsOf(fixture);
      expect(drags.length).toBe(2);
      for (const drag of drags) {
        expect(drag.dragStartDelay).toEqual({ touch: 250, mouse: 0 });
      }
      expect(drags[0].dragStartDelay).toBe(drags[1].dragStartDelay);
      // At rest the card lets the browser pan: the CDK only blocks touch gestures once a drag has started.
      for (const card of cards(el)) {
        expect(card.style.touchAction).not.toBe('none');
      }
    });

    it("leaves a reader's cards scrollable by touch (their drag is disabled)", () => {
      const { fixture, el } = loaded([issue()], { role: 'reader' });

      const [drag] = dragsOf(fixture);
      expect(drag.disabled).toBe(true);
      expect(cards(el)[0].style.touchAction).not.toBe('none');
    });
  });

  describe('moving a card with its status menu', () => {
    it('offers the three other statuses, named after the ticket', () => {
      const { fixture, el } = loaded([issue({ number: 7, status: 'in_progress' })]);
      const card = cards(el)[0];

      expect(card.querySelector('.issue-kanban__move .gbt-menu__trigger')?.getAttribute('aria-label')).toBe('Déplacer le ticket #7 vers');
      const items = openMoveMenu(fixture, card);
      expect(items.map(text)).toEqual(['À faire', 'En revue', 'Terminé']);
    });

    it('moves the card to the chosen lane with the same status update as a drag, announces it and keeps the focus on it', async () => {
      const { fixture, http, el } = loaded([issue({ number: 1, status: 'todo' })]);

      openMoveMenu(fixture, cards(el)[0])
        .find((item) => text(item) === 'En cours')!
        .click();
      fixture.detectChanges();
      await fixture.whenStable();

      const req = http.expectOne(`${ISSUES_URL}/1/status`);
      expect(req.request.body).toEqual({ status: 'in_progress' });
      req.flush(issue({ status: 'in_progress' }));
      fixture.detectChanges();

      expect(cards(lane(el, 'todo')).length).toBe(0);
      expect(cardTitles(lane(el, 'in_progress'))).toEqual(['#1 Bug']);
      expect(text(el.querySelector('.issue-kanban__announcer'))).toBe('Ticket #1 déplacé vers En cours');
      expect(document.activeElement).toBe(lane(el, 'in_progress').querySelector('.issue-kanban__card-link'));
    });

    it('reverts the move if the status update request fails', () => {
      const { fixture, http, el } = loaded([issue({ status: 'todo' })]);

      openMoveMenu(fixture, cards(el)[0])
        .find((item) => text(item) === 'Terminé')!
        .click();
      fixture.detectChanges();

      http.expectOne(`${ISSUES_URL}/1/status`).flush('server error', { status: 500, statusText: 'Internal Server Error' });
      fixture.detectChanges();

      expect(cards(lane(el, 'todo')).length).toBe(1);
      expect(cards(lane(el, 'done')).length).toBe(0);
      expect(TestBed.inject(GbtToastService).toasts().at(-1)).toMatchObject({ message: 'Impossible de déplacer le ticket.', variant: 'error' });
    });

    it('gives a reader no status menu', () => {
      const { el } = loaded([issue()], { role: 'reader' });
      expect(el.querySelector('.issue-kanban__move')).toBeNull();
    });
  });
});
