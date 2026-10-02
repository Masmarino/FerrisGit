import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { By } from '@angular/platform-browser';
import { Router, provideRouter } from '@angular/router';
import { ConfirmDangerModal, MfaSettings as GbtMfaSettings } from '@masmarino/gabarit';
import { MfaSettings } from './mfa-settings';
import { AuthService } from '../../auth/auth.service';
import { provideFerrisgitAuth } from '../../auth/auth-kit';

// The QR code comes through our renderer, which lazily imports `qrcode`; jsdom has no canvas.
const toDataURL = vi.hoisted(() => vi.fn());
vi.mock('qrcode', () => ({ toDataURL }));

const STATUS = '/api/me/mfa';
const ENROLL = '/api/me/mfa/totp/enroll';
const CONFIRM = '/api/me/mfa/totp/confirm';
const REGENERATE = '/api/me/mfa/backup-codes/regenerate';
const DISABLE = '/api/me/mfa/totp/disable';
const OTPAUTH = 'otpauth://totp/FerrisGit:admin?secret=JBSWY3DPEHPK3PXP&issuer=FerrisGit';
const CODES = Array.from({ length: 10 }, (_, i) => `${String(i).repeat(8)}abcdef01${'2'.repeat(8)}${'f'.repeat(8)}`);
const WRONG_PASSWORD = { error: 'current password is incorrect' };
const failure = (status: number) => ({ status, statusText: 'x' });
const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();
const settle = () => new Promise<void>((resolve) => setTimeout(resolve, 10));

