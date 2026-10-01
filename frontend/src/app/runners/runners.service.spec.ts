import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { RunnersService } from './runners.service';

describe('RunnersService', () => {
  function setup() {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting(), RunnersService] });
    return { service: TestBed.inject(RunnersService), http: TestBed.inject(HttpTestingController) };
  }

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  it('lists runners', () => {
    const { service, http } = setup();
    let result: unknown;
    service.list().subscribe((r) => (result = r));
    const runners = [{ id: 'runner-1', name: 'ci-1', tags: ['linux'], lastHeartbeatAt: '2026-01-01T00:00:00Z', createdAt: '2026-01-01T00:00:00Z' }];
    http.expectOne({ url: '/api/admin/runners', method: 'GET' }).flush(runners);
    expect(result).toEqual(runners);
  });

  it('registers a runner', () => {
    const { service, http } = setup();
    let result: unknown;
    service.register('ci-1', ['linux', 'docker']).subscribe((r) => (result = r));
    const req = http.expectOne({ url: '/api/admin/runners', method: 'POST' });
    expect(req.request.body).toEqual({ name: 'ci-1', tags: ['linux', 'docker'] });
    const response = { id: 'runner-1', name: 'ci-1', tags: ['linux', 'docker'], token: 'secret-token' };
    req.flush(response);
    expect(result).toEqual(response);
  });
});
