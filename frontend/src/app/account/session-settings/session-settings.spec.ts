import { TestBed } from '@angular/core/testing';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing';
import { Router, provideRouter } from '@angular/router';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { AuthService } from '../../auth/auth.service';
import { SessionSettings } from './session-settings';

describe('SessionSettings', () => {
  // The token goes to localStorage, which the next spec files share.
  afterEach(() => localStorage.clear());

  function setup() {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([])] });
    const fixture = TestBed.createComponent(SessionSettings);
    fixture.detectChanges();
    const el = fixture.nativeElement as HTMLElement;
    const http = TestBed.inject(HttpTestingController);
    const navigate = vi.spyOn(TestBed.inject(Router), 'navigateByUrl').mockResolvedValue(true);
    const buttonNamed = (root: ParentNode, label: string) =>
      Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find((b) => b.textContent?.trim() === label)!;
    /** Asks, then confirms in the dialog, which names the same action. */
    const confirm = async () => {
      buttonNamed(el.querySelector('gbt-card')!, 'Se déconnecter partout').click();
      fixture.detectChanges();
      await fixture.whenStable();
      buttonNamed(el.querySelector('gbt-confirm-danger-modal')!, 'Se déconnecter partout').click();
      fixture.detectChanges();
    };
    return { fixture, el, http, navigate, confirm };
  }

  it('says what signing out everywhere does, and what it leaves', () => {
    const { el } = setup();

    expect(el.querySelector('h2')?.textContent?.trim()).toBe('Sessions');
    expect(el.textContent).toContain('Ferme toutes vos sessions ouvertes, celle-ci comprise.');
    expect(el.textContent).toContain("Vos jetons d'accès restent valides");
  });

  it('ends every session after a confirmation, then goes to sign-in', async () => {
    const { http, navigate, confirm } = setup();
    const auth = TestBed.inject(AuthService);
    auth.setToken('session');

    await confirm();
    http.expectOne({ method: 'POST', url: '/api/auth/logout-all' }).flush(null);

    expect(auth.token()).toBeNull();
    expect(navigate).toHaveBeenCalledWith('/login');
  });

  it('keeps the session and says so when the server refuses', async () => {
    const { http, navigate, confirm } = setup();
    const toast = vi.spyOn(TestBed.inject(GbtToastService), 'show');
    TestBed.inject(AuthService).setToken('session');

    await confirm();
    http.expectOne('/api/auth/logout-all').flush(null, { status: 500, statusText: 'Server Error' });

    expect(TestBed.inject(AuthService).token()).toBe('session');
    expect(navigate).not.toHaveBeenCalled();
    expect(toast).toHaveBeenCalledWith('Impossible de fermer les sessions. Réessayez.', 'error');
  });
});
