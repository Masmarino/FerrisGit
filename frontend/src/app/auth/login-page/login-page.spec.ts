import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideHttpClient, withInterceptors } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { By } from '@angular/platform-browser';
import { Router, provideRouter } from '@angular/router';
import { RouterTestingHarness } from '@angular/router/testing';
import { AuthLogin } from '@masmarino/gabarit';
import { LoginPage } from './login-page';
import { authInterceptor } from '../auth.interceptor';
import { provideFerrisgitAuth } from '../auth-kit';
import { REQUEST_OPTIONS, fakeAssertion, stubPasskeyBrowser } from '../../shared/webauthn-testing';

// Enrolment draws its QR code through our renderer, which lazily imports `qrcode`; jsdom has no canvas.
const toDataURL = vi.hoisted(() => vi.fn());
vi.mock('qrcode', () => ({ toDataURL }));

@Component({ standalone: true, template: '<p>page</p>' })
class Elsewhere {}

const TOKEN_KEY = 'ferrisgit_token';
const LOGIN = '/api/auth/login';
const CONFIG = '/api/auth/config';
const VERIFY = '/api/auth/mfa/verify';
const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();
const settle = () => new Promise<void>((resolve) => setTimeout(resolve, 10));

describe('LoginPage', () => {
  async function setup(options: { interceptor?: boolean; url?: string } = {}) {
    TestBed.configureTestingModule({
      providers: [
        provideHttpClient(...(options.interceptor ? [withInterceptors([authInterceptor])] : [])),
        provideHttpClientTesting(),
        provideRouter([
          { path: 'login', component: LoginPage },
          { path: 'register', component: Elsewhere },
          { path: 'repositories', component: Elsewhere },
          { path: 'repositories/**', component: Elsewhere },
        ]),
        provideFerrisgitAuth(),
      ],
    });
    const harness = await RouterTestingHarness.create(options.url ?? '/login');
    const http = TestBed.inject(HttpTestingController);
    const router = TestBed.inject(Router);
    const el = () => harness.routeNativeElement as HTMLElement;
    const kit = () => harness.routeDebugElement!.query(By.directive(AuthLogin)).componentInstance as AuthLogin;
    const refresh = async () => {
      harness.detectChanges();
      await harness.fixture.whenStable();
      await settle();
      harness.detectChanges();
    };
    return { harness, http, router, el, kit, refresh };
  }
  type Ctx = Awaited<ReturnType<typeof setup>>;

  const field = (el: HTMLElement, label: string) => {
    const labelEl = Array.from(el.querySelectorAll('label')).find((l) => text(l) === label);
    return labelEl ? el.querySelector<HTMLInputElement>(`[id="${labelEl.getAttribute('for')}"]`) : null;
  };
  const type = (input: HTMLInputElement, value: string) => {
    input.value = value;
    input.dispatchEvent(new Event('input'));
  };
  const button = (el: HTMLElement, label: string) => Array.from(el.querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === label);
  const alertText = (el: HTMLElement) => text(el.querySelector('gbt-alert [role="alert"]'));

  async function signIn(ctx: Ctx, username = 'admin', password = 'secret') {
    ctx.http.expectOne(CONFIG).flush({ registrationEnabled: false, passkeysAvailable: false });
    await ctx.refresh();
    type(field(ctx.el(), "Nom d'utilisateur")!, username);
    type(field(ctx.el(), 'Mot de passe')!, password);
    ctx.el().querySelector('form')!.dispatchEvent(new Event('submit'));
    return ctx.http.expectOne(LOGIN);
  }

  let previousToken: string | null;
  beforeEach(() => {
    previousToken = localStorage.getItem(TOKEN_KEY);
    localStorage.removeItem(TOKEN_KEY);
    toDataURL.mockReset();
    toDataURL.mockResolvedValue('data:image/png;base64,QR');
  });
  afterEach(() => {
    if (previousToken === null) {
      localStorage.removeItem(TOKEN_KEY);
    } else {
      localStorage.setItem(TOKEN_KEY, previousToken);
    }
  });

  describe('the page', () => {
    it('is the kit sign-in page, in French', async () => {
      const ctx = await setup();
      ctx.http.expectOne(CONFIG).flush({ registrationEnabled: false, passkeysAvailable: false });
      await ctx.refresh();

      expect(ctx.el().querySelector('gbt-auth-login')).toBeTruthy();
      expect(text(ctx.el().querySelector('h1'))).toBe('Connexion');
      expect(text(ctx.el().querySelector('.gbt-auth-panel__intro'))).toBe('Connectez-vous pour retrouver vos dépôts, tickets et demandes de fusion.');
      expect(field(ctx.el(), "Nom d'utilisateur")?.getAttribute('autocomplete')).toBe('username');
      expect(field(ctx.el(), 'Mot de passe')?.getAttribute('autocomplete')).toBe('current-password');
      expect(button(ctx.el(), 'Se connecter')).toBeTruthy();
    });

    it('shows the FerrisGit logo in the panel, with its light variant on the dark theme', async () => {
      const ctx = await setup();

      const picture = ctx.el().querySelector('.gbt-auth-panel__logo > picture')!;
      expect(picture).toBeTruthy();
      const source = picture.querySelector('source')!;
      expect(source.getAttribute('srcset')).toBe('Logo_horizontal_dark.png');
      expect(source.getAttribute('media')).toBe('(prefers-color-scheme: dark)');
      const img = picture.querySelector('img')!;
      expect(img.getAttribute('src')).toBe('Logo_horizontal.png');
      expect(img.getAttribute('alt')).toBe('FerrisGit');
      expect([img.getAttribute('width'), img.getAttribute('height')]).toEqual(['1762', '592']);
    });

    it("pulls the logo up by the artwork's own margin (-0.5rem), as the page always did", async () => {
      const ctx = await setup();

      expect(getComputedStyle(ctx.el()).getPropertyValue('--gbt-auth-panel-logo-offset').trim()).toBe('-0.5rem');
    });
  });

  describe('the link to the registration page', () => {
    const link = (el: HTMLElement) => el.querySelector<HTMLAnchorElement>('gbt-auth-footer a');

    it('offers "Créer un compte", to /register, when registration is open', async () => {
      const ctx = await setup();
      ctx.http.expectOne(CONFIG).flush({ registrationEnabled: true, passkeysAvailable: false });
      await ctx.refresh();

      expect(text(ctx.el().querySelector('gbt-auth-footer span'))).toBe('Pas encore de compte ?');
      expect(text(link(ctx.el()))).toBe('Créer un compte');
      expect(link(ctx.el())?.getAttribute('href')).toBe('/register');
      expect(link(ctx.el())?.classList).toContain('gbt-button--link');

      link(ctx.el())!.click();
      await ctx.refresh();
      expect(ctx.router.url).toBe('/register');
    });

    it('is not there when registration is closed, or before the instance answered', async () => {
      const ctx = await setup();
      expect(link(ctx.el())).toBeNull();

      ctx.http.expectOne(CONFIG).flush({ registrationEnabled: false, passkeysAvailable: false });
      await ctx.refresh();
      expect(link(ctx.el())).toBeNull();
    });
  });

  describe('the link to the documentation', () => {
    const link = (el: HTMLElement) => el.querySelector<HTMLAnchorElement>('footer a');

    it.each([true, false])('is offered below the form, registration open or not (%s)', async (registrationEnabled) => {
      const ctx = await setup();
      ctx.http.expectOne(CONFIG).flush({ registrationEnabled, passkeysAvailable: false });
      await ctx.refresh();

      expect(text(link(ctx.el()))).toBe('Documentation');
      expect(link(ctx.el())?.getAttribute('href')).toBe('/docs');
      expect(link(ctx.el())?.closest('main')).toBeNull();
    });
  });

  describe('where a signed-in user goes', () => {
    it('posts the credentials, stores the session and goes to the repositories', async () => {
      const ctx = await setup();

      const request = await signIn(ctx, 'admin', 'secret');
      expect(request.request.body).toEqual({ username: 'admin', password: 'secret' });
      request.flush({ token: 'abc' });
      await ctx.refresh();

      expect(localStorage.getItem(TOKEN_KEY)).toBe('abc');
      expect(ctx.router.url).toBe('/repositories');
    });

    it('goes back to the public page it came from (?returnUrl=)', async () => {
      const ctx = await setup({ url: '/login?returnUrl=%2Frepositories%2Falice%2Fhello%2F-%2Freleases' });

      (await signIn(ctx)).flush({ token: 'abc' });
      await ctx.refresh();

      expect(ctx.router.url).toBe('/repositories/alice/hello/-/releases');
    });

    for (const unsafe of ['https://evil.example/x', '//evil.example/x', '/\\evil.example', 'javascript:alert(1)']) {
      it(`ignores a returnUrl that would leave the site (${unsafe})`, async () => {
        const ctx = await setup({ url: `/login?returnUrl=${encodeURIComponent(unsafe)}` });

        (await signIn(ctx)).flush({ token: 'abc' });
        await ctx.refresh();

        expect(ctx.router.url).toBe('/repositories');
      });
    }

    it('stays, says why in French and stores nothing when the credentials are refused', async () => {
      const ctx = await setup();

      (await signIn(ctx)).flush({ error: 'invalid username or password' }, { status: 401, statusText: 'Unauthorized' });
      await ctx.refresh();

      expect(alertText(ctx.el())).toBe("Nom d'utilisateur ou mot de passe incorrect");
      expect(ctx.router.url).toBe('/login');
      expect(localStorage.getItem(TOKEN_KEY)).toBeNull();
    });

    it('goes to the repositories once the challenge code is accepted, not before', async () => {
      const ctx = await setup();
      (await signIn(ctx)).flush({ token: null, mfaToken: 'pending', mfaSetupRequired: false, mfaHasTotp: true });
      await ctx.refresh();
      expect(text(ctx.el().querySelector('h1'))).toBe('Vérification en deux étapes');
      expect(ctx.router.url).toBe('/login');
      expect(localStorage.getItem(TOKEN_KEY)).toBeNull();

      type(field(ctx.el(), 'Code à 6 chiffres')!, '123 456');
      ctx.el().querySelector('form')!.dispatchEvent(new Event('submit'));
      const request = ctx.http.expectOne(VERIFY);
      expect(request.request.body).toEqual({ mfaToken: 'pending', code: '123456' });
      request.flush({ token: 'session-jwt' });
      await ctx.refresh();

      expect(localStorage.getItem(TOKEN_KEY)).toBe('session-jwt');
      expect(ctx.router.url).toBe('/repositories');
    });

    it('goes to the repositories once the passkey is accepted', async () => {
      const get = vi.fn(() => Promise.resolve(fakeAssertion()));
      onTestFinished(stubPasskeyBrowser({ get, create: vi.fn() }));
      const ctx = await setup();
      ctx.http.expectOne(CONFIG).flush({ registrationEnabled: false, passkeysAvailable: true });
      await ctx.refresh();
      type(field(ctx.el(), "Nom d'utilisateur")!, 'admin');
      type(field(ctx.el(), 'Mot de passe')!, 'secret');
      ctx.el().querySelector('form')!.dispatchEvent(new Event('submit'));
      ctx.http.expectOne(LOGIN).flush({ token: null, mfaToken: 'pending', mfaSetupRequired: false, mfaHasTotp: false, mfaHasPasskey: true });
      await ctx.refresh();

      button(ctx.el(), "Utiliser une clé d'accès")!.click();
      ctx.http.expectOne('/api/auth/mfa/passkey/start').flush({ challengeId: 'c1', publicKey: REQUEST_OPTIONS });
      await ctx.refresh();
      expect(get).toHaveBeenCalledTimes(1);
      const finish = ctx.http.expectOne('/api/auth/mfa/passkey/finish');
      expect(finish.request.body.mfaToken).toBe('pending');
      expect(finish.request.body.challengeId).toBe('c1');
      finish.flush({ token: 'session-jwt' });
      await ctx.refresh();

      expect(localStorage.getItem(TOKEN_KEY)).toBe('session-jwt');
      expect(ctx.router.url).toBe('/repositories');
    });

    it('completes the mandatory enrolment in French: nothing is stored nor opened before the codes are acknowledged', async () => {
      const ctx = await setup();
      (await signIn(ctx)).flush({ token: null, mfaToken: 'pending', mfaSetupRequired: true });
      await ctx.refresh();
      expect(text(ctx.el().querySelector('h1'))).toBe('Double authentification');

      button(ctx.el(), 'Commencer')!.click();
      ctx.http.expectOne('/api/auth/mfa/setup/totp/enroll').flush({ secret: 'JBSWY3DP', otpauthUrl: 'otpauth://totp/FerrisGit:admin?secret=JBSWY3DP' });
      await ctx.refresh();
      expect(toDataURL).toHaveBeenCalledWith('otpauth://totp/FerrisGit:admin?secret=JBSWY3DP', { errorCorrectionLevel: 'M', margin: 1, width: 448 });
      const qr = ctx.el().querySelector<HTMLImageElement>('gbt-totp-qr img')!;
      expect(qr.getAttribute('src')).toBe('data:image/png;base64,QR');
      expect(qr.getAttribute('alt')).toBe("QR code à scanner avec votre application d'authentification");

      type(field(ctx.el(), 'Code à 6 chiffres')!, '123456');
      ctx.el().querySelector('gbt-mfa-enrollment form')!.dispatchEvent(new Event('submit'));
      ctx.http.expectOne('/api/auth/mfa/setup/totp/confirm').flush({ token: 'session-jwt', backupCodes: ['a'.repeat(32)] });
      await ctx.refresh();

      expect(text(ctx.el().querySelector('gbt-backup-codes'))).toContain("J'ai enregistré mes codes de secours");
      expect(button(ctx.el(), 'Continuer')!.disabled).toBe(true);
      expect(localStorage.getItem(TOKEN_KEY)).toBeNull();
      expect(ctx.router.url).toBe('/login');

      ctx.el().querySelector<HTMLInputElement>('gbt-backup-codes input[type="checkbox"]')!.click();
      await ctx.refresh();
      button(ctx.el(), 'Continuer')!.click();
      await ctx.refresh();

      expect(localStorage.getItem(TOKEN_KEY)).toBe('session-jwt');
      expect(ctx.router.url).toBe('/repositories');
    });
  });

  describe('with the auth interceptor (a 401 logs out and goes to /login)', () => {
    it('shows the expired sign-in on the credentials step, on the same page instance', async () => {
      const ctx = await setup({ interceptor: true });
      const page = ctx.harness.routeDebugElement!.componentInstance as LoginPage;
      const kit = ctx.kit();
      (await signIn(ctx)).flush({ token: null, mfaToken: 'pending', mfaSetupRequired: false });
      await ctx.refresh();
      type(field(ctx.el(), 'Code à 6 chiffres')!, '123456');

      kit.verify();
      ctx.http.expectOne(VERIFY).flush({ error: 'invalid or expired token' }, { status: 401, statusText: 'Unauthorized' });
      await ctx.refresh();

      expect(ctx.router.url).toBe('/login');
      expect(ctx.harness.routeDebugElement!.componentInstance).toBe(page);
      expect(ctx.kit()).toBe(kit);
      expect(kit['mfaToken']()).toBeNull();
      expect(alertText(ctx.el())).toBe('Votre connexion a expiré, reconnectez-vous.');
    });

    it('keeps the challenge on screen after a wrong code: the navigation to the same URL is a no-op', async () => {
      const ctx = await setup({ interceptor: true });
      const page = ctx.harness.routeDebugElement!.componentInstance as LoginPage;
      const kit = ctx.kit();
      (await signIn(ctx)).flush({ token: null, mfaToken: 'pending', mfaSetupRequired: false });
      await ctx.refresh();
      type(field(ctx.el(), 'Code à 6 chiffres')!, '000000');

      kit.verify();
      ctx.http.expectOne(VERIFY).flush({}, { status: 401, statusText: 'Unauthorized' });
      await ctx.refresh();

      expect(ctx.router.url).toBe('/login');
      expect(ctx.harness.routeDebugElement!.componentInstance).toBe(page);
      expect(kit['mfaToken']()).toBe('pending');
      expect(alertText(ctx.el())).toBe('Code incorrect');
    });
  });
});
