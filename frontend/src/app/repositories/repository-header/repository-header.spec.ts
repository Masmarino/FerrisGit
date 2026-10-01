import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { provideRouter, Router } from '@angular/router';
import { CLONE_PANEL_ID, RepositoryHeader } from './repository-header';
import { Repository } from '../repositories.service';
import { READ_ONLY_REPOSITORY } from '../read-only-repository';

describe('RepositoryHeader', () => {
  const repo: Repository = {
    id: 'repo-1',
    name: 'hello',
    description: 'A test repo',
    owner: 'alice',
    role: 'owner',
    visibility: 'public',
    createdAt: '2026-01-01T00:00:00Z',
    path: ['alice', 'hello'],
    starCount: 3,
    isStarred: false,
    sizeBytes: 2048,
  };

  function setup(r: Repository | null, ref = 'main') {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([])] });
    const fixture = TestBed.createComponent(RepositoryHeader);
    fixture.componentRef.setInput('repo', r);
    fixture.componentRef.setInput('ref', ref);
    return { fixture, http: TestBed.inject(HttpTestingController) };
  }

  function flushChildRequests(http: HttpTestingController) {
    http.match(() => true).forEach((req) => req.flush([]));
  }

  function render(r: Repository | null = repo, ref = 'main') {
    const ctx = setup(r, ref);
    ctx.fixture.detectChanges();
    flushChildRequests(ctx.http);
    ctx.fixture.detectChanges();
    return ctx;
  }

  function starButton(root: HTMLElement): HTMLButtonElement {
    return root.querySelector('.repository-header__star button') as HTMLButtonElement;
  }

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
    document.getElementById(CLONE_PANEL_ID)?.remove();
  });

  it('leaves the star button out of a read-only (public) view, but keeps the ref switcher and Cloner', () => {
    TestBed.configureTestingModule({ providers: [{ provide: READ_ONLY_REPOSITORY, useValue: true }] });
    const { fixture } = render();

    expect(starButton(fixture.nativeElement)).toBeNull();
    expect(fixture.nativeElement.querySelector('fg-branch-switcher')).not.toBeNull();
    expect(fixture.nativeElement.querySelector('.repository-header__clone')).not.toBeNull();
  });

  it('shows the repository name as the page h1 with its visibility badge', () => {
    const { fixture } = render();

    const h1 = fixture.nativeElement.querySelectorAll('h1');
    expect(h1.length).toBe(1);
    expect(h1[0].textContent.trim()).toBe('hello');
    const badge = fixture.nativeElement.querySelector('gbt-badge');
    expect(badge?.textContent?.trim()).toBe('Public');
  });

  it('shows the visibility as a badge with the info variant for a public repository', () => {
    const { fixture } = render();

    const badge = fixture.nativeElement.querySelector('gbt-badge');
    expect(badge?.textContent?.trim()).toBe('Public');
    expect(badge?.querySelector('.gbt-badge')?.getAttribute('data-variant')).toBe('info');
  });

  it('shows a neutral "Privé" badge for a private repository', () => {
    const { fixture } = render({ ...repo, visibility: 'private' });

    const badge = fixture.nativeElement.querySelector('gbt-badge');
    expect(badge?.textContent?.trim()).toBe('Privé');
    expect(badge?.querySelector('.gbt-badge')?.getAttribute('data-variant')).toBe('neutral');
  });

  it('no longer carries the description, owner/created/size list or clone URL (they live in the overview aside)', () => {
    const { fixture } = render();

    const text = fixture.nativeElement.textContent as string;
    expect(text).not.toContain('A test repo');
    expect(text).not.toContain('2 Ko');
    expect(text).not.toContain(`${location.origin}/alice/hello.git`);
    expect(fixture.nativeElement.querySelector('gbt-copy-field')).toBeNull();
    expect(fixture.nativeElement.querySelector('gbt-description-list')).toBeNull();
  });

  it('renders no header but a placeholder when repo is null (still loading)', () => {
    const { fixture } = setup(null);
    fixture.detectChanges();

    expect(fixture.nativeElement.querySelector('.repository-header')).toBeNull();
    expect(fixture.nativeElement.querySelector('h1')).toBeNull();
    expect(fixture.nativeElement.querySelector('gbt-skeleton')).toBeTruthy();
  });

  it('renders the star toggle as a secondary small gbt-button showing the count', () => {
    const { fixture } = render();

    const star = fixture.nativeElement.querySelector('gbt-button.repository-header__star');
    expect(star).toBeTruthy();
    const inner = starButton(fixture.nativeElement);
    expect(inner.classList).toContain('gbt-button--secondary');
    expect(inner.classList).toContain('gbt-button--small');
    expect(inner.textContent).toContain('3');
    expect(inner.querySelector('gbt-icon')).toBeTruthy();
  });

  it('shows the star count and toggles starred state optimistically on click', () => {
    const { fixture, http } = render();

    const button = starButton(fixture.nativeElement);
    expect(button.textContent).toContain('3');

    button.click();
    fixture.detectChanges();

    expect(button.textContent).toContain('4');
    http.expectOne({ url: '/api/repositories/by-id/repo-1/star', method: 'POST' }).flush({ starCount: 4, isStarred: true });
  });

  it('gives the star button a constant accessible name (with the count) and a pressed state that reflects whether it is starred', () => {
    const { fixture, http } = render();

    const button = starButton(fixture.nativeElement);
    expect(button.getAttribute('aria-pressed')).toBe('false');
    expect(button.getAttribute('aria-label')).toBe('Favoris (3)');

    button.click();
    fixture.detectChanges();
    http.expectOne({ url: '/api/repositories/by-id/repo-1/star', method: 'POST' }).flush({ starCount: 4, isStarred: true });
    fixture.detectChanges();

    expect(button.getAttribute('aria-pressed')).toBe('true');
    expect(button.getAttribute('aria-label')).toBe('Favoris (4)');
  });

  it('unstars an already-starred repository with a DELETE and marks the button as not pressed', () => {
    const { fixture, http } = render({ ...repo, starCount: 5, isStarred: true });

    const button = starButton(fixture.nativeElement);
    expect(button.getAttribute('aria-pressed')).toBe('true');

    button.click();
    fixture.detectChanges();
    expect(button.textContent).toContain('4');
    expect(button.getAttribute('aria-pressed')).toBe('false');
    http.expectOne({ url: '/api/repositories/by-id/repo-1/star', method: 'DELETE' }).flush({ starCount: 4, isStarred: false });
  });

  it('reverts the optimistic star update if the request fails', () => {
    const { fixture, http } = render();

    const button = starButton(fixture.nativeElement);
    button.click();
    fixture.detectChanges();
    http.expectOne({ url: '/api/repositories/by-id/repo-1/star', method: 'POST' }).flush('server error', { status: 500, statusText: 'Internal Server Error' });
    fixture.detectChanges();

    expect(button.textContent).toContain('3');
    expect(button.getAttribute('aria-pressed')).toBe('false');
  });

  it('emits every star state change (optimistic, confirmed, reverted) so the page can mirror the count', () => {
    const { fixture, http } = render();
    const changes: { starCount: number; isStarred: boolean }[] = [];
    fixture.componentInstance.starChange.subscribe((change) => changes.push(change));

    starButton(fixture.nativeElement).click();
    http.expectOne({ url: '/api/repositories/by-id/repo-1/star', method: 'POST' }).flush('boom', { status: 500, statusText: 'Internal Server Error' });

    expect(changes).toEqual([
      { starCount: 4, isStarred: true },
      { starCount: 3, isStarred: false },
    ]);
  });

  it('puts the branch/tag switcher in the header actions', () => {
    const { fixture } = render();

    expect(fixture.nativeElement.querySelector('.gbt-page-header__actions fg-branch-switcher')).toBeTruthy();
  });

  it('brings the Cloner panel into view and focuses its copy button when the page has one', () => {
    const { fixture } = render();
    const panel = document.createElement('div');
    panel.id = CLONE_PANEL_ID;
    panel.innerHTML = '<button type="button">Copier</button>';
    const scrollIntoView = vi.fn();
    panel.scrollIntoView = scrollIntoView;
    document.body.appendChild(panel);
    const navigate = vi.spyOn(TestBed.inject(Router), 'navigate');

    const cloner: HTMLButtonElement = fixture.nativeElement.querySelector('.repository-header__clone button');
    expect(cloner.textContent).toContain('Cloner');
    cloner.click();

    expect(scrollIntoView).toHaveBeenCalled();
    expect(document.activeElement).toBe(panel.querySelector('button'));
    expect(navigate).not.toHaveBeenCalled();
  });

  // The clone URL does not depend on the ref, and the ref may be what does not exist (a not-found page has
  // no Cloner panel). So it always goes to the repository root's overview.
  it('goes to the repository root overview to reach the Cloner panel from a page without one, whatever the ref', () => {
    const { fixture } = render(repo, 'develop');
    const navigate = vi.spyOn(TestBed.inject(Router), 'navigate').mockResolvedValue(true);

    (fixture.nativeElement.querySelector('.repository-header__clone button') as HTMLButtonElement).click();

    expect(navigate).toHaveBeenCalledWith(['/repositories', 'alice', 'hello'], { fragment: CLONE_PANEL_ID });
  });

  it('leaves a not-found ref for the repository root overview when Cloner is used there', () => {
    const { fixture } = render(repo, 'n-existe-pas');
    const navigate = vi.spyOn(TestBed.inject(Router), 'navigate').mockResolvedValue(true);

    (fixture.nativeElement.querySelector('.repository-header__clone button') as HTMLButtonElement).click();

    expect(navigate).toHaveBeenCalledTimes(1);
    const [commands] = navigate.mock.calls[0];
    expect(commands).toEqual(['/repositories', 'alice', 'hello']);
    expect(commands).not.toContain('n-existe-pas');
  });

  it('goes to the repository root when the ref is the default HEAD', () => {
    const { fixture } = render(repo, 'HEAD');
    const navigate = vi.spyOn(TestBed.inject(Router), 'navigate').mockResolvedValue(true);

    (fixture.nativeElement.querySelector('.repository-header__clone button') as HTMLButtonElement).click();

    expect(navigate).toHaveBeenCalledWith(['/repositories', 'alice', 'hello'], { fragment: CLONE_PANEL_ID });
  });
});
