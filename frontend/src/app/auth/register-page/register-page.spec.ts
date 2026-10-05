import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { Router, provideRouter } from '@angular/router';
import { RouterTestingHarness } from '@angular/router/testing';
import { RegisterPage } from './register-page';

@Component({ standalone: true, template: '<p>page</p>' })
class Elsewhere {}

const TOKEN_KEY = 'ferrisgit_token';
const REGISTER = '/api/auth/register';
const CONFIG = '/api/auth/config';
const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();
const settle = () => new Promise<void>((resolve) => setTimeout(resolve, 10));

describe('RegisterPage', () => {
  async function setup(config: { registrationEnabled: boolean; passkeysAvailable: boolean } | 'fails' = { registrationEnabled: true, passkeysAvailable: false }) {
    TestBed.configureTestingModule({
      providers: [
        provideHttpClient(),
        provideHttpClientTesting(),
        provideRouter([
          { path: 'register', component: RegisterPage },
          { path: 'login', component: Elsewhere },
          { path: 'repositories', component: Elsewhere },
        ]),
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
    if (config === 'fails') {
      http.expectOne(CONFIG).flush(null, { status: 500, statusText: 'Server Error' });
    } else {
      http.expectOne(CONFIG).flush(config);
    }
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
  const submit = (ctx: Ctx) => ctx.el().querySelector('form')!.dispatchEvent(new Event('submit'));
  const button = (el: HTMLElement, label: string) => Array.from(el.querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === label);

  async function fill(ctx: Ctx, username = 'Alice', email = 'alice@example.com') {
    type(field(ctx.el(), "Nom d'utilisateur")!, username);
    type(field(ctx.el(), 'Adresse e-mail')!, email);
    await ctx.refresh();
  }

  async function register(ctx: Ctx) {
    await fill(ctx);
    submit(ctx);
    const request = ctx.http.expectOne(REGISTER);
    expect(request.request.body).toEqual({ username: 'Alice', email: 'alice@example.com' });
    return request;
  }

  afterEach(() => localStorage.clear());

  it('asks for a username and an address, and for no password', async () => {
    const ctx = await setup();

    expect(text(ctx.el().querySelector('h1'))).toBe('Créer un compte');
    expect(field(ctx.el(), "Nom d'utilisateur")).not.toBeNull();
    expect(field(ctx.el(), 'Adresse e-mail')).not.toBeNull();
    expect(ctx.el().querySelector('input[type="password"]')).toBeNull();
    expect(text(ctx.el())).toContain("Nous vous y envoyons un lien pour confirmer l'adresse et choisir votre mot de passe.");
    expect(button(ctx.el(), 'Créer mon compte')).toBeTruthy();
  });

  it('links to the sign-in page', async () => {
    const ctx = await setup();

    const link = Array.from(ctx.el().querySelectorAll('a')).find((a) => text(a) === 'Se connecter');
    expect(link?.getAttribute('href')).toBe('/login');
  });

  it('shows the closed view, without a form, when registration is off', async () => {
    const ctx = await setup({ registrationEnabled: false, passkeysAvailable: false });

    expect(text(ctx.el().querySelector('h1'))).toBe('Les inscriptions sont fermées');
    expect(ctx.el().querySelector('form')).toBeNull();
    await ctx.refresh();
    button(ctx.el(), 'Se connecter')!.click();
    await ctx.refresh();
    expect(ctx.router.url).toBe('/login');
  });

  it('still shows the form when the config cannot be read: the server decides at submit', async () => {
    const ctx = await setup('fails');

    expect(ctx.el().querySelector('form')).not.toBeNull();
  });

  it('checks both fields before sending anything', async () => {
    const ctx = await setup();
    await fill(ctx, 'ab', 'not-an-email');

    submit(ctx);
    await ctx.refresh();

    ctx.http.expectNone(REGISTER);
    expect(text(ctx.el())).toContain('Commencez par une lettre');
    expect(text(ctx.el())).toContain('Saisissez une adresse e-mail valide');
  });

  it('sends the trimmed fields and asks to check the mailbox, without any session', async () => {
    const ctx = await setup();
    await fill(ctx, '  Alice ', ' alice@example.com ');

    submit(ctx);
    const request = ctx.http.expectOne(REGISTER);
    expect(request.request.body).toEqual({ username: 'Alice', email: 'alice@example.com' });
    request.flush(null, { status: 204, statusText: 'No Content' });
    await ctx.refresh();

    expect(text(ctx.el().querySelector('h1'))).toBe('Consultez votre boîte mail');
    expect(text(ctx.el())).toContain('alice@example.com');
    expect(text(ctx.el())).toContain('24 heures');
    expect(ctx.el().querySelector('form')).toBeNull();
    expect(localStorage.getItem(TOKEN_KEY)).toBeNull();
    expect(ctx.router.url).toBe('/register');
  });

  it('tells the user to register again if nothing arrives, and offers the sign-in page', async () => {
    const ctx = await setup();
    (await register(ctx)).flush(null, { status: 204, statusText: 'No Content' });
    await ctx.refresh();

    expect(text(ctx.el())).toContain('Inscrivez-vous de nouveau');
    button(ctx.el(), 'Se connecter')!.click();
    await ctx.refresh();
    expect(ctx.router.url).toBe('/login');
  });

  it('sends one request at a time', async () => {
    const ctx = await setup();
    await fill(ctx);

    submit(ctx);
    submit(ctx);

    ctx.http.expectOne(REGISTER).flush(null, { status: 204, statusText: 'No Content' });
  });

  it('keeps the draft and says so when the name or address is taken', async () => {
    const ctx = await setup();
    (await register(ctx)).flush({ error: 'username already taken' }, { status: 409, statusText: 'Conflict' });
    await ctx.refresh();

    expect(text(ctx.el())).toContain("Ce nom d'utilisateur ou cette adresse e-mail est déjà utilisé");
    expect(field(ctx.el(), "Nom d'utilisateur")?.value).toBe('Alice');
    expect(field(ctx.el(), 'Adresse e-mail')?.value).toBe('alice@example.com');
  });

  it('puts a reserved name on its field', async () => {
    const ctx = await setup();
    (await register(ctx)).flush({ error: 'username is reserved' }, { status: 400, statusText: 'Bad Request' });
    await ctx.refresh();

    expect(text(ctx.el())).toContain("Ce nom d'utilisateur n'est pas disponible");
  });

  it('asks to try again later when too many attempts were made', async () => {
    const ctx = await setup();
    (await register(ctx)).flush({ error: 'too many registration attempts, try again later' }, { status: 429, statusText: 'Too Many Requests' });
    await ctx.refresh();

    expect(text(ctx.el())).toContain('Trop de tentatives');
    expect(ctx.el().querySelector('form')).not.toBeNull();
  });

  it('says the confirmation mail could not be sent on a 503, and keeps the form for another try', async () => {
    const ctx = await setup();
    (await register(ctx)).flush({ error: 'the confirmation e-mail could not be sent, try again later' }, { status: 503, statusText: 'Service Unavailable' });
    await ctx.refresh();

    expect(text(ctx.el())).toContain("Le message de confirmation n'a pas pu être envoyé");
    expect(ctx.el().querySelector('form')).not.toBeNull();
  });

  it('falls back to the closed view when the server says registration is disabled', async () => {
    const ctx = await setup();
    (await register(ctx)).flush({ error: 'registration is disabled' }, { status: 400, statusText: 'Bad Request' });
    await ctx.refresh();

    expect(text(ctx.el().querySelector('h1'))).toBe('Les inscriptions sont fermées');
  });

  it('shows a generic failure for anything else', async () => {
    const ctx = await setup();
    (await register(ctx)).flush(null, { status: 500, statusText: 'Server Error' });
    await ctx.refresh();

    expect(text(ctx.el())).toContain("L'inscription a échoué, réessayez.");
  });
});
