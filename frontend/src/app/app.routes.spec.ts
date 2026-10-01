import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting } from '@angular/common/http/testing';
import { ActivatedRouteSnapshot, provideRouter, Router } from '@angular/router';
import { of } from 'rxjs';
import { routes } from './app.routes';
import { PublicConfigService } from './public/public-config.service';
import { PublicLayout } from './public/public-layout/public-layout';
import { ExplorePage } from './public/explore-page/explore-page';
import { PublicRepositoryPage } from './public/public-repository-page/public-repository-page';
import { PublicNotFound } from './public/public-not-found/public-not-found';
import { AppShell } from './shell/app-shell';
import { HomePage } from './home/home-page/home-page';
import { LoginPage } from './auth/login-page/login-page';
import { WorkspacePage } from './repositories/workspace-page/workspace-page';
import { RepositoryPathResolver } from './repositories/repository-path-resolver/repository-path-resolver';

const TOKEN_KEY = 'ferrisgit_token';

// No outlet: the router recognises, guards and loads the components without rendering them.
async function open(url: string, options: { signedIn?: boolean; publicPagesEnabled?: boolean } = {}) {
  if (options.signedIn) {
    localStorage.setItem(TOKEN_KEY, 'a-token');
  }
  TestBed.configureTestingModule({
    providers: [
      provideHttpClient(),
      provideHttpClientTesting(),
      provideRouter(routes),
      { provide: PublicConfigService, useValue: { publicPagesEnabled: () => of(options.publicPagesEnabled ?? true) } },
    ],
  });
  const router = TestBed.inject(Router);
  await router.navigateByUrl(url);
  const chain: ActivatedRouteSnapshot[] = [];
  for (let route: ActivatedRouteSnapshot | null = router.routerState.snapshot.root.firstChild; route; route = route.firstChild) {
    chain.push(route);
  }
  return { url: router.url, components: chain.map((route) => route.component).filter((component) => component !== null) };
}

describe('routes', () => {
  afterEach(() => localStorage.removeItem(TOKEN_KEY));

  describe('without a session', () => {
    it('shows the catalog at /, in the public layout', async () => {
      expect(await open('/')).toEqual({ url: '/', components: [PublicLayout, ExplorePage] });
    });

    it('sends / to the sign-in page when the public pages are off', async () => {
      expect(await open('/', { publicPagesEnabled: false })).toEqual({ url: '/login', components: [LoginPage] });
    });

    it('shows the catalog at /explore, with its query', async () => {
      expect(await open('/explore?q=rust&page=2')).toEqual({ url: '/explore?q=rust&page=2', components: [PublicLayout, ExplorePage] });
    });

    it.each(['/repositories/alice/hello', '/repositories/acme/tools/cli/-/blob/main/README.md', '/repositories/alice/hello/-/settings'])('opens %s as a public repository page', async (url) => {
      expect(await open(url)).toEqual({ url, components: [PublicLayout, PublicRepositoryPage] });
    });

    it.each(['/repositories', '/home', '/account', '/admin/settings'])('keeps sending the signed-in page %s to the sign-in page', async (url) => {
      expect(await open(url)).toEqual({ url: '/login', components: [LoginPage] });
    });

    it('answers an unknown URL with the public "not found" page', async () => {
      expect(await open('/does/not/exist')).toEqual({ url: '/does/not/exist', components: [PublicLayout, PublicNotFound] });
    });
  });

  describe('with a session', () => {
    it('keeps / going to the dashboard', async () => {
      expect(await open('/', { signedIn: true })).toEqual({ url: '/home', components: [AppShell, HomePage] });
    });

    it('opens the workspace in the shell, as before', async () => {
      expect(await open('/repositories', { signedIn: true })).toEqual({ url: '/repositories', components: [AppShell, WorkspacePage] });
    });

    it('opens a repository in the shell, as before', async () => {
      const url = '/repositories/alice/hello/-/releases';
      expect(await open(url, { signedIn: true })).toEqual({ url, components: [AppShell, RepositoryPathResolver] });
    });

    it('can still open the catalog at /explore, in the public layout', async () => {
      expect(await open('/explore', { signedIn: true })).toEqual({ url: '/explore', components: [PublicLayout, ExplorePage] });
    });
  });
});