describe('MfaSettings', () => {
  beforeEach(() => {
    toDataURL.mockReset();
    toDataURL.mockResolvedValue('data:image/png;base64,QR');
  });

  function setup(status: { totpEnabled: boolean; backupCodesRemaining: number } = { totpEnabled: true, backupCodesRemaining: 8 }) {
    const calls: string[] = [];
    const auth = { setToken: vi.fn(), logout: vi.fn(() => calls.push('logout')) };
    TestBed.configureTestingModule({
      providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([]), provideFerrisgitAuth(), { provide: AuthService, useValue: auth }],
    });
    const navigate = vi.spyOn(TestBed.inject(Router), 'navigateByUrl').mockImplementation((url) => {
      calls.push(`navigate ${String(url)}`);
      return Promise.resolve(true);
    });
    const fixture = TestBed.createComponent(MfaSettings);
    const http = TestBed.inject(HttpTestingController);
    fixture.detectChanges();
    const status$ = http.expectOne(STATUS);
    expect(status$.request.method).toBe('GET');
    status$.flush({ passkeys: [], ...status });
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

  const button = (el: HTMLElement, label: string) => Array.from(el.querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === label);
  const field = (el: HTMLElement, label: string) => {
    const labelEl = Array.from(el.querySelectorAll('label')).find((l) => text(l) === label);
    return labelEl ? el.querySelector<HTMLInputElement>(`[id="${labelEl.getAttribute('for')}"]`) : null;
  };
  const type = (input: HTMLInputElement, value: string) => {
    input.value = value;
    input.dispatchEvent(new Event('input'));
  };
  const dialog = (ctx: Ctx) => ctx.fixture.debugElement.query(By.directive(ConfirmDangerModal))?.componentInstance as ConfirmDangerModal | undefined;

  async function prompt(ctx: Ctx, opener: string, password = 'hunter22') {
    button(ctx.el, opener)!.click();
    await ctx.refresh();
    type(field(ctx.el, 'Mot de passe actuel')!, password);
    await ctx.refresh();
    ctx.el.querySelector('form')!.dispatchEvent(new Event('submit'));
    await ctx.refresh();
  }

  async function disable(ctx: Ctx, password = 'hunter22') {
    await prompt(ctx, 'Réinitialiser', password);
    dialog(ctx)!.confirmed.emit();
    ctx.fixture.detectChanges();
    const request = ctx.http.expectOne(DISABLE);
    expect(request.request.body).toEqual({ currentPassword: password });
    return request;
  }

  afterEach(() => TestBed.inject(HttpTestingController).verify());

  it('is the kit card, in French, reading the status from the API', () => {
    const { fixture, el } = setup({ totpEnabled: true, backupCodesRemaining: 2 });

    expect(fixture.debugElement.query(By.directive(GbtMfaSettings))).toBeTruthy();
    expect(text(el.querySelector('gbt-card h2'))).toBe("Application d'authentification");
    expect(text(el)).toContain('Application configurée');
    expect(text(el)).toContain('2 codes de secours restants');
    expect(button(el, 'Régénérer les codes de secours')).toBeTruthy();
  });

  describe('removing the app (the session is revoked with it)', () => {
    it('signs out, then goes to /login, once the server removed it (204); no token is stored', async () => {
      const ctx = await setup();
      const request = await disable(ctx);
      expect(ctx.auth.logout).not.toHaveBeenCalled();
      expect(ctx.navigate).not.toHaveBeenCalled();

      request.flush(null, { status: 204, statusText: 'No Content' });
      await ctx.refresh();

      expect(ctx.auth.logout).toHaveBeenCalledTimes(1);
      expect(ctx.navigate).toHaveBeenCalledExactlyOnceWith('/login');
      expect(ctx.calls).toEqual(['logout', 'navigate /login']);
      expect(ctx.auth.setToken).not.toHaveBeenCalled();
    });

    it('signs nobody out on a wrong password (400)', async () => {
      const ctx = await setup();

      (await disable(ctx, 'nope')).flush(WRONG_PASSWORD, failure(400));
      await ctx.refresh();

      expect(text(ctx.el.querySelector('.gbt-input__error'))).toBe('Mot de passe incorrect');
      expect(ctx.auth.logout).not.toHaveBeenCalled();
      expect(ctx.navigate).not.toHaveBeenCalled();
    });

    it.each([[500], [429]])('signs nobody out when the server fails (%i)', async (status) => {
      const ctx = await setup();

      (await disable(ctx)).flush({}, failure(status));
      await ctx.refresh();

      expect(ctx.auth.logout).not.toHaveBeenCalled();
      expect(ctx.navigate).not.toHaveBeenCalled();
    });
  });

  it('keeps the session through a regeneration of the backup codes', async () => {
    const ctx = await setup();

    await prompt(ctx, 'Régénérer les codes de secours');
    const request = ctx.http.expectOne(REGENERATE);
    expect(request.request.body).toEqual({ currentPassword: 'hunter22' });
    request.flush({ backupCodes: CODES });
    await ctx.refresh();

    expect(text(ctx.el.querySelector('gbt-card h2'))).toBe('Codes de secours');
    expect(ctx.auth.logout).not.toHaveBeenCalled();
    expect(ctx.navigate).not.toHaveBeenCalled();
  });

  it('draws the enrolment QR locally, with the kit options, and names it in French', async () => {
    const ctx = await setup({ totpEnabled: false, backupCodesRemaining: 0 });

    await prompt(ctx, 'Configurer maintenant');
    ctx.http.expectOne(ENROLL).flush({ secret: 'JBSWY3DPEHPK3PXP', otpauthUrl: OTPAUTH });
    await ctx.refresh();

    expect(toDataURL).toHaveBeenCalledExactlyOnceWith(OTPAUTH, { errorCorrectionLevel: 'M', margin: 1, width: 448 });
    const qr = ctx.el.querySelector<HTMLImageElement>('gbt-totp-qr img')!;
    expect(qr.getAttribute('src')).toBe('data:image/png;base64,QR');
    expect(qr.getAttribute('alt')).toBe("QR code à scanner avec votre application d'authentification");
    expect(text(ctx.el.querySelector('gbt-totp-qr'))).toContain('Ou saisissez cette clé dans votre application');

    type(field(ctx.el, 'Code à 6 chiffres')!, '123456');
    ctx.el.querySelector('form')!.dispatchEvent(new Event('submit'));
    ctx.http.expectOne(CONFIRM).flush({ backupCodes: CODES });
    await ctx.refresh();
    expect(text(ctx.el)).toContain("J'ai enregistré mes codes de secours");
    expect(ctx.auth.logout).not.toHaveBeenCalled();
  });
});
