import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { ContributorAvatars } from './contributor-avatars';
import { Contributor } from '../repositories.service';

describe('ContributorAvatars', () => {
  function setup() {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting()] });
    const fixture = TestBed.createComponent(ContributorAvatars);
    fixture.componentRef.setInput('repositoryId', 'repo-1');
    fixture.componentRef.setInput('ref', 'main');
    return { fixture, http: TestBed.inject(HttpTestingController) };
  }

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  function contributor(name: string, commitCount = 1): Contributor {
    return { name, email: `${name.toLowerCase()}@example.com`, commitCount };
  }

  it('lists one user chip per contributor with their commit count, up to the visible cap', () => {
    const { fixture, http } = setup();
    fixture.detectChanges();
    http.expectOne('/api/repositories/by-id/repo-1/contributors/main').flush([contributor('Alice', 42), contributor('Bob', 1), contributor('Chloé', 7)]);
    fixture.detectChanges();

    const items: HTMLElement[] = Array.from(fixture.nativeElement.querySelectorAll('.contributor-avatars li'));
    expect(items.length).toBe(3);
    expect(items[0].querySelector('gbt-user-chip')?.textContent).toContain('Alice');
    expect(items[0].querySelector('.contributor-avatars__count')?.textContent?.trim()).toBe('42 commits');
    expect(items[1].querySelector('.contributor-avatars__count')?.textContent?.trim()).toBe('1 commit');
    expect(fixture.nativeElement.querySelector('.contributor-avatars__more')).toBeNull();
  });

  it('caps visible contributors at 8 and shows a +N indicator for the rest', () => {
    const { fixture, http } = setup();
    fixture.detectChanges();
    const names = Array.from({ length: 10 }, (_, i) => `Contributor${i}`);
    http.expectOne('/api/repositories/by-id/repo-1/contributors/main').flush(names.map((name) => contributor(name)));
    fixture.detectChanges();

    const chips = fixture.nativeElement.querySelectorAll('gbt-user-chip');
    expect(chips.length).toBe(8);
    expect(fixture.nativeElement.querySelector('.contributor-avatars__more').textContent).toContain('+2');
  });

  it('shows skeleton rows while the contributors load', () => {
    const { fixture, http } = setup();
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelector('gbt-skeleton')).toBeTruthy();
    http.expectOne('/api/repositories/by-id/repo-1/contributors/main').flush([contributor('Alice')]);
    fixture.detectChanges();
    expect(fixture.nativeElement.querySelector('gbt-skeleton')).toBeNull();
  });

  it('renders no list but a short muted message when there are no contributors', () => {
    const { fixture, http } = setup();
    fixture.detectChanges();
    http.expectOne('/api/repositories/by-id/repo-1/contributors/main').flush([]);
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelector('.contributor-avatars')).toBeNull();
    expect(fixture.nativeElement.textContent).toContain('Aucun contributeur');
  });

  it('treats a failed request like an empty list instead of loading forever', () => {
    const { fixture, http } = setup();
    fixture.detectChanges();
    http.expectOne('/api/repositories/by-id/repo-1/contributors/main').flush('boom', { status: 500, statusText: 'Internal Server Error' });
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelector('gbt-skeleton')).toBeNull();
    expect(fixture.nativeElement.textContent).toContain('Aucun contributeur');
  });
});
