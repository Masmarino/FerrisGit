import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { By } from '@angular/platform-browser';
import { Router, provideRouter } from '@angular/router';
import { type AuthConfig, ConfirmDangerModal, type Passkey, PasskeySettings as GbtPasskeySettings } from '@masmarino/gabarit';
import { Observable, of } from 'rxjs';
import { PasskeySettings } from './passkey-settings';
import { AuthService } from '../../auth/auth.service';
import { provideFerrisgitAuth } from '../../auth/auth-kit';

const STATUS = '/api/me/mfa';
const DELETE = (id: string) => `/api/me/mfa/passkeys/${id}/delete`;
const HOUR = 60 * 60 * 1000;
const DAY = 24 * HOUR;
const ago = (ms: number) => new Date(Date.now() - ms).toISOString();
const MACBOOK: Passkey = { id: 'p1', name: 'MacBook Touch ID', createdAt: '2025-03-12T09:30:00Z', lastUsedAt: ago(2 * HOUR) };
const YUBIKEY: Passkey = { id: 'p2', name: 'YubiKey', createdAt: ago(3 * DAY), lastUsedAt: null };
const WRONG_PASSWORD = { error: 'current password is incorrect' };
const failure = (status: number) => ({ status, statusText: 'x' });
const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();
const settle = () => new Promise<void>((resolve) => setTimeout(resolve, 10));

