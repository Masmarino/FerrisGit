import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { Notification, NotificationsService } from './notifications.service';

describe('NotificationsService', () => {
  beforeEach(() => {
    TestBed.configureTestingModule({ providers: [NotificationsService, provideHttpClient(), provideHttpClientTesting()] });
  });

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  const sampleNotification: Notification = {
    id: '1',
    kind: 'merge_request_approved',
    repositoryOwner: 'alice',
    repositoryName: 'hello',
    actorUsername: 'bob',
    mergeRequestId: '42',
    mergeRequestTitle: 'Add feature',
    pipelineId: null,
    commitSha: null,
    issueId: null,
    issueNumber: null,
    issueTitle: null,
    role: null,
    read: false,
    createdAt: '2026-01-01T00:00:00Z',
  };

  it('lists notifications from the API', () => {
    const service = TestBed.inject(NotificationsService);
    const http = TestBed.inject(HttpTestingController);

    let result: unknown;
    service.list().subscribe((notifications) => (result = notifications));
    const req = http.expectOne('/api/notifications');
    expect(req.request.method).toBe('GET');
    req.flush([sampleNotification]);

    expect(result).toEqual([sampleNotification]);
  });

  it('fetches the unread count from the API', () => {
    const service = TestBed.inject(NotificationsService);
    const http = TestBed.inject(HttpTestingController);

    let result: unknown;
    service.unreadCount().subscribe((count) => (result = count));
    const req = http.expectOne('/api/notifications/unread-count');
    expect(req.request.method).toBe('GET');
    req.flush({ count: 3 });

    expect(result).toEqual({ count: 3 });
  });

  it('marks a single notification as read', () => {
    const service = TestBed.inject(NotificationsService);
    const http = TestBed.inject(HttpTestingController);

    let called = false;
    service.markRead('1').subscribe(() => (called = true));
    const req = http.expectOne('/api/notifications/1/read');
    expect(req.request.method).toBe('POST');
    expect(req.request.body).toEqual({});
    req.flush(null);

    expect(called).toBe(true);
  });

  it('marks all notifications as read', () => {
    const service = TestBed.inject(NotificationsService);
    const http = TestBed.inject(HttpTestingController);

    let called = false;
    service.markAllRead().subscribe(() => (called = true));
    const req = http.expectOne('/api/notifications/read-all');
    expect(req.request.method).toBe('POST');
    expect(req.request.body).toEqual({});
    req.flush(null);

    expect(called).toBe(true);
  });
});
