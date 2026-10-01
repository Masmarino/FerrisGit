import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { PublicConfigService } from './public-config.service';

describe('PublicConfigService', () => {
  function setup() {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting()] });
    return { service: TestBed.inject(PublicConfigService), http: TestBed.inject(HttpTestingController) };
  }

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  it('reads publicPagesEnabled from /api/auth/config, then answers from memory', () => {
    const { service, http } = setup();
    const answers: (boolean | null)[] = [];

    service.publicPagesEnabled().subscribe((v) => answers.push(v));
    http.expectOne('/api/auth/config').flush({ registrationEnabled: false, passkeysAvailable: true, publicPagesEnabled: true });
    service.publicPagesEnabled().subscribe((v) => answers.push(v));

    expect(answers).toEqual([true, true]);
  });

  it('counts a missing field as closed (a server without public pages)', () => {
    const { service, http } = setup();
    let answer: boolean | null | undefined;

    service.publicPagesEnabled().subscribe((v) => (answer = v));
    http.expectOne('/api/auth/config').flush({ registrationEnabled: false, passkeysAvailable: true });

    expect(answer).toBe(false);
  });

  it('answers null when the request fails, and asks again next time', () => {
    const { service, http } = setup();
    const answers: (boolean | null)[] = [];

    service.publicPagesEnabled().subscribe((v) => answers.push(v));
    http.expectOne('/api/auth/config').flush('down', { status: 503, statusText: 'Unavailable' });
    service.publicPagesEnabled().subscribe((v) => answers.push(v));
    http.expectOne('/api/auth/config').flush({ publicPagesEnabled: false });

    expect(answers).toEqual([null, false]);
  });
});
