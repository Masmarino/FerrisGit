import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { DashboardService } from './dashboard.service';

describe('DashboardService', () => {
  function setup() {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting()] });
    const service = TestBed.inject(DashboardService);
    const http = TestBed.inject(HttpTestingController);
    return { service, http };
  }

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  it('fetches /api/dashboard', () => {
    const { service, http } = setup();
    let result: unknown;
    service.get().subscribe((r) => (result = r));

    const req = http.expectOne('/api/dashboard');
    req.flush({ assignedIssues: [], authoredIssues: [], authoredMergeRequests: [], mergeRequestsToReview: [], activity: [] });

    expect(result).toEqual({ assignedIssues: [], authoredIssues: [], authoredMergeRequests: [], mergeRequestsToReview: [], activity: [] });
  });
});
