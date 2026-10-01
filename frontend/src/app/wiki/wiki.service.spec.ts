import { TestBed } from '@angular/core/testing';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { provideHttpClient } from '@angular/common/http';
import { WikiService } from './wiki.service';

describe('WikiService', () => {
  let service: WikiService;
  let httpMock: HttpTestingController;

  beforeEach(() => {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting()] });
    service = TestBed.inject(WikiService);
    httpMock = TestBed.inject(HttpTestingController);
  });

  afterEach(() => httpMock.verify());

  it('URL-encodes a slug containing unsafe characters', () => {
    service.detail('r1', 'weird slug/with?chars').subscribe();
    const req = httpMock.expectOne('/api/repositories/r1/wiki/pages/weird%20slug%2Fwith%3Fchars');
    req.flush({ content: '', headSha: 'x', title: '' });
  });

  it('sends baseSha as a query parameter on delete, not a body', () => {
    service.delete('r1', 'Home', 'abc123').subscribe();
    const req = httpMock.expectOne('/api/repositories/r1/wiki/pages/Home?baseSha=abc123');
    expect(req.request.method).toBe('DELETE');
    expect(req.request.body).toBeNull();
    req.flush(null);
  });
});
