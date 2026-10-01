import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting } from '@angular/common/http/testing';
import { Title } from '@angular/platform-browser';
import { provideRouter, Router } from '@angular/router';
import { RouterTestingHarness } from '@angular/router/testing';
import { PublicLayout } from './public-layout';
import { PageTitleService } from '../../shell/page-title.service';
import { RepositoryContextService } from '../../repositories/repository-context.service';
import { PublicRepositoryContextService } from '../public-repository-context.service';

@Component({ standalone: true, template: '<p class="page">contenu</p>' })
class Page {}

const TOKEN_KEY = 'ferrisgit_token';
const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();

async function setup(url = '/explore', options: { signedIn?: boolean } = {}) {
  if (options.signedIn) {
    localStorage.setItem(TOKEN_KEY, 'a-token');
  }
  TestBed.configureTestingModule({
    providers: [
      provideHttpClient(),
      provideHttpClientTesting(),
      provideRouter([{ path: '', component: PublicLayout, children: [{ path: '**', component: Page }] }]),
      { provide: RepositoryContextService, useClass: PublicRepositoryContextService },
    ],
  });
  const harness = await RouterTestingHarness.create(url);
  const el = () => harness.fixture.nativeElement as HTMLElement;
  return { harness, el, router: TestBed.inject(Router) };
}

describe('PublicLayout', () => {
  afterEach(() => localStorage.removeItem(TOKEN_KEY));

  it('frames the page: a header above the routed content, no sidebar', async () => {
    const { el } = await setup();

    expect(el().querySelector('header.public-layout__header')).not.toBeNull();
    expect(text(el().querySelector('main .page'))).toBe('contenu');
    expect(el().querySelector('nav')).toBeNull();
  });

  it('shows the logo, linking to the catalog', async () => {
    const { el } = await setup();

    const home = el().querySelector<HTMLAnchorElement>('a.public-layout__home')!;
    expect(home.getAttribute('href')).toBe('/explore');
    expect(home.getAttribute('aria-label')).toContain('FerrisGit');
    expect(home.querySelector('img')?.getAttribute('src')).toBe('Logo_horizontal.png');
  });

  it('offers an anonymous visitor to sign in and come back to this page', async () => {
    const { el } = await setup('/repositories/alice/hello/-/releases');

    const link = el().querySelector<HTMLAnchorElement>('.public-layout__account a')!;
    expect(text(link)).toBe('Se connecter');
    expect(link.getAttribute('href')).toBe('/login?returnUrl=%2Frepositories%2Falice%2Fhello%2F-%2Freleases');
  });

  it('keeps the return address current as the visitor moves on', async () => {
    const { el, harness } = await setup('/explore');

    await harness.navigateByUrl('/repositories/alice/hello');
    harness.fixture.detectChanges();

    expect(el().querySelector('.public-layout__account a')?.getAttribute('href')).toBe('/login?returnUrl=%2Frepositories%2Falice%2Fhello');
  });

  it('offers a signed-in user their repositories instead', async () => {
    const { el } = await setup('/explore', { signedIn: true });

    const link = el().querySelector<HTMLAnchorElement>('.public-layout__account a')!;
    expect(text(link)).toBe('Mes dépôts');
    expect(link.getAttribute('href')).toBe('/repositories');
  });

  describe('quick search', () => {
    it('is a labelled search landmark', async () => {
      const { el } = await setup();

      const form = el().querySelector('form[role="search"]')!;
      expect(form.getAttribute('aria-label')).toBe('Recherche rapide');
      const input = form.querySelector('input')!;
      expect(text(el().querySelector(`label[for="${input.id}"]`))).toBe('Rechercher un dépôt public');
      expect(input.getAttribute('maxlength')).toBe('100');
    });

    it('opens the catalog with the trimmed text', async () => {
      const { el, harness, router } = await setup('/repositories/alice/hello');
      const input = el().querySelector<HTMLInputElement>('form[role="search"] input')!;

      input.value = '  ferris ';
      input.dispatchEvent(new Event('input'));
      el().querySelector('form[role="search"]')!.dispatchEvent(new Event('submit', { cancelable: true }));
      await harness.fixture.whenStable();

      expect(router.url).toBe('/explore?q=ferris');
    });

    it('opens the whole catalog for an empty search', async () => {
      const { el, harness, router } = await setup('/explore?q=old');

      el().querySelector('form[role="search"]')!.dispatchEvent(new Event('submit', { cancelable: true }));
      await harness.fixture.whenStable();

      expect(router.url).toBe('/explore');
    });
  });

  describe('document title', () => {
    it('is "<page> · FerrisGit"', async () => {
      const { harness } = await setup();

      TestBed.inject(PageTitleService).set('Explorer');
      harness.fixture.detectChanges();

      expect(TestBed.inject(Title).getTitle()).toBe('Explorer · FerrisGit');
    });

    it('names the repository too on its pages', async () => {
      const { harness } = await setup();

      TestBed.inject(RepositoryContextService).enter('repo-1', ['alice', 'hello'], [], null);
      TestBed.inject(PageTitleService).set('Releases');
      harness.fixture.detectChanges();
      expect(TestBed.inject(Title).getTitle()).toBe('Releases · alice/hello · FerrisGit');

      TestBed.inject(PageTitleService).set('');
      harness.fixture.detectChanges();
      expect(TestBed.inject(Title).getTitle()).toBe('alice/hello · FerrisGit');
    });
  });
});