describe('PasskeySettings', () => {
  function setup(passkeys: Passkey[] = [MACBOOK, YUBIKEY], config: Observable<AuthConfig> = of({ registrationEnabled: false, passkeysAvailable: true })) {
    const calls: string[] = [];
    const auth = { setToken: vi.fn(), logout: vi.fn(() => calls.push('logout')), authConfig: vi.fn(() => config) };
    // No LOCALE_ID here: the page's `[locale]="'fr'"` is what makes the dates French.
    TestBed.configureTestingModule({
      providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([]), provideFerrisgitAuth(), { provide: AuthService, useValue: auth }],
    });
    const navigate = vi.spyOn(TestBed.inject(Router), 'navigateByUrl').mockImplementation((url) => {
      calls.push(`navigate ${String(url)}`);
      return Promise.resolve(true);
    });
    const fixture = TestBed.createComponent(PasskeySettings);
    const http = TestBed.inject(HttpTestingController);
    fixture.detectChanges();
    http.expectOne(STATUS).flush({ totpEnabled: true, backupCodesRemaining: 8, passkeys });
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;
    const refresh = async () => {
      fixture.detectChanges();
      await fixture.whenStable();
      await settle();
      fixture.detectChanges();
    };
    return { fixture, http, auth, navigate, calls, el, refresh };
  }
  type Ctx = ReturnType<typeof setup>;

  const field = (el: HTMLElement, label: string) => {
    const labelEl = Array.from(el.querySelectorAll('label')).find((l) => text(l) === label);
    return labelEl ? el.querySelector<HTMLInputElement>(`[id="${labelEl.getAttribute('for')}"]`) : null;
  };
  const type = (input: HTMLInputElement, value: string) => {
    input.value = value;
    input.dispatchEvent(new Event('input'));
  };
  const dialog = (ctx: Ctx) => ctx.fixture.debugElement.query(By.directive(ConfirmDangerModal))?.componentInstance as ConfirmDangerModal | undefined;

  async function deleteKey(ctx: Ctx, passkey: Passkey, password = 'hunter22') {
    ctx.el.querySelector<HTMLButtonElement>(`[data-passkey-id="${passkey.id}"] [data-opener="delete"] button`)!.click();
    await ctx.refresh();
    type(field(ctx.el, 'Mot de passe actuel')!, password);
    await ctx.refresh();
    ctx.el.querySelector(`[data-passkey-id="${passkey.id}"] form`)!.dispatchEvent(new Event('submit'));
    await ctx.refresh();
    dialog(ctx)!.confirmed.emit();
    ctx.fixture.detectChanges();
    const request = ctx.http.expectOne(DELETE(passkey.id));
    expect(request.request.body).toEqual({ currentPassword: password });
    return request;
  }

  afterEach(() => TestBed.inject(HttpTestingController).verify());

  it('is the kit card, in French, listing the keys from the API with French dates', () => {
    const { fixture, el, auth } = setup();

    expect(fixture.debugElement.query(By.directive(GbtPasskeySettings))).toBeTruthy();
    expect(text(el.querySelector('gbt-card h2'))).toBe("Clés d'accès 2");
    expect(el.querySelector('ul')?.getAttribute('aria-label')).toBe("Clés d'accès enregistrées");
    expect(text(el.querySelector('[data-passkey-id="p1"] .gbt-passkey-settings__name'))).toBe('MacBook Touch ID');
    expect(text(el.querySelector('[data-passkey-id="p1"] .gbt-passkey-settings__meta'))).toBe('Ajoutée le 12/03/2025 Dernière utilisation il y a 2 h');
    expect(text(el.querySelector('[data-passkey-id="p2"] .gbt-passkey-settings__meta'))).toBe('Ajoutée il y a 3 j Jamais utilisée');
    expect(el.querySelector('[data-passkey-id="p1"] [data-opener="delete"] button')?.getAttribute('aria-label')).toBe('Supprimer la clé MacBook Touch ID');
    expect(auth.authConfig).toHaveBeenCalledTimes(1);
  });

  it('says adding is impossible, in French, when the server cannot run passkeys', () => {
    const { el } = setup([MACBOOK], of({ registrationEnabled: false, passkeysAvailable: false }));

    // jsdom has no WebAuthn, so the browser's reason is shown first, in French.
    expect(text(el.querySelector('gbt-alert'))).toContain("Ce navigateur ne prend pas en charge les clés d'accès");
  });

  describe('deleting a key (every session is revoked with it)', () => {
    it('signs out, then goes to /login, once the server deleted it (204); no token is stored', async () => {
      const ctx = setup();
      const request = await deleteKey(ctx, MACBOOK);
      expect(ctx.auth.logout).not.toHaveBeenCalled();

      request.flush(null, { status: 204, statusText: 'No Content' });
      await ctx.refresh();

      expect(ctx.auth.logout).toHaveBeenCalledTimes(1);
      expect(ctx.navigate).toHaveBeenCalledExactlyOnceWith('/login');
      expect(ctx.calls).toEqual(['logout', 'navigate /login']);
      expect(ctx.auth.setToken).not.toHaveBeenCalled();
    });

    it('signs nobody out on a wrong password (400)', async () => {
      const ctx = setup();

      (await deleteKey(ctx, MACBOOK, 'nope')).flush(WRONG_PASSWORD, failure(400));
      await ctx.refresh();

      expect(text(ctx.el.querySelector('.gbt-input__error'))).toBe('Mot de passe incorrect');
      expect(ctx.auth.logout).not.toHaveBeenCalled();
      expect(ctx.navigate).not.toHaveBeenCalled();
    });

    it.each([[500], [429]])('signs nobody out when the server fails (%i)', async (status) => {
      const ctx = setup();

      (await deleteKey(ctx, MACBOOK)).flush({}, failure(status));
      await ctx.refresh();

      expect(ctx.auth.logout).not.toHaveBeenCalled();
      expect(ctx.navigate).not.toHaveBeenCalled();
    });

    it('signs nobody out for a key that was already gone (404): it only leaves the list', async () => {
      const ctx = setup();

      (await deleteKey(ctx, MACBOOK)).flush({ error: 'passkey not found' }, failure(404));
      await ctx.refresh();

      expect(ctx.el.querySelector('[data-passkey-id="p1"]')).toBeNull();
      expect(text(ctx.el)).toContain("La clé d'accès « MacBook Touch ID » n'existe plus.");
      expect(ctx.auth.logout).not.toHaveBeenCalled();
      expect(ctx.navigate).not.toHaveBeenCalled();
    });

    it('encodes the id of the key in the path', async () => {
      const ctx = setup([{ ...MACBOOK, id: 'a/b' }]);

      ctx.el.querySelector<HTMLButtonElement>('[data-opener="delete"] button')!.click();
      await ctx.refresh();
      type(field(ctx.el, 'Mot de passe actuel')!, 'hunter22');
      await ctx.refresh();
      ctx.el.querySelector('li form')!.dispatchEvent(new Event('submit'));
      await ctx.refresh();
      dialog(ctx)!.confirmed.emit();

      ctx.http.expectOne('/api/me/mfa/passkeys/a%2Fb/delete').flush(null, { status: 204, statusText: 'No Content' });
      await ctx.refresh();
      expect(ctx.auth.logout).toHaveBeenCalledTimes(1);
    });
  });
});
