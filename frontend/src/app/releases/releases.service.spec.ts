import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { ReleaseDetail, ReleasesService, releaseStatus, ReleaseSummary } from './releases.service';

describe('ReleasesService', () => {
  function setup() {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting(), ReleasesService] });
    return { service: TestBed.inject(ReleasesService), http: TestBed.inject(HttpTestingController) };
  }

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  it('lists releases with their author, notes excerpt and asset count', () => {
    const { service, http } = setup();
    let result: ReleaseSummary[] | undefined;
    service.list('repo-1').subscribe((r) => (result = r));
    const releases: ReleaseSummary[] = [
      {
        id: 'release-1',
        tagName: 'v1.2.0',
        title: 'Version 1.2',
        draft: false,
        prerelease: false,
        authorId: 'user-1',
        author: { id: 'user-1', username: 'alice' },
        notesExcerpt: 'Pagination des tickets côté serveur.',
        assetCount: 3,
        createdAt: '2026-01-01T00:00:00Z',
        publishedAt: '2026-01-02T00:00:00Z',
      },
      {
        id: 'release-2',
        tagName: 'v1.3.0-rc.1',
        title: 'Version 1.3 RC 1',
        draft: true,
        prerelease: true,
        authorId: 'user-gone',
        author: null,
        notesExcerpt: '',
        assetCount: 0,
        createdAt: '2026-02-01T00:00:00Z',
        publishedAt: null,
      },
    ];
    http.expectOne({ url: '/api/repositories/repo-1/releases', method: 'GET' }).flush(releases);

    expect(result).toEqual(releases);
    expect(result?.[0].author?.username).toBe('alice');
    expect(result?.[1].author).toBeNull();
  });

  it('fetches a release with its author and each asset’s uploader', () => {
    const { service, http } = setup();
    let result: ReleaseDetail | undefined;
    service.detail('repo-1', 'v1.2.0').subscribe((r) => (result = r));
    const detail: ReleaseDetail = {
      id: 'release-1',
      tagName: 'v1.2.0',
      title: 'Version 1.2',
      notes: '## Nouveautés',
      draft: false,
      prerelease: false,
      authorId: 'user-1',
      author: { id: 'user-1', username: 'alice' },
      createdAt: '2026-01-01T00:00:00Z',
      publishedAt: '2026-01-02T00:00:00Z',
      targetCommitSha: 'deadbeef',
      assets: [
        {
          id: 'asset-1',
          filename: 'ferrisgit-linux-amd64.tar.gz',
          contentType: 'application/gzip',
          sizeBytes: 12_400_000,
          uploadedBy: 'user-2',
          uploader: { id: 'user-2', username: 'bastien' },
          createdAt: '2026-01-02T00:00:00Z',
        },
      ],
    };
    http.expectOne({ url: '/api/repositories/repo-1/releases/v1.2.0', method: 'GET' }).flush(detail);

    expect(result).toEqual(detail);
    expect(result?.assets[0].uploader?.username).toBe('bastien');
  });
});

describe('releaseStatus', () => {
  it('reads a draft as a draft, whatever its prerelease flag', () => {
    expect(releaseStatus({ draft: true, prerelease: false })).toBe('draft');
    expect(releaseStatus({ draft: true, prerelease: true })).toBe('draft');
  });

  it('reads a published prerelease as a prerelease, anything else as published', () => {
    expect(releaseStatus({ draft: false, prerelease: true })).toBe('prerelease');
    expect(releaseStatus({ draft: false, prerelease: false })).toBe('published');
  });
});
