import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { provideRouter, Router } from '@angular/router';
import { RouterTestingHarness } from '@angular/router/testing';
import { DevicePage } from './device-page';
import { DeviceRequest } from './device-login.service';
import { PageTitleService } from '../../shell/page-title.service';
import { REQUEST_OPTIONS, dismissedPrompt, fakeAssertion, stubPasskeyBrowser } from '../../shared/webauthn-testing';

const REQUEST: DeviceRequest = {
  userCode: 'BCDF-GHJK',
  clientName: 'fgit sur portable',
  scopes: ['issues:read', 'merge-requests:write'],
  tokenLifetimeDays: 90,
  requestedAt: '2026-11-02T10:00:00Z',
  expiresAt: '2026-11-02T10:15:00Z',
  hasTotp: true,
  hasPasskey: false,
};

const FIND = '/api/auth/device/BCDF-GHJK';
const APPROVE = `${FIND}/approve`;
const DENY = `${FIND}/deny`;
const PASSKEY = `${FIND}/passkey`;

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();
const settle = () => new Promise<void>((resolve) => setTimeout(resolve, 10));

describe('DevicePage', () => {
  async function setup(url = '/account/device?code=BCDF-GHJK') {
    const pageTitle = { set: vi.fn() };
    TestBed.configureTestingModule({
      providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([{ path: 'account/device', component: DevicePage }]), { provide: PageTitleService, useValue: pageTitle }],
    });
    const harness = await RouterTestingHarness.create(url);
    const http = TestBed.inject(HttpTestingController);
    const el = () => harness.routeNativeElement as HTMLElement;
    const refresh = async () => {
      harness.detectChanges();
      await harness.fixture.whenStable(); // ngModel writes a field's value asynchronously
      await settle();
      harness.detectChanges();
    };
    const button = (label: string) => Array.from(el().querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === label);
    const field = (label: string) => {
      const labelEl = Array.from(el().querySelectorAll('gbt-input label')).find((l) => text(l) === label);
      return labelEl ? el().querySelector<HTMLInputElement>(`[id="${labelEl.getAttribute('for')}"]`) : null;
    };
    const type = async (label: string, value: string) => {
      const input = field(label)!;
      input.value = value;
      input.dispatchEvent(new Event('input'));
      await refresh();
    };
    const submit = () => el().querySelector('form')!.dispatchEvent(new Event('submit', { cancelable: true }));
    const answer = async (request: DeviceRequest = REQUEST) => {
      http.expectOne(FIND).flush(request);
      await refresh();
    };
    const fail = async (url: string, status: number, error = 'nope') => {
      http.expectOne(url).flush({ error }, { status, statusText: 'Refused' });
      await refresh();
    };
    const heading = () => text(el().querySelector('gbt-card h2'));
    const alert = () => text(el().querySelector('.device-page__error'));
    return { harness, http, el, refresh, button, field, type, submit, answer, fail, heading, alert, pageTitle, router: TestBed.inject(Router) };
  }

  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  it('names the page', async () => {
    const { answer, pageTitle, el } = await setup();
    await answer();

    expect(pageTitle.set).toHaveBeenCalledWith('Connecter un terminal');
    expect(text(el().querySelector('gbt-page-header h1'))).toBe('Connecter un terminal');
  });

  describe('without a code in the URL', () => {
    it('asks for one, without calling the server', async () => {
      const { field, button, heading, refresh } = await setup('/account/device');
      await refresh();

      expect(heading()).toBe('Saisir le code');
      expect(field('Code affiché par le terminal')).not.toBeNull();
      expect(button('Continuer')!.disabled).toBe(true);
    });

    it('opens the request of the typed code, whatever its case and dash, with the code in the URL', async () => {
      const { type, submit, refresh, router, answer, el } = await setup('/account/device');
      await refresh();

      await type('Code affiché par le terminal', ' bcdfghjk ');
      submit();
      await refresh();

      expect(router.url).toBe('/account/device?code=BCDF-GHJK');
      await answer();
      expect(text(el().querySelector('.device-page__code'))).toBe('BCDF-GHJK');
    });

    it('says what a code looks like when the typed one cannot be one', async () => {
      const { type, submit, refresh, el } = await setup('/account/device');
      await refresh();

      await type('Code affiché par le terminal', 'trop-court');
      submit();
      await refresh();

      expect(text(el().querySelector('gbt-input'))).toContain('il compte 8 lettres ou chiffres');
    });

    it('sends nothing to the server for a malformed code in the URL, a path among them', async () => {
      const { refresh, heading, el } = await setup(`/account/device?code=${encodeURIComponent('../../access-tokens')}`);
      await refresh();

      expect(heading()).toBe('Saisir le code');
      expect(text(el().querySelector('gbt-input'))).toContain('il compte 8 lettres ou chiffres');
    });
  });

  describe('loading a request', () => {
    it('shows a loading state until the server answers', async () => {
      const { el, answer } = await setup();

      expect(text(el().querySelector('gbt-skeleton-list [role="status"]'))).toBe('Chargement de la demande…');
      await answer();
      expect(el().querySelector('gbt-skeleton-list')).toBeNull();
    });

    it('accepts a code in lower case in the URL, as the server does', async () => {
      const { http, refresh } = await setup('/account/device?code=bcdfghjk');
      await refresh();

      http.expectOne(FIND).flush(REQUEST);
      await refresh();
    });

    it('says what is requested: the code, the client, the scopes in plain words and the lifetime, with the warning', async () => {
      const { el, answer } = await setup();
      await answer();

      expect(text(el().querySelector('.device-page__code'))).toBe('BCDF-GHJK');
      expect(text(el().querySelector('.device-page__client'))).toBe('fgit sur portable');
      expect(Array.from(el().querySelectorAll('.device-page__scope')).map(text)).toEqual(['Tickets : lecture', 'Demandes de fusion : lecture et écriture']);
      expect(text(el().querySelector('.device-page__lifetime'))).toBe('90 jours');
      const warning = text(el().querySelector('.device-page__warning'));
      expect(warning).toContain('Ne confirmez que si vous venez de lancer cette connexion vous-même');
      expect(warning).toContain('le code ci-dessous est celui qu');
    });

    it('shows a scope this version does not know as written, rather than hiding it', async () => {
      const { el, answer } = await setup();
      await answer({ ...REQUEST, scopes: ['deploy:write'] });

      expect(Array.from(el().querySelectorAll('.device-page__scope')).map(text)).toEqual(['deploy:write']);
    });

    it('writes one day in the singular', async () => {
      const { el, answer } = await setup();
      await answer({ ...REQUEST, tokenLifetimeDays: 1 });

      expect(text(el().querySelector('.device-page__lifetime'))).toBe('1 jour');
    });

    it('keeps clientName as text: its markup is shown, never interpreted', async () => {
      const { el, answer } = await setup();
      const clientName = '<img src=x onerror="alert(1)"><b>fgit</b>';
      await answer({ ...REQUEST, clientName });

      const client = el().querySelector('.device-page__client')!;
      expect(client.textContent).toBe(clientName);
      expect(client.children).toHaveLength(0);
      expect(el().querySelector('img')).toBeNull();
    });

    it('says the request is gone on a 404, and offers another code', async () => {
      const { fail, heading, el, button, refresh, router } = await setup();
      await fail(FIND, 404, 'device request');

      expect(heading()).toBe('Demande introuvable');
      expect(text(el().querySelector('gbt-card'))).toContain('a expiré');
      expect(el().querySelector('form')).toBeNull();

      button('Saisir un autre code')!.click();
      await refresh();
      expect(router.url).toBe('/account/device');
      expect(heading()).toBe('Saisir le code');
    });

    it('says to wait on a 429, and retrying asks again', async () => {
      const { fail, heading, el, button, http, refresh } = await setup();
      await fail(FIND, 429);

      expect(heading()).toBe('Trop de tentatives');
      expect(text(el().querySelector('gbt-card'))).toContain('réessayez dans quelques minutes');

      button('Réessayer')!.click();
      await refresh();
      http.expectOne(FIND).flush(REQUEST);
      await refresh();
      expect(heading()).toBe('Demande de connexion');
    });

    it('offers to retry when loading fails otherwise', async () => {
      const { fail, el, button, answer, refresh } = await setup();
      await fail(FIND, 500);

      expect(text(el().querySelector('gbt-alert'))).toContain("La demande n'a pas pu être chargée.");

      button('Réessayer')!.click();
      await refresh();
      await answer();
      expect(el().querySelector('.device-page__code')).not.toBeNull();
    });

    it('stays quiet on a 401, which the interceptor answers by going to the sign-in page', async () => {
      const { fail, el } = await setup();
      await fail(FIND, 401);

      expect(el().querySelector('gbt-alert')).toBeNull();
      expect(el().querySelector('gbt-skeleton-list')).not.toBeNull();
    });
  });

  describe('approving with a TOTP code', () => {
    it('asks for the code only on an account with TOTP, and no passkey button', async () => {
      const { field, button, answer } = await setup();
      await answer();

      expect(field('Code à 6 chiffres')).not.toBeNull();
      expect(button("Confirmer avec une clé d'accès")).toBeUndefined();
      expect(button('Confirmer')!.disabled).toBe(true);
    });

    it('sends the code without its spaces, and says the terminal can go on', async () => {
      const { http, type, submit, answer, refresh, heading, el } = await setup();
      await answer();

      await type('Code à 6 chiffres', '123 456');
      submit();
      const approve = http.expectOne(APPROVE);
      expect(approve.request.method).toBe('POST');
      expect(approve.request.body).toEqual({ code: '123456' });
      approve.flush(null, { status: 204, statusText: 'No Content' });
      await refresh();

      expect(heading()).toBe('Terminal connecté');
      expect(text(el().querySelector('gbt-card'))).toContain('Vous pouvez retourner à votre terminal.');
      expect(el().querySelector('form')).toBeNull();
      expect(document.activeElement).toBe(el().querySelector('.device-page__result h2'));
    });

    it('stays on the form on a wrong code, says so, and empties the field for the next try', async () => {
      const { type, submit, answer, fail, alert, field, button, heading } = await setup();
      await answer();

      await type('Code à 6 chiffres', '000000');
      submit();
      await fail(APPROVE, 400, 'invalid code');

      expect(heading()).toBe('Demande de connexion');
      expect(alert()).toBe('Code incorrect. Réessayez avec un nouveau code.');
      expect(field('Code à 6 chiffres')!.value).toBe('');
      expect(document.activeElement).toBe(field('Code à 6 chiffres'));
      expect(button('Confirmer')).toBeDefined();
    });

    it('stays on the form when rate limited, and says to wait', async () => {
      const { type, submit, answer, fail, alert, heading } = await setup();
      await answer();

      await type('Code à 6 chiffres', '000000');
      submit();
      await fail(APPROVE, 429);

      expect(heading()).toBe('Demande de connexion');
      expect(alert()).toBe('Trop de tentatives, réessayez dans quelques minutes.');
    });

    it('says the admin scope is out of reach when the server refuses it for that', async () => {
      const { type, submit, answer, fail, alert } = await setup();
      await answer({ ...REQUEST, scopes: ['admin:write'] });

      await type('Code à 6 chiffres', '123456');
      submit();
      await fail(APPROVE, 400, 'only an administrator can create a token with the admin scope');

      expect(alert()).toContain('réservée aux administrateurs');
    });

    it('moves to the "gone" state when the request ended meanwhile', async () => {
      const { type, submit, answer, fail, heading } = await setup();
      await answer();

      await type('Code à 6 chiffres', '123456');
      submit();
      await fail(APPROVE, 404, 'device request');

      expect(heading()).toBe('Demande introuvable');
    });

    it('sends one approval while it runs', async () => {
      const { http, type, submit, answer, refresh } = await setup();
      await answer();

      await type('Code à 6 chiffres', '123456');
      submit();
      submit();

      http.expectOne(APPROVE).flush(null, { status: 204, statusText: 'No Content' });
      await refresh();
    });
  });

  describe('approving with a backup code', () => {
    it('offers it next to the TOTP code, and sends it as backupCode', async () => {
      const { http, button, field, type, submit, answer, refresh, heading } = await setup();
      await answer();

      button('Utiliser un code de secours')!.click();
      await refresh();
      expect(field('Code à 6 chiffres')).toBeNull();
      expect(document.activeElement).toBe(field('Code de secours'));

      await type('Code de secours', ' abcd1234 ');
      submit();
      const approve = http.expectOne(APPROVE);
      expect(approve.request.body).toEqual({ backupCode: 'abcd1234' });
      approve.flush(null, { status: 204, statusText: 'No Content' });
      await refresh();

      expect(heading()).toBe('Terminal connecté');
    });

    it('says a backup code was refused in its own words', async () => {
      const { button, type, submit, answer, fail, alert, refresh } = await setup();
      await answer();
      button('Utiliser un code de secours')!.click();
      await refresh();

      await type('Code de secours', 'abcd');
      submit();
      await fail(APPROVE, 400, 'invalid code');

      expect(alert()).toBe('Code de secours incorrect ou déjà utilisé.');
    });

    it('can go back to the other factor', async () => {
      const { button, field, answer, refresh } = await setup();
      await answer();
      button('Utiliser un code de secours')!.click();
      await refresh();

      button('Revenir à mon second facteur')!.click();
      await refresh();

      expect(field('Code à 6 chiffres')).not.toBeNull();
    });
  });

  describe('approving with a passkey', () => {
    const PASSKEY_ONLY = { ...REQUEST, hasTotp: false, hasPasskey: true };

    it('asks the browser for the assertion, then approves with the challenge and the credential', async () => {
      const get = vi.fn(() => Promise.resolve(fakeAssertion()));
      onTestFinished(stubPasskeyBrowser({ get, create: vi.fn() }));
      const { http, button, field, answer, refresh, heading } = await setup();
      await answer(PASSKEY_ONLY);
      expect(field('Code à 6 chiffres')).toBeNull();

      button("Confirmer avec une clé d'accès")!.click();
      const start = http.expectOne(PASSKEY);
      expect(start.request.method).toBe('POST');
      start.flush({ challengeId: 'c1', publicKey: REQUEST_OPTIONS });
      await refresh();

      expect(get).toHaveBeenCalledTimes(1);
      const options = (get.mock.calls[0] as unknown as [{ publicKey: { challenge: unknown } }])[0];
      expect(options.publicKey.challenge).toBeInstanceOf(ArrayBuffer);
      const approve = http.expectOne(APPROVE);
      expect(approve.request.body.challengeId).toBe('c1');
      expect(approve.request.body.credential).toMatchObject({ id: 'credential-id', type: 'public-key', rawId: 'AQID' });
      approve.flush(null, { status: 204, statusText: 'No Content' });
      await refresh();

      expect(heading()).toBe('Terminal connecté');
    });

    it('offers both factors to an account with both', async () => {
      onTestFinished(stubPasskeyBrowser({ get: vi.fn(), create: vi.fn() }));
      const { button, field, answer } = await setup();
      await answer({ ...REQUEST, hasPasskey: true });

      expect(button("Confirmer avec une clé d'accès")).toBeDefined();
      expect(field('Code à 6 chiffres')).not.toBeNull();
    });

    it('approves nothing when the prompt is dismissed, and says it can be tried again', async () => {
      onTestFinished(stubPasskeyBrowser({ get: vi.fn(() => Promise.reject(dismissedPrompt())), create: vi.fn() }));
      const { http, button, answer, refresh, alert } = await setup();
      await answer(PASSKEY_ONLY);

      button("Confirmer avec une clé d'accès")!.click();
      http.expectOne(PASSKEY).flush({ challengeId: 'c1', publicKey: REQUEST_OPTIONS });
      await refresh();

      http.expectNone(APPROVE);
      expect(alert()).toBe('Validation annulée. Vous pouvez réessayer.');
      expect(button("Confirmer avec une clé d'accès")!.disabled).toBe(false);
    });

    it('says the passkey was refused on a 400 invalid code, and stays', async () => {
      onTestFinished(stubPasskeyBrowser({ get: vi.fn(() => Promise.resolve(fakeAssertion())), create: vi.fn() }));
      const { http, button, answer, refresh, fail, alert, heading } = await setup();
      await answer(PASSKEY_ONLY);

      button("Confirmer avec une clé d'accès")!.click();
      http.expectOne(PASSKEY).flush({ challengeId: 'c1', publicKey: REQUEST_OPTIONS });
      await refresh();
      await fail(APPROVE, 400, 'invalid code');

      expect(heading()).toBe('Demande de connexion');
      expect(alert()).toBe("Cette clé d'accès n'a pas été acceptée.");
    });

    it('says passkeys are off when the server answers 503', async () => {
      onTestFinished(stubPasskeyBrowser({ get: vi.fn(), create: vi.fn() }));
      const { button, answer, fail, alert } = await setup();
      await answer(PASSKEY_ONLY);

      button("Confirmer avec une clé d'accès")!.click();
      await fail(PASSKEY, 503, 'passkeys are not available on this server');

      expect(alert()).toContain("Les clés d'accès ne sont pas disponibles");
    });

    it('falls back on the backup code when this browser has no passkeys and nothing else is left', async () => {
      onTestFinished(stubPasskeyBrowser(null));
      const { button, field, answer, el } = await setup();
      await answer(PASSKEY_ONLY);

      expect(button("Confirmer avec une clé d'accès")).toBeUndefined();
      expect(field('Code de secours')).not.toBeNull();
      expect(button('Revenir à mon second facteur')).toBeUndefined();
      expect(text(el().querySelector('form gbt-alert'))).toContain('Ce navigateur ne gère pas');
    });

    it('sends no approval once the page is left during the prompt', async () => {
      let resolve: (value: unknown) => void = () => {};
      onTestFinished(stubPasskeyBrowser({ get: vi.fn(() => new Promise((done) => (resolve = done))), create: vi.fn() }));
      const { http, button, answer, refresh, harness } = await setup();
      await answer(PASSKEY_ONLY);

      button("Confirmer avec une clé d'accès")!.click();
      http.expectOne(PASSKEY).flush({ challengeId: 'c1', publicKey: REQUEST_OPTIONS });
      await refresh();
      harness.fixture.destroy();
      resolve(fakeAssertion());
      await settle();

      http.expectNone(APPROVE);
    });
  });

  describe('refusing', () => {
    it('calls deny, and says the terminal got nothing', async () => {
      const { http, button, answer, refresh, heading, el } = await setup();
      await answer();

      button('Refuser')!.click();
      const deny = http.expectOne(DENY);
      expect(deny.request.method).toBe('POST');
      deny.flush(null, { status: 204, statusText: 'No Content' });
      await refresh();

      expect(heading()).toBe('Connexion refusée');
      expect(text(el().querySelector('gbt-card'))).toContain("Le terminal n'a reçu aucun jeton.");
    });

    it('needs no second factor', async () => {
      const { button, answer } = await setup();
      await answer();

      expect(button('Refuser')!.disabled).toBe(false);
      button('Refuser')!.click();
      TestBed.inject(HttpTestingController).expectOne(DENY).flush(null, { status: 204, statusText: 'No Content' });
    });

    it('says when the refusal failed, and stays', async () => {
      const { button, answer, fail, alert, heading } = await setup();
      await answer();

      button('Refuser')!.click();
      await fail(DENY, 500);

      expect(heading()).toBe('Demande de connexion');
      expect(alert()).toBe("Le refus n'a pas abouti. Réessayez.");
    });

    it('moves to the "gone" state on a 404', async () => {
      const { button, answer, fail, heading } = await setup();
      await answer();

      button('Refuser')!.click();
      await fail(DENY, 404, 'device request');

      expect(heading()).toBe('Demande introuvable');
    });
  });
});
