import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { AdminMetricsService } from './admin-metrics.service';

describe('AdminMetricsService', () => {
  function setup() {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting()] });
    const service = TestBed.inject(AdminMetricsService);
    const http = TestBed.inject(HttpTestingController);
    return { service, http };
  }

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  it('fetches stats from /api/admin/stats', () => {
    const { service, http } = setup();
    let result;
    service.getStats().subscribe((r) => (result = r));

    http.expectOne('/api/admin/stats').flush({ totalUsers: 3, totalRepositories: 5, pipelinesLast7Days: 12 });

    expect(result).toEqual({ totalUsers: 3, totalRepositories: 5, pipelinesLast7Days: 12 });
  });

  it('fetches history with the given day range as a query param', () => {
    const { service, http } = setup();
    service.getHistory(7).subscribe();

    http.expectOne('/api/admin/metrics/history?days=7').flush([]);
  });

  it('fetches health from /api/admin/health', () => {
    const { service, http } = setup();
    let result;
    service.getHealth().subscribe((r) => (result = r));

    const health = {
      database: { status: 'up', detail: null, responseTimeMs: 2, activeConnections: 1, maxConnections: 10, serverVersion: '18.0' },
      storage: { status: 'up', detail: null, usedBytes: 10, freeBytes: 90, totalBytes: 100 },
      uptimeSeconds: 42,
    };
    http.expectOne('/api/admin/health').flush(health);

    expect(result).toEqual(health);
  });
});
