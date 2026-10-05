import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { VersionService } from './version.service';

describe('VersionService', () => {
  function setup() {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting()] });
    return { service: TestBed.inject(VersionService), http: TestBed.inject(HttpTestingController) };
  }

  afterEach(() => TestBed.inject(HttpTestingController).verify());

  it('reads the running version once', () => {
    const { service, http } = setup();

    service.load();
    service.load();
    http.expectOne('/api/version').flush({ version: '0.1.3' });
    service.load();

    expect(service.version()).toBe('0.1.3');
  });

  it('asks again after a failure', () => {
    const { service, http } = setup();

    service.load();
    http.expectOne('/api/version').flush(null, { status: 503, statusText: 'Unavailable' });
    service.load();
    http.expectOne('/api/version').flush({ version: '0.1.3' });

    expect(service.version()).toBe('0.1.3');
  });
});
