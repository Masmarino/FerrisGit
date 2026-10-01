import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { Router, provideRouter } from '@angular/router';
import { RouterTestingHarness } from '@angular/router/testing';
import { RegisterPage } from './register-page';
import { provideFerrisgitAuth } from '../auth-kit';

// The enrolment draws its QR code with FerrisGit's renderer, which lazily imports `qrcode`. jsdom has no canvas.
vi.mock('qrcode', () => ({ toDataURL: vi.fn().mockResolvedValue('data:image/png;base64,QR') }));

@Component({ standalone: true, template: '<p>page</p>' })
class Elsewhere {}

const TOKEN_KEY = 'ferrisgit_token';
const REGISTER = '/api/auth/register';
const CONFIG = '/api/auth/config';
const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();
const settle = () => new Promise<void>((resolve) => setTimeout(resolve, 10));

describe('RegisterPage', () => {
  async function setup(config: { registrationEnabled: boolean; passkeysAvailable: boolean } = { registrationEnabled: true, passkeysAvailable: false }) {
    TestBed.configureTestingModule({
      providers: [
        provideHttpClient(),
        provideHttpClientTesting(),
        provideRouter([
          { path: 'register', component: RegisterPage },
          { path: 'login', component: Elsewhere },
          { path: 'repositories', component: Elsewhere },
        ]),
        provideFerrisgitAuth(),
      ],
    });
    const harness = await RouterTestingHarness.create('/register');
    const http = TestBed.inject(HttpTestingController);
    const el = () => harness.routeNativeElement as HTMLElement;
    const refresh = async () => {
      harness.detectChanges();
      await harness.fixture.whenStable();
      await settle();
      harness.detectChanges();
    };
    http.expectOne(CONFIG).flush(config);
    await refresh();
    return { harness, http, router: TestBed.inject(Router), el, refresh };
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

  async function register(ctx: Ctx) {
    type(field(ctx.el(), "Nom d'utilisateur")!, 'Alice');
    type(field(ctx.el(), 'Adresse e-mail')!, 'alice@example.com');
    type(field(ctx.el(), 'Mot de passe')!, 'a-long-password');
    ctx.el().querySelector('form')!.dispatchEvent(new Event('submit'));
    const request = ctx.http.expectOne(REGISTER);
    expect(request.request.body).toEqual({ username: 'Alice', email: 'alice@example.com', password: 'a-long-password' });
    return request;
  }

  let previousToken: string | null;
  beforeEach(() => {
    previousToken = localStorage.getItem(TOKEN_KEY);
    localStorage.removeItem(TOKEN_KEY);
  });
  afterEach(() => {
    if (previousToken === null) {
      localStorage.removeItem(TOKEN_KEY);
    } else {
      localStorage.setItem(TOKEN_KEY, previousToken);
    }
  });

  it('is the kit registration form, in French, with the FerrisGit logo', async () => {
    const { el } = await setup();

    expect(el().querySelector('gbt-auth-register')).toBeTruthy();
    expect(text(el().querySelector('h1'))).toBe('Créer un compte');
    expect(text(el().querySelector('.gbt-auth-panel__intro'))).toBe('Rejoignez FerrisGit pour héberger vos dépôts, tickets et demandes de fusion.');
    expect(field(el(), 'Adresse e-mail')?.type).toBe('email');
    expect(button(el(), 'Créer mon compte')).toBeTruthy();
    const img = el().querySelector<HTMLImageElement>('.gbt-auth-panel__logo > picture img')!;
    expect(img.getAttribute('alt')).toBe('FerrisGit');
    expect(el().querySelector('.gbt-auth-panel__logo > picture source')?.getAttribute('srcset')).toBe('Logo_horizontal_dark.png');
    expect(getComputedStyle(el()).getPropertyValue('--gbt-auth-panel-logo-offset').trim()).toBe('-0.5rem');
  });

  it('links to the sign-in page under the form: "Vous avez déjà un compte ? Se connecter"', async () => {
    const { el, router, refresh } = await setup();
    const link = el().querySelector<HTMLAnchorElement>('gbt-auth-footer a')!;

    expect(text(el().querySelector('gbt-auth-footer span'))).toBe('Vous avez déjà un compte ?');
    expect(text(link)).toBe('Se connecter');
    expect(link.getAttribute('href')).toBe('/login');
    link.click();
    await refresh();

    expect(router.url).toBe('/login');
  });

  it('goes to the repositories when the server opened a session right away', async () => {
    const ctx = await setup();

    (await register(ctx)).flush({ token: 'abc' });
    await ctx.refresh();

    expect(localStorage.getItem(TOKEN_KEY)).toBe('abc');
    expect(ctx.router.url).toBe('/repositories');
  });

  it('goes to the repositories only once the enrolment is done and its backup codes acknowledged', async () => {
    const ctx = await setup();
    (await register(ctx)).flush({ token: null, mfaToken: 'pending', mfaSetupRequired: true });
    await ctx.refresh();
    expect(text(ctx.el().querySelector('h1'))).toBe('Double authentification');

    button(ctx.el(), 'Commencer')!.click();
    ctx.http.expectOne('/api/auth/mfa/setup/totp/enroll').flush({ secret: 'JBSWY3DP', otpauthUrl: 'otpauth://totp/FerrisGit:alice?secret=JBSWY3DP' });
    await ctx.refresh();
    type(field(ctx.el(), 'Code à 6 chiffres')!, '123456');
    ctx.el().querySelector('gbt-mfa-enrollment form')!.dispatchEvent(new Event('submit'));
    const confirm = ctx.http.expectOne('/api/auth/mfa/setup/totp/confirm');
    expect(confirm.request.body).toEqual({ mfaToken: 'pending', code: '123456' });
    confirm.flush({ token: 'session-jwt', backupCodes: ['a'.repeat(32)] });
    await ctx.refresh();

    expect(localStorage.getItem(TOKEN_KEY)).toBeNull();
    expect(ctx.router.url).toBe('/register');

    ctx.el().querySelector<HTMLInputElement>('gbt-backup-codes input[type="checkbox"]')!.click();
    await ctx.refresh();
    button(ctx.el(), 'Continuer')!.click();
    await ctx.refresh();

    expect(localStorage.getItem(TOKEN_KEY)).toBe('session-jwt');
    expect(ctx.router.url).toBe('/repositories');
  });

  it('sends "Se connecter" of the closed registration to the sign-in page', async () => {
    const { el, router, refresh } = await setup({ registrationEnabled: false, passkeysAvailable: false });
    expect(text(el().querySelector('h1'))).toBe('Les inscriptions sont fermées');

    button(el(), 'Se connecter')!.click();
    await refresh();

    expect(router.url).toBe('/login');
  });

  it('sends "Se connecter" of the created account (enrolment left) to the sign-in page', async () => {
    const ctx = await setup();
    (await register(ctx)).flush({ token: null, mfaToken: 'pending', mfaSetupRequired: true });
    await ctx.refresh();

    button(ctx.el(), 'Retour')!.click();
    await ctx.refresh();
    expect(text(ctx.el().querySelector('h1'))).toBe('Votre compte est créé');
    expect(text(ctx.el().querySelector('.gbt-auth-register__created-name'))).toBe("Votre nom d'utilisateur : alice");
    expect(ctx.router.url).toBe('/register');

    button(ctx.el(), 'Se connecter')!.click();
    await ctx.refresh();

    expect(ctx.router.url).toBe('/login');
    expect(localStorage.getItem(TOKEN_KEY)).toBeNull();
  });
});
