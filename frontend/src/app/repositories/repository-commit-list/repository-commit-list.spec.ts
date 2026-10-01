import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { RepositoryCommitList } from './repository-commit-list';
import { CommitInfo } from '../repositories.service';
import { PageTitleService } from '../../shell/page-title.service';

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();

const COMMITS: CommitInfo[] = [
  { sha: '9f3c2a17b4e8d6051c2f7a9e3b1d4c6f8a0e2b57', message: 'Refonte de la page\n\nDétails.', authorName: 'Florian Simon', authorEmail: 'f@x.dev', committedAt: '2026-09-30T10:00:00Z' },
  { sha: '4b7e1d9c2a5f8e3b6d0c9a1f4e7b2d5c8a3f6e19', message: 'Premier commit', authorName: 'Alice Martin', authorEmail: 'a@x.dev', committedAt: '2026-09-01T10:00:00Z' },
];

describe('RepositoryCommitList', () => {
  function setup(ref?: string) {
    const pageTitle = { set: vi.fn() };
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting(), { provide: PageTitleService, useValue: pageTitle }] });
    const fixture = TestBed.createComponent(RepositoryCommitList);
    fixture.componentRef.setInput('repositoryId', 'repo-1');
    if (ref) fixture.componentRef.setInput('ref', ref);
    fixture.detectChanges();
    return { fixture, el: fixture.nativeElement as HTMLElement, http: TestBed.inject(HttpTestingController), pageTitle };
  }

  afterEach(() => TestBed.inject(HttpTestingController).verify());

  it('lists the default branch commits, newest first: title, author, short sha and date', () => {
    const { fixture, el, http, pageTitle } = setup();

    const request = http.expectOne((r) => r.url === '/api/repositories/by-id/repo-1/commits');
    expect(request.request.params.has('ref')).toBe(false);
    request.flush(COMMITS);
    fixture.detectChanges();

    expect(pageTitle.set).toHaveBeenCalledWith('Commits');
    expect(text(el.querySelector('.repository-commit-list__ref'))).toBe('Sur la branche par défaut');
    const rows = Array.from(el.querySelectorAll('.repository-commit-list__items > li'));
    expect(rows.map((row) => text(row.querySelector('.repository-commit-list__title')))).toEqual(['Refonte de la page', 'Premier commit']);
    expect(text(rows[0].querySelector('.repository-commit-list__sha'))).toBe('9f3c2a1');
    expect(text(rows[0].querySelector('gbt-user-chip'))).toContain('Florian Simon');
    expect(rows[0].querySelector('time')?.getAttribute('datetime')).toBe('2026-09-30T10:00:00Z');
  });

  it('lists the commits of the given ref', () => {
    const { fixture, el, http } = setup('develop');

    const request = http.expectOne((r) => r.url === '/api/repositories/by-id/repo-1/commits');
    expect(request.request.params.get('ref')).toBe('develop');
    request.flush([]);
    fixture.detectChanges();

    expect(text(el.querySelector('.repository-commit-list__ref'))).toBe('Sur develop');
    expect(text(el.querySelector('gbt-list-card'))).toContain('Aucun commit');
  });

  it('says the commits could not load, and retries', () => {
    const { fixture, el, http } = setup();

    http.expectOne((r) => r.url === '/api/repositories/by-id/repo-1/commits').flush('boom', { status: 500, statusText: 'Server Error' });
    fixture.detectChanges();
    expect(text(el.querySelector('gbt-list-card'))).toContain("Les commits n'ont pas pu être chargés");

    Array.from(el.querySelectorAll<HTMLButtonElement>('gbt-list-card button')).find((b) => text(b) === 'Réessayer')!.click();
    http.expectOne((r) => r.url === '/api/repositories/by-id/repo-1/commits').flush(COMMITS);
    fixture.detectChanges();
    expect(el.querySelectorAll('.repository-commit-list__items > li')).toHaveLength(2);
  });
});
