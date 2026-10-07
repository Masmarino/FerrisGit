import { TestBed } from '@angular/core/testing';
import { Router, provideRouter } from '@angular/router';
import { of } from 'rxjs';
import { NotificationBell } from './notification-bell';
import { Notification, NotificationsService } from '../../notifications/notifications.service';

function makeNotification(overrides: Partial<Notification>): Notification {
  return {
    id: '1',
    kind: 'merge_request_approved',
    repositoryOwner: 'alice',
    repositoryName: 'hello',
    actorUsername: 'bob',
    mergeRequestId: null,
    mergeRequestTitle: null,
    pipelineId: null,
    commitSha: null,
    role: null,
    issueId: null,
    issueNumber: null,
    issueTitle: null,
    read: false,
    createdAt: '2026-01-01T00:00:00Z',
    ...overrides,
  };
}

describe('NotificationBell', () => {
  function setup() {
    const notificationsServiceStub = {
      list: vi.fn(() => of<Notification[]>([])),
      unreadCount: vi.fn(() => of({ count: 0 })),
      markRead: vi.fn(() => of(undefined)),
      markAllRead: vi.fn(() => of(undefined)),
    };
    TestBed.configureTestingModule({
      providers: [provideRouter([]), { provide: NotificationsService, useValue: notificationsServiceStub }],
    });
    const fixture = TestBed.createComponent(NotificationBell);
    const router = TestBed.inject(Router);
    const navigateSpy = vi.spyOn(router, 'navigate').mockResolvedValue(true);
    return { fixture, component: fixture.componentInstance, notificationsServiceStub, navigateSpy };
  }

  function renderWithNotification(n: Notification) {
    const result = setup();
    result.notificationsServiceStub.list.mockReturnValue(of([n]));
    result.fixture.detectChanges();
    const trigger: HTMLButtonElement = result.fixture.nativeElement.querySelector('button[aria-haspopup]');
    trigger.click();
    result.fixture.detectChanges();
    return result;
  }

  function itemText(fixture: ReturnType<typeof setup>['fixture']): string {
    const li: HTMLLIElement = fixture.nativeElement.querySelector('.notification-bell__list li');
    return li.textContent?.trim() ?? '';
  }

  describe('trigger and "Tout marquer comme lu"', () => {
    const triggerOf = (fixture: ReturnType<typeof setup>['fixture']): HTMLButtonElement => fixture.nativeElement.querySelector('button[aria-haspopup]');

    it('is an icon-only Gabarit button named "Notifications", announcing the popup it opens', () => {
      const { fixture } = setup();
      fixture.detectChanges();
      const trigger = triggerOf(fixture);

      expect(trigger.classList).toContain('gbt-button--icon-only');
      expect(trigger.getAttribute('aria-label')).toBe('Notifications');
      expect(trigger.getAttribute('aria-haspopup')).toBe('true');
      expect(trigger.getAttribute('aria-expanded')).toBe('false');
      expect(trigger.getAttribute('aria-controls')).toBeTruthy();
    });

    it('reflects the open state in aria-expanded, and controls the panel it opens', () => {
      const { fixture } = setup();
      fixture.detectChanges();
      const trigger = triggerOf(fixture);

      trigger.click();
      fixture.detectChanges();

      expect(trigger.getAttribute('aria-expanded')).toBe('true');
      expect(fixture.nativeElement.querySelector(`#${trigger.getAttribute('aria-controls')}`)).toBeTruthy();
    });

    it('counts the unread notifications in its name', () => {
      const { fixture, notificationsServiceStub } = setup();
      notificationsServiceStub.unreadCount.mockReturnValue(of({ count: 3 }));
      fixture.detectChanges();

      expect(triggerOf(fixture).getAttribute('aria-label')).toBe('Notifications (3 non lues)');
    });

    it('marks everything as read from the link button in the panel header', () => {
      const { fixture, notificationsServiceStub } = renderWithNotification(makeNotification({}));
      const markAll = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll<HTMLButtonElement>('.notification-bell__header button')).find((b) => b.textContent?.trim() === 'Tout marquer comme lu')!;

      expect(markAll.classList).toContain('gbt-button--link');
      markAll.click();

      expect(notificationsServiceStub.markAllRead).toHaveBeenCalledTimes(1);
    });
  });

  describe('sentence()', () => {
    it('renders the merge_request_approved sentence', () => {
      const n = makeNotification({ kind: 'merge_request_approved', actorUsername: 'bob', mergeRequestTitle: 'Add feature', mergeRequestId: '1' });
      const { fixture } = renderWithNotification(n);
      expect(itemText(fixture)).toBe('bob a approuvé votre demande de fusion « Add feature »');
    });

    it('renders the merge_request_changes_requested sentence', () => {
      const n = makeNotification({
        kind: 'merge_request_changes_requested',
        actorUsername: 'bob',
        mergeRequestTitle: 'Add feature',
        mergeRequestId: '1',
      });
      const { fixture } = renderWithNotification(n);
      expect(itemText(fixture)).toBe('bob a demandé des changements sur votre demande de fusion « Add feature »');
    });

    it('renders the merge_request_commented sentence', () => {
      const n = makeNotification({ kind: 'merge_request_commented', actorUsername: 'bob', mergeRequestTitle: 'Add feature', mergeRequestId: '1' });
      const { fixture } = renderWithNotification(n);
      expect(itemText(fixture)).toBe('bob a commenté votre demande de fusion « Add feature »');
    });

    it('renders the merge_request_merged sentence', () => {
      const n = makeNotification({ kind: 'merge_request_merged', actorUsername: 'bob', mergeRequestTitle: 'Add feature', mergeRequestId: '1' });
      const { fixture } = renderWithNotification(n);
      expect(itemText(fixture)).toBe('bob a fusionné votre demande de fusion « Add feature »');
    });

    it('renders the merge_request_closed sentence', () => {
      const n = makeNotification({ kind: 'merge_request_closed', actorUsername: 'bob', mergeRequestTitle: 'Add feature', mergeRequestId: '1' });
      const { fixture } = renderWithNotification(n);
      expect(itemText(fixture)).toBe('bob a fermé votre demande de fusion « Add feature »');
    });

    it('renders the collaborator_added sentence', () => {
      const n = makeNotification({ kind: 'collaborator_added', actorUsername: 'bob', role: 'contributor' });
      const { fixture } = renderWithNotification(n);
      expect(itemText(fixture)).toBe('bob vous a ajouté comme contributeur sur alice/hello');
    });

    it('renders the collaborator_role_changed sentence', () => {
      const n = makeNotification({ kind: 'collaborator_role_changed', actorUsername: 'bob', role: 'maintainer' });
      const { fixture } = renderWithNotification(n);
      expect(itemText(fixture)).toBe('bob a changé votre rôle en mainteneur sur alice/hello');
    });

    it('renders the collaborator_removed sentence', () => {
      const n = makeNotification({ kind: 'collaborator_removed', actorUsername: 'bob' });
      const { fixture } = renderWithNotification(n);
      expect(itemText(fixture)).toBe('bob vous a retiré de alice/hello');
    });

    it('renders the pipeline_failed sentence', () => {
      const n = makeNotification({ kind: 'pipeline_failed', commitSha: 'abcdef1234567890', pipelineId: '9' });
      const { fixture } = renderWithNotification(n);
      expect(itemText(fixture)).toBe('La pipeline sur abcdef12 a échoué (alice/hello)');
    });

    it('falls back to a generic sentence for an unrecognized kind', () => {
      const n = makeNotification({ kind: 'some_future_kind' as unknown as Notification['kind'] });
      const { fixture } = renderWithNotification(n);
      expect(itemText(fixture)).toBe('Nouvelle notification');
    });
  });

  describe('polling', () => {
    beforeEach(() => {
      vi.useFakeTimers();
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    it('polls unreadCount every 20s while alive, and stops after ngOnDestroy', () => {
      const { fixture, notificationsServiceStub } = setup();
      fixture.detectChanges();

      expect(notificationsServiceStub.unreadCount).toHaveBeenCalledTimes(1);

      vi.advanceTimersByTime(20000);
      expect(notificationsServiceStub.unreadCount).toHaveBeenCalledTimes(2);

      fixture.destroy();
      vi.advanceTimersByTime(60000);
      expect(notificationsServiceStub.unreadCount).toHaveBeenCalledTimes(2);
    });
  });

  describe('select()', () => {
    it('navigates to the merge request route when mergeRequestId is set', () => {
      const { component, navigateSpy, notificationsServiceStub } = setup();
      const n = makeNotification({ kind: 'merge_request_approved', mergeRequestId: '42', pipelineId: null });

      component.select(n);

      expect(notificationsServiceStub.markRead).toHaveBeenCalledWith('1');
      expect(navigateSpy).toHaveBeenCalledWith(['/repositories', 'alice', 'hello', '-', 'merge-requests', '42']);
    });

    it('navigates to the pipeline route when pipelineId is set and mergeRequestId is not', () => {
      const { component, navigateSpy, notificationsServiceStub } = setup();
      const n = makeNotification({ kind: 'pipeline_failed', mergeRequestId: null, pipelineId: '7' });

      component.select(n);

      expect(notificationsServiceStub.markRead).toHaveBeenCalledWith('1');
      expect(navigateSpy).toHaveBeenCalledWith(['/repositories', 'alice', 'hello', '-', 'pipelines', '7']);
    });

    it('navigates to the repositories list for a collaborator_removed notification', () => {
      const { component, navigateSpy, notificationsServiceStub } = setup();
      const n = makeNotification({ kind: 'collaborator_removed', mergeRequestId: null, pipelineId: null });

      component.select(n);

      expect(notificationsServiceStub.markRead).toHaveBeenCalledWith('1');
      expect(navigateSpy).toHaveBeenCalledWith(['/repositories']);
    });

    it('navigates to the collaborators section of the repository settings for a collaborator_added notification', () => {
      const { component, navigateSpy, notificationsServiceStub } = setup();
      const n = makeNotification({ kind: 'collaborator_added', mergeRequestId: null, pipelineId: null });

      component.select(n);

      expect(notificationsServiceStub.markRead).toHaveBeenCalledWith('1');
      expect(navigateSpy).toHaveBeenCalledWith(['/repositories', 'alice', 'hello', '-', 'settings'], { queryParams: { section: 'collaborators' } });
    });
  });
});
