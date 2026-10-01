import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { SearchResponse, SearchService } from './search.service';

describe('SearchService', () => {
  function setup() {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting()] });
    return { service: TestBed.inject(SearchService), httpMock: TestBed.inject(HttpTestingController) };
  }

  it('sends the query as a q parameter and returns the response', () => {
    const { service, httpMock } = setup();
    const response: SearchResponse = { repositories: [], issues: [], mergeRequests: [], users: [] };

    let received: SearchResponse | undefined;
    service.search('widget').subscribe((r) => (received = r));

    const req = httpMock.expectOne((r) => r.url === '/api/search' && r.params.get('q') === 'widget');
    req.flush(response);

    expect(received).toEqual(response);
    httpMock.verify();
  });
});
