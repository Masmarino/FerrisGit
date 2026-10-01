import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { PublicReleasesService } from './public-releases.service';

describe('PublicReleasesService', () => {
  function setup() {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting()] });
    return { service: TestBed.inject(PublicReleasesService), http: TestBed.inject(HttpTestingController) };
  }

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  it('reads tags, releases and one release under /api/public/repositories/{id}', () => {
    const { service, http } = setup();

    service.listTags('repo-1').subscribe();
    service.list('repo-1').subscribe();
    service.detail('repo-1', 'v1.0/rc').subscribe();

    http.expectOne('/api/public/repositories/repo-1/tags').flush([]);
    http.expectOne('/api/public/repositories/repo-1/releases').flush([]);
    http.expectOne('/api/public/repositories/repo-1/releases/v1.0%2Frc').flush({});
  });

  it('downloads an asset as a blob', () => {
    const { service, http } = setup();
    let blob: Blob | undefined;

    service.downloadAsset('repo-1', 'v1.0', 'asset-1').subscribe((b) => (blob = b));

    const req = http.expectOne('/api/public/repositories/repo-1/releases/v1.0/assets/asset-1');
    expect(req.request.responseType).toBe('blob');
    req.flush(new Blob(['x']));
    expect(blob).toBeInstanceOf(Blob);
  });
});
