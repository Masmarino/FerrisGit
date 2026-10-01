import { Component, signal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { provideRouter, Router } from '@angular/router';
import { RouterTestingHarness } from '@angular/router/testing';
import { Observable, of, Subject } from 'rxjs';
import { Icon, GbtToastService, type MfaStatus } from '@masmarino/gabarit';
import { AccountPage } from './account-page';
import { AuthService } from '../../auth/auth.service';
import { Me, MeService } from '../../shell/me.service';
import { PageTitleService } from '../../shell/page-title.service';
import { ApiTokenSummary, TokensService } from '../../api-tokens/api-tokens.service';
import { ApiTokensList } from '../../api-tokens/api-tokens-list/api-tokens-list';
import { MfaService } from '../mfa.service';
import { provideFerrisgitAuth } from '../../auth/auth-kit';
import { MfaSettings } from '../mfa-settings/mfa-settings';
import { stubPasskeyBrowser } from '../../shared/webauthn-testing';
import { PasskeySettings } from '../passkey-settings/passkey-settings';

@Component({ template: '' })
class LoginStub {}

const ME: Me = { id: 'u1', username: 'alice', email: 'alice@example.com', isAdmin: false };

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();

describe('AccountPage', () => {
  async function setup(url = '/account') {
    const meStub = {
      id: signal(ME.id),
      username: signal(ME.username),
      email: signal(ME.email),
      isAdmin: signal(ME.isAdmin),
      load: vi.fn(),
      updateEmail: vi.fn((email: string) => {
        meStub.email.set(email);
        return of<Me>({ ...ME, email });
      }),
      changePassword: vi.fn((): Observable<{ token?: string } | null> => of(null)),
    };
    const authStub = { logout: vi.fn(), setToken: vi.fn(), authConfig: vi.fn(() => of({ registrationEnabled: false, passkeysAvailable: true })) };
    const toastStub = { show: vi.fn() };
    const pageTitleStub = { set: vi.fn() };
    const tokensStub = {
      list: vi.fn(() => of<ApiTokenSummary[]>([])),
      create: vi.fn(() => of({ id: 't1', name: 'ci', token: 'plain-token' })),
      revoke: vi.fn(() => of<void>(undefined)),
    };
    const mfaStub = {
      status: vi.fn(() => of<MfaStatus>({ totpEnabled: true, backupCodesRemaining: 8, passkeys: [] })),
      enroll: vi.fn(),
      confirm: vi.fn(),
      regenerate: vi.fn(),
      disable: vi.fn(),
    };
    TestBed.configureTestingModule({
      providers: [
        provideRouter([
          { path: 'account', component: AccountPage },
          { path: 'login', component: LoginStub },
        ]),
        { provide: MeService, useValue: meStub },
        { provide: AuthService, useValue: authStub },
        { provide: GbtToastService, useValue: toastStub },
        { provide: PageTitleService, useValue: pageTitleStub },
        { provide: TokensService, useValue: tokensStub },
        { provide: MfaService, useValue: mfaStub },
        provideFerrisgitAuth(),
      ],
    });
    const harness = await RouterTestingHarness.create(url);
    const el = () => harness.routeNativeElement as HTMLElement;
    const settle = async () => {
      harness.fixture.detectChanges();
      await harness.fixture.whenStable(); // ngModel writes a field's value asynchronously
      harness.fixture.detectChanges();
    };
    const navigate = async (target: string) => {
      await harness.navigateByUrl(target);
      await settle();
    };
    const current = () => Array.from(el().querySelectorAll('gbt-nav-tabs a[aria-current="page"]')).map((a) => a.querySelector('.gbt-nav-tab__label')?.textContent);
    const card = (heading: string) => Array.from(el().querySelectorAll('gbt-card')).find((c) => text(c.querySelector('h2')) === heading);
    const field = (label: string) => {
      const labelEl = Array.from(el().querySelectorAll('gbt-input label')).find((l) => text(l) === label);
      return labelEl ? el().querySelector<HTMLInputElement>(`#${labelEl.getAttribute('for')}`) : null;
    };
    const type = (label: string, value: string) => {
      const input = field(label)!;
      input.value = value;
      input.dispatchEvent(new Event('input'));
    };
    const commit = (label: string, value: string) => {
      type(label, value);
      field(label)!.dispatchEvent(new Event('blur'));
    };
    const button = (label: string) =>
      Array.from(el().querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === label || b.getAttribute('aria-label') === label);
    await settle();
    return { harness, el, settle, navigate, current, card, field, type, commit, button, meStub, authStub, toastStub, pageTitleStub, tokensStub, mfaStub };
  }

  it('sets the page title and loads the current user', async () => {
    const { pageTitleStub, meStub } = await setup();

    expect(pageTitleStub.set).toHaveBeenCalledWith('Mon compte');
    expect(meStub.load).toHaveBeenCalled();
  });

  describe('page and navigation', () => {
    it('is a wide page: the "Mon compte" h1 above a layout whose sticky nav column holds the section navigation', async () => {
      const { el } = await setup();

      expect(text(el().querySelector('gbt-page-header h1'))).toBe('Mon compte');
      const layout = el().querySelector('gbt-page-layout')!;
      expect(layout.getAttribute('data-width')).toBe('wide');
      expect(layout.hasAttribute('data-sticky-nav')).toBe(true);
      const nav = layout.querySelector('nav.gbt-page-layout__nav')!;
      expect(nav.getAttribute('aria-label')).toBe('Réglages du compte');
      expect(nav.querySelector('gbt-nav-tabs')).not.toBeNull();
      expect(el().querySelectorAll('nav')).toHaveLength(1);
      expect(el().querySelector('.form-container')).toBeNull();
    });

    it('lists the four sections, in order, as links to ?section=<key> (the profile on the bare path), each with an icon', async () => {
      const { el } = await setup();

      const links = Array.from(el().querySelectorAll<HTMLAnchorElement>('gbt-nav-tabs a'));
      expect(links.map((a) => a.querySelector('.gbt-nav-tab__label')?.textContent)).toEqual(['Profil', 'Mot de passe', 'Sécurité', "Jetons d'API"]);
      expect(links.map((a) => a.getAttribute('href'))).toEqual(['/account', '/account?section=password', '/account?section=security', '/account?section=tokens']);
      expect(links.every((a) => a.querySelector('gbt-icon') !== null)).toBe(true);
    });

    it('shows the profile section by default, and only it', async () => {
      const { el, current, card } = await setup();

      expect(current()).toEqual(['Profil']);
      expect(card('Profil')).toBeDefined();
      expect(card('Mot de passe')).toBeUndefined();
      expect(el().querySelector('fg-api-tokens-list')).toBeNull();
    });

    it('shows the section named in ?section=, and marks it active', async () => {
      const { harness, el, current, card } = await setup('/account?section=password');

      expect(current()).toEqual(['Mot de passe']);
      expect(card('Mot de passe')).toBeDefined();
      expect(card('Profil')).toBeUndefined();
      expect(el().querySelector('fg-api-tokens-list')).toBeNull();

      await harness.navigateByUrl('/account?section=tokens');
      harness.fixture.detectChanges();
      expect(current()).toEqual(["Jetons d'API"]);
      expect(harness.routeDebugElement!.queryAll(By.directive(ApiTokensList))).toHaveLength(1);
      expect(card('Mot de passe')).toBeUndefined();
    });

    it('falls back to the profile for an unknown section, in the page and in the nav', async () => {
      const { current, card } = await setup('/account?section=avance');

      expect(current()).toEqual(['Profil']);
      expect(card('Profil')).toBeDefined();
    });

    it('moves between sections through the nav links', async () => {
      const { el, current, card, settle } = await setup();

      const passwordLink = Array.from(el().querySelectorAll<HTMLAnchorElement>('gbt-nav-tabs a')).find((a) => text(a) === 'Mot de passe')!;
      passwordLink.click();
      await settle();

      expect(TestBed.inject(Router).url).toBe('/account?section=password');
      expect(current()).toEqual(['Mot de passe']);
      expect(card('Mot de passe')).toBeDefined();
      expect(card('Profil')).toBeUndefined();
    });

    it('has at most one primary button in each section', async () => {
      const { el, navigate, type } = await setup();

      const primaries = () => el().querySelectorAll('.gbt-button--primary').length;
      expect(primaries()).toBeLessThanOrEqual(1);
      await navigate('/account?section=password');
      type('Mot de passe actuel', 'old-password');
      type('Nouveau mot de passe', 'new-password');
      type('Confirmer le nouveau mot de passe', 'new-password');
      await navigate('/account?section=password');
      expect(primaries()).toBe(1);
      await navigate('/account?section=security');
      expect(primaries()).toBeLessThanOrEqual(1);
      await navigate('/account?section=tokens');
      expect(primaries()).toBeLessThanOrEqual(1);
    });
  });

  describe('security', () => {
    it('renders the passkeys and the authenticator settings in the "Sécurité" section, marked active, and only there', async () => {
      const { harness, el, current, card, mfaStub } = await setup('/account?section=security');

      expect(current()).toEqual(['Sécurité']);
      expect(harness.routeDebugElement!.queryAll(By.directive(MfaSettings))).toHaveLength(1);
      expect(harness.routeDebugElement!.queryAll(By.directive(PasskeySettings))).toHaveLength(1);
      expect(card("Clés d'accès")).toBeDefined();
      expect(card("Application d'authentification")).toBeDefined();
      expect(mfaStub.status).toHaveBeenCalledTimes(2);
      expect(card('Profil')).toBeUndefined();
      expect(card('Mot de passe')).toBeUndefined();
      expect(el().querySelector('fg-api-tokens-list')).toBeNull();
    });

    it('does not load the factor status until its section is opened', async () => {
      const { harness, navigate, mfaStub } = await setup();

      expect(mfaStub.status).not.toHaveBeenCalled();
      expect(harness.routeDebugElement!.queryAll(By.directive(MfaSettings))).toHaveLength(0);
      expect(harness.routeDebugElement!.queryAll(By.directive(PasskeySettings))).toHaveLength(0);
      await navigate('/account?section=security');
      expect(mfaStub.status).toHaveBeenCalledTimes(2);
    });

    it('has one primary button at most when a form is opened in each card: the second closes the first', async () => {
      // jsdom has no WebAuthn, so give the page a browser that does, then put the real one back.
      const restoreBrowser = stubPasskeyBrowser({ create: vi.fn(), get: vi.fn() });
      onTestFinished(restoreBrowser);
      const { el, settle, button } = await setup('/account?section=security');
      const primaries = () => el().querySelectorAll('button.gbt-button--primary').length;

      button("Ajouter une clé d'accès")!.click();
      await settle();
      expect(primaries()).toBe(1);
      button('Régénérer les codes de secours')!.click();
      await settle();

      expect(primaries()).toBeLessThanOrEqual(1);
      expect(el().querySelector('fg-passkey-settings form')).toBeNull();
      expect(el().querySelector('fg-mfa-settings form')).toBeTruthy();
    });

    it('lists the passkeys before the authenticator app (the recommended factor first)', async () => {
      const { el } = await setup('/account?section=security');

      const headings = Array.from(el().querySelectorAll('gbt-card h2')).map((h) => text(h));
      expect(headings).toEqual(["Clés d'accès", "Application d'authentification"]);
    });

    it('says the double authentication in the page intro', async () => {
      const { el } = await setup();

      expect(text(el().querySelector('.account-page__intro'))).toContain('double authentification');
    });
  });

  describe('profile', () => {
    it('is a "Profil" card with a user icon: avatar, read-only username and the email field', async () => {
      const { el, card, field, harness, meStub, settle } = await setup();

      const profile = card('Profil')!;
      const icon = harness.routeDebugElement!.queryAll(By.css('gbt-card')).find((de) => text(de.nativeElement.querySelector('h2')) === 'Profil')!;
      expect((icon.query(By.directive(Icon))?.componentInstance as Icon | undefined)?.name()).toBe('user');
      expect(profile.querySelector('.gbt-card__description')).not.toBeNull();
      expect(profile.querySelector('gbt-avatar')).not.toBeNull();
      expect(text(profile.querySelector('.account-page__username'))).toBe('alice');
      expect(field("Nom d'utilisateur")).toBeNull();
      expect(text(profile.querySelector('.account-page__identity-meta'))).toBe("Nom d'utilisateur, non modifiable");
      expect(field('Email')?.type).toBe('email');
      expect(field('Email')?.value).toBe('alice@example.com');
      expect(el().querySelector('.account-page__email [role="status"]')).not.toBeNull();

      meStub.username.set('');
      await settle();
      expect(profile.querySelector('.account-page__identity [aria-busy="true"]')).not.toBeNull();
      expect(profile.querySelector('gbt-avatar')).toBeNull();
    });

    it('reads a dotted username as two words for the avatar initials', async () => {
      const { card, meStub, settle } = await setup();

      meStub.username.set('florian.simon');
      await settle();
      expect(text(card('Profil')!.querySelector('gbt-avatar'))).toBe('FS');
      expect(text(card('Profil')!.querySelector('.account-page__username'))).toBe('florian.simon');
    });

    it('says when the user is an administrator', async () => {
      const { card, meStub, settle } = await setup();

      expect(text(card('Profil'))).not.toContain('Super-administrateur');
      meStub.isAdmin.set(true);
      await settle();
      expect(text(card('Profil'))).toContain('Super-administrateur');
    });

    it('saves the email when the field is left, and says so under it (and in a toast)', async () => {
      const { el, commit, settle, meStub, toastStub } = await setup();
      const state = () => el().querySelector('.account-page__email [role="status"]');

      expect(text(state())).toBe('');
      commit('Email', 'alice@exemple.fr');
      await settle();

      expect(meStub.updateEmail).toHaveBeenCalledExactlyOnceWith('alice@exemple.fr');
      expect(text(state())).toBe('Enregistré');
      expect(state()?.getAttribute('data-state')).toBe('saved');
      expect(toastStub.show).toHaveBeenCalledWith('Email mis à jour.');
    });

    it('does not save an unchanged email', async () => {
      const { commit, settle, meStub } = await setup();

      commit('Email', 'alice@example.com');
      await settle();

      expect(meStub.updateEmail).not.toHaveBeenCalled();
    });

    it('explains a blank or malformed email instead of saving it, and clears the error once valid', async () => {
      const { el, commit, settle, meStub } = await setup();
      const error = () => text(el().querySelector('.account-page__email .gbt-input__error'));

      commit('Email', '   ');
      await settle();
      expect(error()).toBe('Indiquez votre adresse email');

      commit('Email', 'alice.example.com');
      await settle();
      expect(error()).toBe('Entrez une adresse email valide');
      expect(meStub.updateEmail).not.toHaveBeenCalled();

      commit('Email', 'alice@example.com');
      await settle();
      expect(error()).toBeUndefined();
    });

    it('shows the saved email again when the save fails, says so and toasts the error', async () => {
      const { el, commit, settle, field, meStub, toastStub } = await setup();
      const refused = new Subject<Me>();
      meStub.updateEmail.mockReturnValue(refused);
      const state = () => el().querySelector('.account-page__email [role="status"]');

      commit('Email', 'alice@exemple.fr');
      await settle();
      expect(text(state())).toBe('Enregistrement…');
      expect(field('Email')?.value).toBe('alice@exemple.fr');

      refused.error({ status: 500 });
      await settle();

      expect(field('Email')?.value).toBe('alice@example.com');
      expect(text(state())).toBe('Non enregistré');
      expect(state()?.getAttribute('data-state')).toBe('error');
      expect(toastStub.show).toHaveBeenCalledWith("Impossible de mettre à jour l'email.", 'error');

      meStub.updateEmail.mockReturnValue(of({ ...ME, email: 'alice@exemple.fr' }));
      commit('Email', 'alice@exemple.fr');
      await settle();
      expect(meStub.updateEmail).toHaveBeenCalledTimes(2);
      expect(text(state())).toBe('Enregistré');
    });

    it('logs out and goes to the login page from "Déconnexion"', async () => {
      const { button, settle, authStub } = await setup();

      const logout = button('Déconnexion')!;
      expect(logout.classList).not.toContain('gbt-button--primary');
      expect(logout.classList).toContain('gbt-button--secondary'); // signing out isn't destructive, so it is a bordered neutral button rather than quiet text
      logout.click();
      await settle();

      expect(authStub.logout).toHaveBeenCalledOnce();
      expect(TestBed.inject(Router).url).toBe('/login');
    });
  });

  describe('password', () => {
    async function passwordSection() {
      return setup('/account?section=password');
    }

    it('is a "Mot de passe" card with a lock icon and three labelled password fields', async () => {
      const { card, field, harness } = await passwordSection();

      const password = card('Mot de passe')!;
      expect(password.querySelector('.gbt-card__description')).not.toBeNull();
      const de = harness.routeDebugElement!.queryAll(By.css('gbt-card')).find((c) => text(c.nativeElement.querySelector('h2')) === 'Mot de passe')!;
      expect((de.query(By.directive(Icon))?.componentInstance as Icon | undefined)?.name()).toBe('lock');
      expect(field('Mot de passe actuel')?.type).toBe('password');
      expect(field('Mot de passe actuel')?.autocomplete).toBe('current-password');
      expect(field('Nouveau mot de passe')?.autocomplete).toBe('new-password');
      expect(field('Confirmer le nouveau mot de passe')?.autocomplete).toBe('new-password');
    });

    it('hints the minimum length under the new password, and marks it met', async () => {
      const { el, type, settle } = await passwordSection();
      const hint = () => el().querySelector('.account-page__length-hint');

      expect(text(hint())).toBe('8 caractères minimum');
      expect(hint()?.getAttribute('data-state')).toBeNull();
      type('Nouveau mot de passe', 'long-enough');
      await settle();
      expect(hint()?.getAttribute('data-state')).toBe('met');
    });

    it('keeps the button secondary until the three fields are filled, then makes it the primary action', async () => {
      const { button, type, settle } = await passwordSection();

      expect(button('Changer le mot de passe')!.classList).toContain('gbt-button--secondary');
      type('Mot de passe actuel', 'old-password');
      type('Nouveau mot de passe', 'new-password');
      await settle();
      expect(button('Changer le mot de passe')!.classList).toContain('gbt-button--secondary');
      type('Confirmer le nouveau mot de passe', 'new-password');
      await settle();
      expect(button('Changer le mot de passe')!.classList).toContain('gbt-button--primary');
    });

    it('refuses mismatched passwords with an error on the confirmation field, without sending anything', async () => {
      const { el, type, settle, button, meStub } = await passwordSection();

      type('Mot de passe actuel', 'old-password');
      type('Nouveau mot de passe', 'new-password');
      type('Confirmer le nouveau mot de passe', 'other-password');
      button('Changer le mot de passe')!.click();
      await settle();

      expect(meStub.changePassword).not.toHaveBeenCalled();
      expect(text(el().querySelector('.account-page__confirm .gbt-input__error'))).toBe('Les nouveaux mots de passe ne correspondent pas.');

      type('Confirmer le nouveau mot de passe', 'new-password');
      await settle();
      expect(el().querySelector('.account-page__confirm .gbt-input__error')).toBeNull();
    });

    it('refuses a new password shorter than 8 characters with an error on that field', async () => {
      const { el, type, settle, button, meStub } = await passwordSection();

      type('Mot de passe actuel', 'old-password');
      type('Nouveau mot de passe', 'short');
      type('Confirmer le nouveau mot de passe', 'short');
      button('Changer le mot de passe')!.click();
      await settle();

      expect(meStub.changePassword).not.toHaveBeenCalled();
      expect(text(el().querySelector('.account-page__new .gbt-input__error'))).toBe('Le mot de passe doit contenir au moins 8 caractères.');
    });

    it('changes the password, empties the fields and says so (inline status and toast)', async () => {
      const { el, type, settle, button, field, meStub, toastStub } = await passwordSection();

      type('Mot de passe actuel', 'old-password');
      type('Nouveau mot de passe', 'new-password');
      type('Confirmer le nouveau mot de passe', 'new-password');
      await settle();
      button('Changer le mot de passe')!.click();
      await settle();

      expect(meStub.changePassword).toHaveBeenCalledExactlyOnceWith('old-password', 'new-password');
      const status = el().querySelector('.account-page__password-status [role="status"]');
      expect(status?.getAttribute('role')).toBe('status');
      expect(text(status)).toBe('Mot de passe modifié.');
      expect(status?.getAttribute('data-state')).toBe('saved');
      expect(toastStub.show).toHaveBeenCalledWith('Mot de passe modifié.');
      expect(field('Mot de passe actuel')?.value).toBe('');
      expect(field('Nouveau mot de passe')?.value).toBe('');
      expect(field('Confirmer le nouveau mot de passe')?.value).toBe('');
    });

    it('stores the fresh token the server returns, so the session outlives the password change', async () => {
      const { type, settle, button, meStub, authStub } = await passwordSection();
      const done = new Subject<{ token?: string } | null>();
      meStub.changePassword.mockReturnValue(done);

      type('Mot de passe actuel', 'old-password');
      type('Nouveau mot de passe', 'new-password');
      type('Confirmer le nouveau mot de passe', 'new-password');
      button('Changer le mot de passe')!.click();
      await settle();
      expect(authStub.setToken).not.toHaveBeenCalled();
      done.next({ token: 'fresh-jwt' });
      done.complete();
      await settle();

      expect(authStub.setToken).toHaveBeenCalledExactlyOnceWith('fresh-jwt');
      expect(authStub.logout).not.toHaveBeenCalled();
    });

    it('keeps the current token when the server returns none (an older server)', async () => {
      const { type, settle, button, authStub, toastStub } = await passwordSection();

      type('Mot de passe actuel', 'old-password');
      type('Nouveau mot de passe', 'new-password');
      type('Confirmer le nouveau mot de passe', 'new-password');
      button('Changer le mot de passe')!.click();
      await settle();

      expect(toastStub.show).toHaveBeenCalledWith('Mot de passe modifié.');
      expect(authStub.setToken).not.toHaveBeenCalled();
      expect(authStub.logout).not.toHaveBeenCalled();
    });

    it('stores no token when the change is refused', async () => {
      const { type, settle, button, meStub, authStub } = await passwordSection();
      const refused = new Subject<{ token?: string } | null>();
      meStub.changePassword.mockReturnValue(refused);

      type('Mot de passe actuel', 'wrong-password');
      type('Nouveau mot de passe', 'new-password');
      type('Confirmer le nouveau mot de passe', 'new-password');
      button('Changer le mot de passe')!.click();
      await settle();
      refused.error({ status: 400 });
      await settle();

      expect(authStub.setToken).not.toHaveBeenCalled();
      expect(authStub.logout).not.toHaveBeenCalled();
    });

    it('sends the change once while it runs', async () => {
      const { el, type, settle, button, meStub } = await passwordSection();
      const pending = new Subject<{ token?: string } | null>();
      meStub.changePassword.mockReturnValue(pending);

      type('Mot de passe actuel', 'old-password');
      type('Nouveau mot de passe', 'new-password');
      type('Confirmer le nouveau mot de passe', 'new-password');
      await settle();
      const submit = button('Changer le mot de passe')!;
      submit.click();
      await settle();
      expect(submit.getAttribute('aria-busy')).toBe('true');
      submit.click();
      el().querySelector('form.account-page__password-form')!.dispatchEvent(new Event('submit', { cancelable: true }));
      await settle();

      expect(meStub.changePassword).toHaveBeenCalledOnce();
    });

    it('says the current password is wrong when the server refuses it, keeping the fields filled in', async () => {
      const { el, type, settle, button, field, meStub, toastStub } = await passwordSection();
      const refused = new Subject<{ token?: string } | null>();
      meStub.changePassword.mockReturnValue(refused);

      type('Mot de passe actuel', 'wrong-password');
      type('Nouveau mot de passe', 'new-password');
      type('Confirmer le nouveau mot de passe', 'new-password');
      button('Changer le mot de passe')!.click();
      await settle();
      refused.error({ status: 400 });
      await settle();

      const status = el().querySelector('.account-page__password-status [role="status"]');
      expect(text(status)).toBe('Mot de passe actuel incorrect.');
      expect(status?.getAttribute('data-state')).toBe('error');
      expect(toastStub.show).toHaveBeenCalledWith('Impossible de modifier le mot de passe.', 'error');
      expect(field('Nouveau mot de passe')?.value).toBe('new-password');
    });

    it('says the change could not be made when the server fails for another reason', async () => {
      const { el, type, settle, button, meStub } = await passwordSection();
      const refused = new Subject<{ token?: string } | null>();
      meStub.changePassword.mockReturnValue(refused);

      type('Mot de passe actuel', 'old-password');
      type('Nouveau mot de passe', 'new-password');
      type('Confirmer le nouveau mot de passe', 'new-password');
      button('Changer le mot de passe')!.click();
      await settle();
      refused.error({ status: 500 });
      await settle();

      expect(text(el().querySelector('.account-page__password-status'))).toBe("Le mot de passe n'a pas pu être modifié. Réessayez plus tard.");
    });

    it('submits with Enter from any field', async () => {
      const { el, type, settle, meStub } = await passwordSection();

      type('Mot de passe actuel', 'old-password');
      type('Nouveau mot de passe', 'new-password');
      type('Confirmer le nouveau mot de passe', 'new-password');
      el().querySelector('form.account-page__password-form')!.dispatchEvent(new Event('submit', { cancelable: true }));
      await settle();

      expect(meStub.changePassword).toHaveBeenCalledOnce();
    });
  });

  describe('API tokens', () => {
    it('renders the token list in its section', async () => {
      const { harness, tokensStub } = await setup('/account?section=tokens');

      expect(harness.routeDebugElement!.queryAll(By.directive(ApiTokensList))).toHaveLength(1);
      expect(tokensStub.list).toHaveBeenCalled();
    });
  });
});
