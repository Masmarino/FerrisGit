import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { By } from '@angular/platform-browser';
import { ActivatedRoute, Router, convertToParamMap, provideRouter } from '@angular/router';
import { RouterTestingHarness } from '@angular/router/testing';
import { AuthActivate } from '@masmarino/gabarit';
import { ActivatePage } from './activate-page';
import { provideFerrisgitAuth } from '../auth-kit';

@Component({ standalone: true, template: '<p>page</p>' })
class Elsewhere {}

const ACTIVATE = '/api/auth/activate';
const TOKEN = 'ab12'.repeat(16);
const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();
const settle = () => new Promise<void>((resolve) => setTimeout(resolve, 10));

const field = (el: HTMLElement, label: string) => {
  const labelEl = Array.from(el.querySelectorAll('label')).find((l) => text(l) === label);
  return labelEl ? el.querySelector<HTMLInputElement>(`[id="${labelEl.getAttribute('for')}"]`) : null;
};
const type = (input: HTMLInputElement, value: string) => {
  input.value = value;
  input.dispatchEvent(new Event('input'));
};
const button = (el: HTMLElement, label: string) => Array.from(el.querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === label);

describe('ActivatePage', () => {
  async function setup(token: string | null = TOKEN, where: 'fragment' | 'query' = 'fragment', extra = '') {
    const fragment = where === 'fragment' && token !== null ? `token=${token}${extra}` : null;
    const query: Record<string, string> = where === 'query' && token !== null ? { token } : {};
    TestBed.configureTestingModule({
      providers: [
        provideHttpClient(),
        provideHttpClientTesting(),
        provideRouter([]),
        provideFerrisgitAuth(),
        { provide: ActivatedRoute, useValue: { snapshot: { fragment, queryParamMap: convertToParamMap(query) } } },
      ],
    });
    const http = TestBed.inject(HttpTestingController);
    const router = TestBed.inject(Router);
    // The page wipes the token from the URL; spy on it rather than run it against the empty test router.
    const navigate = vi.spyOn(router, 'navigateByUrl').mockResolvedValue(true);
    const fixture = TestBed.createComponent(ActivatePage);
    fixture.detectChanges();
    await fixture.whenStable();
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;
    const refresh = async () => {
      fixture.detectChanges();
      await fixture.whenStable();
      await settle();
      fixture.detectChanges();
    };
    return { fixture, http, router, navigate, el, refresh };
  }
  type Ctx = Awaited<ReturnType<typeof setup>>;

  async function submit(ctx: Ctx, password = 'a-long-password') {
    type(field(ctx.el, 'Nouveau mot de passe')!, password);
    type(field(ctx.el, 'Confirmez le mot de passe')!, password);
    await ctx.refresh();
    ctx.el.querySelector('form')!.dispatchEvent(new Event('submit'));
  }

  describe('the link', () => {
    it('reads the token from the fragment and posts it with the new password', async () => {
      const ctx = await setup();

      await submit(ctx);

      expect(ctx.http.expectOne(ACTIVATE).request.body).toEqual({ token: TOKEN, password: 'a-long-password' });
    });

    it('is tolerant of other parameters in the fragment', async () => {
      const ctx = await setup(TOKEN, 'fragment', '&utm=mail');

      await submit(ctx);

      expect(ctx.http.expectOne(ACTIVATE).request.body).toEqual({ token: TOKEN, password: 'a-long-password' });
    });

    it('still reads `?token=`, for the links of mails sent before the link moved to the fragment', async () => {
      const ctx = await setup(TOKEN, 'query');

      await submit(ctx);

      expect(ctx.http.expectOne(ACTIVATE).request.body).toEqual({ token: TOKEN, password: 'a-long-password' });
    });

    it.each([['fragment'], ['query']] as const)('takes the token out of the URL (replacing the history entry) once read from the %s, but keeps it for the submit', async (where) => {
      const ctx = await setup(TOKEN, where);

      expect(ctx.navigate).toHaveBeenCalledExactlyOnceWith('/activate', { replaceUrl: true });
      await submit(ctx);
      expect(ctx.http.expectOne(ACTIVATE).request.body.token).toBe(TOKEN);
    });

    it('has nothing to wipe when the URL carries no parameter', async () => {
      const { navigate } = await setup(null);

      expect(navigate).not.toHaveBeenCalled();
    });

    it('wipes a malformed token from the URL too, and treats the link as dead without calling the API', async () => {
      const { navigate, http, el } = await setup('not-a-token');

      expect(navigate).toHaveBeenCalledWith('/activate', { replaceUrl: true });
      http.expectNone(ACTIVATE);
      expect(text(el.querySelector('h1'))).toBe('Ce lien ne fonctionne pas');
    });

    it('never renders the token', async () => {
      const { el } = await setup();

      expect(el.innerHTML).not.toContain(TOKEN);
      expect(Array.from(el.querySelectorAll('input')).map((input) => input.value)).not.toContain(TOKEN);
    });
  });

  describe('the page', () => {
    it('is the kit activation form, in French, with the FerrisGit logo and the link to the sign-in', async () => {
      const { el, fixture } = await setup();

      expect(text(el.querySelector('h1'))).toBe('Activez votre compte');
      expect(field(el, 'Nouveau mot de passe')?.getAttribute('autocomplete')).toBe('new-password');
      expect(field(el, 'Confirmez le mot de passe')?.getAttribute('autocomplete')).toBe('new-password');
      expect(button(el, 'Activer mon compte')).toBeTruthy();
      expect(el.querySelector('.gbt-auth-panel__logo > picture img')?.getAttribute('alt')).toBe('FerrisGit');
      expect(getComputedStyle(el).getPropertyValue('--gbt-auth-panel-logo-offset').trim()).toBe('-0.5rem');
      expect(text(el.querySelector('gbt-auth-footer span'))).toBe('Votre compte est déjà actif ?');
      const link = el.querySelector<HTMLAnchorElement>('gbt-auth-footer a')!;
      expect(text(link)).toBe('Se connecter');
      expect(link.getAttribute('href')).toBe('/login');
      expect(fixture.debugElement.query(By.directive(AuthActivate))).toBeTruthy();
    });

    it('stays on the success view once the account is activated: no session, and "Se connecter" goes to the sign-in', async () => {
      const ctx = await setup();
      ctx.navigate.mockClear();
      await submit(ctx);
      ctx.http.expectOne(ACTIVATE).flush(null, { status: 204, statusText: 'No Content' });
      await ctx.refresh();

      expect(text(ctx.el.querySelector('h1'))).toBe('Votre compte est activé');
      expect(ctx.navigate).not.toHaveBeenCalled();
      expect(localStorage.getItem('ferrisgit_token')).toBeNull();

      button(ctx.el, 'Se connecter')!.click();
      await ctx.refresh();
      expect(ctx.navigate).toHaveBeenCalledExactlyOnceWith('/login');
    });

    it('sends "Se connecter" of a dead link to the sign-in', async () => {
      const ctx = await setup(null);
      expect(text(ctx.el.querySelector('h1'))).toBe('Ce lien ne fonctionne pas');

      button(ctx.el, 'Se connecter')!.click();
      await ctx.refresh();

      expect(ctx.navigate).toHaveBeenCalledExactlyOnceWith('/login');
    });
  });

  describe('with the real router', () => {
    async function open(url: string) {
      TestBed.configureTestingModule({
        providers: [
          provideHttpClient(),
          provideHttpClientTesting(),
          provideRouter([
            { path: 'activate', component: ActivatePage },
            { path: 'login', component: Elsewhere },
          ]),
          provideFerrisgitAuth(),
        ],
      });
      const harness = await RouterTestingHarness.create(url);
      const refresh = async () => {
        harness.detectChanges();
        await harness.fixture.whenStable();
        await settle();
        harness.detectChanges();
      };
      await refresh();
      return { harness, refresh, router: TestBed.inject(Router), http: TestBed.inject(HttpTestingController), el: () => harness.routeNativeElement as HTMLElement };
    }

    it('scrubs the address bar and keeps the form: the token read before the scrub is the one sent', async () => {
      const { harness, refresh, router, http, el } = await open(`/activate#token=${TOKEN}`);
      const page = harness.routeDebugElement!.componentInstance;

      expect(router.url).toBe('/activate');
      expect(harness.routeDebugElement!.componentInstance).toBe(page);
      expect(text(el().querySelector('h1'))).toBe('Activez votre compte');

      type(field(el(), 'Nouveau mot de passe')!, 'a-long-password');
      type(field(el(), 'Confirmez le mot de passe')!, 'a-long-password');
      await refresh();
      el().querySelector('form')!.dispatchEvent(new Event('submit'));
      const request = http.expectOne(ACTIVATE);
      expect(request.request.body).toEqual({ token: TOKEN, password: 'a-long-password' });
      request.flush(null, { status: 204, statusText: 'No Content' });
      await refresh();

      expect(text(el().querySelector('h1'))).toBe('Votre compte est activé');
      expect(router.url).toBe('/activate');
    });
  });
});
