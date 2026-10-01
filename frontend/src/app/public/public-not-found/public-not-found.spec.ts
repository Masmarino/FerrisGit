import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter } from '@angular/router';
import { RouterTestingHarness } from '@angular/router/testing';
import { PublicNotFound } from './public-not-found';
import { PageTitleService } from '../../shell/page-title.service';

@Component({ standalone: true, template: '<fg-public-not-found />', imports: [PublicNotFound] })
class Host {}

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();

describe('PublicNotFound', () => {
  async function setup(url: string) {
    const pageTitle = { set: vi.fn() };
    TestBed.configureTestingModule({ providers: [provideRouter([{ path: '**', component: Host }]), { provide: PageTitleService, useValue: pageTitle }] });
    const harness = await RouterTestingHarness.create(url);
    return { el: harness.routeNativeElement as HTMLElement, pageTitle };
  }

  it('says the repository does not exist or is not public, without telling which', async () => {
    const { el, pageTitle } = await setup('/repositories/acme/secret');

    expect(text(el.querySelector('h1'))).toBe("Ce dépôt n'existe pas ou n'est pas public");
    expect(text(el.querySelector('gbt-empty-state'))).toContain('Si vous avez accès à ce dépôt, connectez-vous pour le voir.');
    expect(pageTitle.set).toHaveBeenCalledWith('Introuvable');
  });

  it('offers to sign in and come back here, or to browse the public repositories', async () => {
    const { el } = await setup('/repositories/acme/secret/-/releases');

    const links = Array.from(el.querySelectorAll<HTMLAnchorElement>('.public-not-found__actions a'));
    expect(links.map((a) => [text(a), a.getAttribute('href')])).toEqual([
      ['Se connecter', '/login?returnUrl=%2Frepositories%2Facme%2Fsecret%2F-%2Freleases'],
      ['Explorer les dépôts publics', '/explore'],
    ]);
  });
});
