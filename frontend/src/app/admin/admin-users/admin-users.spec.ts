import { HttpErrorResponse } from '@angular/common/http';
import { signal, WritableSignal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter, Router } from '@angular/router';
import { Subject } from 'rxjs';
import { AdminUser, AdminUsersService, InviteResult, PasswordResetResult } from '../admin-users.service';
import { AuthService } from '../../auth/auth.service';
import { MeService } from '../../shell/me.service';
import { PageTitleService } from '../../shell/page-title.service';
import { formatDateTime, GbtToastService } from '@masmarino/gabarit';
import { AdminUsers } from './admin-users';

const absoluteDateTime = (iso: string) => formatDateTime(iso, 'fr', { day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' });

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();
const minutesAgo = (minutes: number) => new Date(Date.now() - minutes * 60_000).toISOString();
const inHours = (hours: number) => new Date(Date.now() + hours * 3_600_000).toISOString();

const user = (overrides: Partial<AdminUser> & Pick<AdminUser, 'id' | 'username'>): AdminUser => ({
  email: `${overrides.username}@example.com`,
  isAdmin: false,
  createdAt: minutesAgo(60 * 24 * 5),
  state: 'active',
  invitationExpiresAt: null,
  mfaEnabled: true,
  ...overrides,
});

const ADMIN = user({ id: 'u1', username: 'alice', isAdmin: true, createdAt: minutesAgo(60 * 24 * 90) });
const ACTIVE_MFA = user({ id: 'u2', username: 'bob', createdAt: minutesAgo(60 * 24 * 20) });
const ACTIVE_NO_MFA = user({ id: 'u3', username: 'carol', mfaEnabled: false, createdAt: minutesAgo(60 * 24 * 10) });
const PENDING = user({ id: 'u4', username: 'dave', state: 'invited', mfaEnabled: false, invitationExpiresAt: inHours(20), createdAt: minutesAgo(180) });
const EXPIRED = user({ id: 'u5', username: 'erin', state: 'invited', mfaEnabled: false, invitationExpiresAt: minutesAgo(60 * 30), createdAt: minutesAgo(60 * 24 * 3) });
const USERS = [ADMIN, ACTIVE_MFA, ACTIVE_NO_MFA, PENDING, EXPIRED];

const failure = (status: number, error?: string) => new HttpErrorResponse({ status, error: error === undefined ? null : { error } });

describe('AdminUsers', () => {
  let listRequests: Subject<AdminUser[]>[];
  let resendRequests: Subject<InviteResult>[];
  let resetRequests: Subject<void>[];
  let passwordResetRequests: Subject<PasswordResetResult>[];
  let setAdminRequests: Subject<void>[];
  let usersStub: {
    list: ReturnType<typeof vi.fn>;
    resend: ReturnType<typeof vi.fn>;
    resetMfa: ReturnType<typeof vi.fn>;
    resetPassword: ReturnType<typeof vi.fn>;
    setAdmin: ReturnType<typeof vi.fn>;
    invite: ReturnType<typeof vi.fn>;
  };
  let meStub: { id: WritableSignal<string>; isAdmin: WritableSignal<boolean>; load: ReturnType<typeof vi.fn> };
  let toastStub: { show: ReturnType<typeof vi.fn> };
  let authStub: { logout: ReturnType<typeof vi.fn> };
  let pageTitleStub: { set: ReturnType<typeof vi.fn> };

  function create() {
    listRequests = [];
    resendRequests = [];
    resetRequests = [];
    passwordResetRequests = [];
    setAdminRequests = [];
    usersStub = {
      list: vi.fn(() => {
        const request = new Subject<AdminUser[]>();
        listRequests.push(request);
        return request;
      }),
      resend: vi.fn(() => {
        const request = new Subject<InviteResult>();
        resendRequests.push(request);
        return request;
      }),
      resetMfa: vi.fn(() => {
        const request = new Subject<void>();
        resetRequests.push(request);
        return request;
      }),
      resetPassword: vi.fn(() => {
        const request = new Subject<PasswordResetResult>();
        passwordResetRequests.push(request);
        return request;
      }),
      setAdmin: vi.fn(() => {
        const request = new Subject<void>();
        setAdminRequests.push(request);
        return request;
      }),
      invite: vi.fn(() => new Subject<InviteResult>()),
    };
    meStub = { id: signal('u1'), isAdmin: signal(true), load: vi.fn() };
    toastStub = { show: vi.fn() };
    authStub = { logout: vi.fn() };
    pageTitleStub = { set: vi.fn() };
    TestBed.configureTestingModule({
      providers: [
        provideRouter([]),
        { provide: AdminUsersService, useValue: usersStub },
        { provide: GbtToastService, useValue: toastStub },
        { provide: AuthService, useValue: authStub },
        { provide: PageTitleService, useValue: pageTitleStub },
        { provide: MeService, useValue: meStub },
      ],
    });
    const fixture = TestBed.createComponent(AdminUsers);
    const el = fixture.nativeElement as HTMLElement;
    const navigate = vi.spyOn(TestBed.inject(Router), 'navigateByUrl').mockResolvedValue(true);
    const settle = async () => {
      await fixture.whenStable();
      fixture.detectChanges();
    };
    return { fixture, component: fixture.componentInstance, el, settle, navigate };
  }

  async function loaded(users: AdminUser[] = USERS) {
    const context = create();
    context.fixture.detectChanges();
    listRequests[0].next(users);
    await context.settle();
    return context;
  }

  const rows = (el: HTMLElement) => Array.from(el.querySelectorAll<HTMLElement>('gbt-list-card ul.admin-users__rows > li > gbt-list-row'));
  const names = (el: HTMLElement) => rows(el).map((row) => text(row.querySelector('.gbt-user-chip__name')));
  const rowOf = (el: HTMLElement, username: string) => rows(el).find((row) => text(row.querySelector('.gbt-user-chip__name')) === username)!;
  const badges = (row: HTMLElement) => Array.from(row.querySelectorAll('gbt-badge')).map((badge) => text(badge));
  const badge = (row: HTMLElement, label: string) => Array.from(row.querySelectorAll('gbt-badge')).find((b) => text(b) === label);
  const variantOf = (b: Element | undefined) => b?.querySelector('.gbt-badge')?.getAttribute('data-variant');
  const button = (root: ParentNode, label: string) => Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === label);
  const menuTrigger = (row: HTMLElement) => row.querySelector<HTMLButtonElement>('gbt-menu button[aria-haspopup="menu"]');
  const menuItems = (row: HTMLElement) => Array.from(row.querySelectorAll<HTMLElement>('[role="menuitem"]')).map((item) => text(item));

  async function pick(context: Awaited<ReturnType<typeof loaded>>, username: string, label: string) {
    const row = rowOf(context.el, username);
    menuTrigger(row)!.click();
    await context.settle();
    Array.from(row.querySelectorAll<HTMLElement>('[role="menuitem"]'))
      .find((item) => text(item) === label)!
      .click();
    await context.settle();
  }

  describe('the page', () => {
    it('sets the page title and loads the users once', async () => {
      const { fixture } = create();
      fixture.detectChanges();

      expect(pageTitleStub.set).toHaveBeenCalledWith('Utilisateurs');
      expect(usersStub.list).toHaveBeenCalledTimes(1);
    });

    it('is a "Utilisateurs" page whose one primary action invites a user', async () => {
      const { el } = await loaded();

      expect(text(el.querySelector('gbt-page-header h1'))).toBe('Utilisateurs');
      expect(el.querySelector('gbt-page-layout')!.getAttribute('data-width')).toBe('wide');
      const primary = Array.from(el.querySelectorAll('.gbt-button--primary'));
      expect(primary.map((b) => text(b))).toEqual(['Inviter un utilisateur']);
    });

    it('counts the accounts and the pending invitations in the header', async () => {
      const { el } = await loaded();
      expect(text(el.querySelector('.admin-users__summary'))).toBe('5 comptes · 2 invitations en attente');
    });

    it('speaks in the singular, and says nothing about invitations when none is pending', async () => {
      const single = await loaded([ADMIN]);
      expect(text(single.el.querySelector('.admin-users__summary'))).toBe('1 compte');
      TestBed.resetTestingModule();
      const one = await loaded([ADMIN, PENDING]);
      expect(text(one.el.querySelector('.admin-users__summary'))).toBe('2 comptes · 1 invitation en attente');
    });

    it('shows skeleton rows while loading, and no rows nor summary', () => {
      const { fixture, el } = create();
      fixture.detectChanges();

      expect(el.querySelector('[aria-busy="true"]')).not.toBeNull();
      expect(rows(el)).toHaveLength(0);
      expect(text(el.querySelector('.admin-users__summary'))).toBe('');
    });

    it('mounts the toolbar from the first paint, idle (inert) while loading, live once the list is there: nothing shifts', async () => {
      const context = create();
      context.fixture.detectChanges();
      const toolbar = () => context.el.querySelector('gbt-list-toolbar')!;
      const before = toolbar();
      expect(before).not.toBeNull();
      expect(before.hasAttribute('inert')).toBe(true);

      listRequests[0].next(USERS);
      await context.settle();
      expect(toolbar()).toBe(before);
      expect(toolbar().hasAttribute('inert')).toBe(false);
    });

    it('keeps the toolbar (idle) when the first load fails, and while retrying', async () => {
      const context = create();
      context.fixture.detectChanges();
      listRequests[0].error(failure(500));
      await context.settle();
      expect(context.el.querySelector('gbt-list-toolbar')?.hasAttribute('inert')).toBe(true);
    });
  });

  describe('the list', () => {
    it('lists everybody, the newest account first', async () => {
      const { el } = await loaded();
      expect(names(el)).toEqual(['dave', 'erin', 'carol', 'bob', 'alice']);
    });

    it('gives each row its avatar, name and e-mail address', async () => {
      const { el } = await loaded();
      const row = rowOf(el, 'bob');

      expect(row.querySelector('gbt-user-chip gbt-avatar')).not.toBeNull();
      expect(text(row.querySelector('.admin-users__email'))).toBe('bob@example.com');
    });

    it('marks administrators, and the signed-in account as "Vous"', async () => {
      const { el } = await loaded();

      expect(badges(rowOf(el, 'alice'))).toContain('Super-administrateur');
      expect(badges(rowOf(el, 'alice'))).toContain('Vous');
      expect(badges(rowOf(el, 'bob'))).not.toContain('Super-administrateur');
      expect(badges(rowOf(el, 'bob'))).not.toContain('Vous');
    });

    it("links each name to the user's page, except the signed-in administrator's own", async () => {
      const { el } = await loaded();

      for (const [username, id] of [['bob', 'u2'], ['carol', 'u3'], ['dave', 'u4'], ['erin', 'u5']] as const) {
        const link = rowOf(el, username).querySelector<HTMLAnchorElement>('a.admin-users__user-link');
        expect(link?.getAttribute('href'), username).toBe(`/admin/users/${id}`);
        expect(text(link?.querySelector('.gbt-user-chip__name')), username).toBe(username);
      }
      expect(rowOf(el, 'alice').querySelector('a')).toBeNull();
      expect(text(rowOf(el, 'alice').querySelector('.gbt-user-chip__name'))).toBe('alice');
    });

    it('keeps every row action on the list next to the link', async () => {
      const { el } = await loaded();
      expect(menuTrigger(rowOf(el, 'bob'))).not.toBeNull();
      expect(rowOf(el, 'bob').querySelector('a.admin-users__user-link')).not.toBeNull();
    });

    it('says whether the account is active or invited, and whether the invitation has expired', async () => {
      const { el } = await loaded();

      expect(badge(rowOf(el, 'bob'), 'Actif')).toBeDefined();
      expect(variantOf(badge(rowOf(el, 'bob'), 'Actif'))).toBe('success');
      expect(variantOf(badge(rowOf(el, 'dave'), 'Invitation en attente'))).toBe('warning');
      expect(variantOf(badge(rowOf(el, 'erin'), 'Invitation expirée'))).toBe('error');
      expect(badge(rowOf(el, 'erin'), 'Invitation en attente')).toBeUndefined();
    });

    it('says whether the double authentication is set up, for the accounts that can have it', async () => {
      const { el } = await loaded();

      expect(variantOf(badge(rowOf(el, 'bob'), 'Double authentification active'))).toBe('success');
      expect(variantOf(badge(rowOf(el, 'carol'), 'Non configurée'))).toBe('neutral');
      expect(badges(rowOf(el, 'dave')).some((label) => label?.includes('authentification') || label === 'Non configurée')).toBe(false);
    });

    it('shows when the account was created, the exact date on hover', async () => {
      const { el } = await loaded();
      const time = rowOf(el, 'bob').querySelector<HTMLTimeElement>('.admin-users__created time')!;

      expect(text(time)).toBe('il y a 20 j');
      expect(time.title).toBe(absoluteDateTime(ACTIVE_MFA.createdAt));
      expect(time.getAttribute('datetime')).toBe(ACTIVE_MFA.createdAt);
    });

    it('shows a pending invitation\'s expiry, and an expired one as expired', async () => {
      const { el } = await loaded();

      expect(text(rowOf(el, 'dave').querySelector('.admin-users__expiry'))).toBe(`expire le ${absoluteDateTime(PENDING.invitationExpiresAt!)}`);
      expect(text(rowOf(el, 'erin').querySelector('.admin-users__expiry'))).toBe('a expiré hier');
      expect(rowOf(el, 'bob').querySelector('.admin-users__expiry')).toBeNull();
    });
  });

  describe('search, filter and sort', () => {
    async function typeSearch(context: Awaited<ReturnType<typeof loaded>>, value: string) {
      const input = context.el.querySelector<HTMLInputElement>('gbt-list-toolbar input')!;
      input.value = value;
      input.dispatchEvent(new Event('input'));
      await context.settle();
    }

    it('searches the usernames and the e-mail addresses, whatever the case', async () => {
      const context = await loaded([ADMIN, user({ id: 'u9', username: 'zoe', email: 'Bobby@Corp.example' }), ACTIVE_MFA]);

      await typeSearch(context, 'BOB');

      expect(names(context.el)).toEqual(['zoe', 'bob']);
    });

    it('says nothing matches when nothing does, and offers no rows', async () => {
      const context = await loaded();

      await typeSearch(context, 'zzz');

      expect(rows(context.el)).toHaveLength(0);
      expect(text(context.el.querySelector('[list-card-message]'))).toBe('Aucun utilisateur ne correspond à cette recherche');
    });

    it('filters on the pending invitations (expired ones included), with the counts on the tabs', async () => {
      const { el, settle } = await loaded();
      const tabs = Array.from(el.querySelectorAll<HTMLElement>('gbt-segmented-control [role="radio"]'));

      expect(tabs.map((tab) => text(tab))).toEqual(['Tous (5)', 'Invitations en attente (2)']);
      tabs[1].click();
      await settle();

      expect(names(el)).toEqual(['dave', 'erin']);
      tabs[0].click();
      await settle();
      expect(names(el)).toHaveLength(5);
    });

    it('combines the filter and the search', async () => {
      const context = await loaded();
      Array.from(context.el.querySelectorAll<HTMLElement>('gbt-segmented-control [role="radio"]'))[1].click();
      await context.settle();

      await typeSearch(context, 'dav');

      expect(names(context.el)).toEqual(['dave']);
    });

    it('says there is no pending invitation when the filter finds none', async () => {
      const context = await loaded([ADMIN, ACTIVE_MFA]);
      Array.from(context.el.querySelectorAll<HTMLElement>('gbt-segmented-control [role="radio"]'))[1].click();
      await context.settle();

      expect(text(context.el.querySelector('[list-card-message]'))).toBe('Aucune invitation en attente');
    });

    it('sorts by username when asked, and reverses the direction', async () => {
      const context = await loaded();
      const sortSelect = context.fixture.componentInstance as unknown as { sortValue: { set(v: string): void }; direction: { set(v: string): void } };

      sortSelect.sortValue.set('username');
      sortSelect.direction.set('asc');
      await context.settle();
      expect(names(context.el)).toEqual(['alice', 'bob', 'carol', 'dave', 'erin']);

      sortSelect.direction.set('desc');
      await context.settle();
      expect(names(context.el)).toEqual(['erin', 'dave', 'carol', 'bob', 'alice']);
    });
  });

  describe('the row actions', () => {
    it('gives every row a menu: the administrator flag applies to every account', async () => {
      const { el } = await loaded();

      for (const username of ['alice', 'bob', 'carol', 'dave', 'erin']) {
        expect(menuTrigger(rowOf(el, username)), username).not.toBeNull();
      }
      expect(el.querySelector('.admin-users__menu-slot')).toBeNull();
    });

    it('offers "Renvoyer l\'invitation" to the invited, the password reset to the active accounts but the signed-in one, the MFA reset to accounts that have one, and the administrator flag to all', async () => {
      const context = await loaded();
      for (const [username, expected] of [
        ['dave', ["Renvoyer l'invitation", 'Nommer super-administrateur']],
        ['erin', ["Renvoyer l'invitation", 'Nommer super-administrateur']],
        ['bob', ['Réinitialiser le mot de passe', 'Réinitialiser la double authentification', 'Nommer super-administrateur']],
        ['carol', ['Réinitialiser le mot de passe', 'Nommer super-administrateur']],
        ['alice', ['Réinitialiser la double authentification', "Retirer les droits de super-administrateur"]],
      ] as const) {
        const row = rowOf(context.el, username);
        menuTrigger(row)!.click();
        await context.settle();
        expect(menuItems(row), username).toEqual(expected);
        menuTrigger(row)!.click();
        await context.settle();
      }
    });

    it('names each menu after its user for assistive technology', async () => {
      const { el } = await loaded();
      expect(menuTrigger(rowOf(el, 'dave'))!.getAttribute('aria-label')).toBe('Actions pour dave');
    });
  });

  describe('resending an invitation', () => {
    const RENEWED = { ...PENDING, invitationExpiresAt: inHours(24) };

    it('resends to that user, shows the row busy meanwhile, and refuses a second request', async () => {
      const context = await loaded();

      await pick(context, 'dave', "Renvoyer l'invitation");

      expect(usersStub.resend).toHaveBeenCalledWith('u4');
      expect(rowOf(context.el, 'dave').getAttribute('aria-busy')).toBe('true');
      context.component.resend(PENDING);
      expect(usersStub.resend).toHaveBeenCalledTimes(1);
    });

    it('says the invitation went out, renews the row and clears the busy state', async () => {
      const context = await loaded();
      await pick(context, 'dave', "Renvoyer l'invitation");

      resendRequests[0].next({ user: RENEWED, emailSent: true });
      resendRequests[0].complete();
      await context.settle();

      expect(toastStub.show).toHaveBeenCalledWith('Invitation renvoyée à dave@example.com.');
      expect(rowOf(context.el, 'dave').getAttribute('aria-busy')).toBeNull();
      expect(text(rowOf(context.el, 'dave').querySelector('.admin-users__expiry'))).toBe(`expire le ${absoluteDateTime(RENEWED.invitationExpiresAt!)}`);
      expect(context.el.querySelector('fg-link-mail-failed')).toBeNull();
    });

    it('turns an expired invitation back into a pending one', async () => {
      const context = await loaded();
      await pick(context, 'erin', "Renvoyer l'invitation");

      resendRequests[0].next({ user: { ...EXPIRED, invitationExpiresAt: inHours(24) }, emailSent: true });
      await context.settle();

      expect(badge(rowOf(context.el, 'erin'), 'Invitation en attente')).toBeDefined();
      expect(badge(rowOf(context.el, 'erin'), 'Invitation expirée')).toBeUndefined();
    });

    it('shows the reason and the activation link when the mail could not be sent, until dismissed', async () => {
      const context = await loaded();
      await pick(context, 'dave', "Renvoyer l'invitation");

      resendRequests[0].next({ user: RENEWED, emailSent: false, emailError: 'connection refused', activationUrl: 'http://localhost:4200/activate#token=abc' });
      await context.settle();

      const panel = context.el.querySelector('.admin-users__mail-failed')!;
      expect(text(panel)).toContain("Le mail n'a pas pu être envoyé");
      expect(text(panel)).toContain('connection refused');
      expect(text(panel.querySelector('code'))).toBe('http://localhost:4200/activate#token=abc');
      expect(text(panel)).toContain('dave');
      expect(toastStub.show).not.toHaveBeenCalledWith('Invitation renvoyée à dave@example.com.');

      expect(context.el.querySelectorAll('.admin-users__mail-failed gbt-alert')).toHaveLength(1);
      expect(document.activeElement).toBe(panel.querySelector('.link-mail-failed'));

      panel.querySelector<HTMLButtonElement>('button[aria-label="Fermer"]')!.click();
      await context.settle();
      expect(context.el.querySelector('.admin-users__mail-failed')).toBeNull();
    });

    it('says so, and frees the row, when the resend fails', async () => {
      const context = await loaded();
      await pick(context, 'dave', "Renvoyer l'invitation");

      resendRequests[0].error(failure(500));
      await context.settle();

      expect(toastStub.show).toHaveBeenCalledWith("L'invitation n'a pas pu être renvoyée. Réessayez plus tard.", 'error');
      expect(rowOf(context.el, 'dave').getAttribute('aria-busy')).toBeNull();
      expect(usersStub.list).toHaveBeenCalledTimes(1);
    });

    it('reloads the list when the account turns out to be active already (400)', async () => {
      const context = await loaded();
      await pick(context, 'dave', "Renvoyer l'invitation");

      resendRequests[0].error(failure(400));
      await context.settle();

      expect(toastStub.show).toHaveBeenCalledWith("Ce compte est déjà activé : il n'y a plus d'invitation à renvoyer.", 'error');
      expect(usersStub.list).toHaveBeenCalledTimes(2);
      listRequests[1].next([ADMIN, { ...PENDING, state: 'active', invitationExpiresAt: null }]);
      await context.settle();
      expect(badge(rowOf(context.el, 'dave'), 'Actif')).toBeDefined();
    });
  });

  describe('resetting the double authentication', () => {
    it('asks first: the dialog says the user will be signed out everywhere and must set it up again', async () => {
      const context = await loaded();

      await pick(context, 'bob', 'Réinitialiser la double authentification');

      const dialog = context.el.querySelector('gbt-confirm-danger-modal')!;
      expect(dialog).not.toBeNull();
      expect(text(dialog.querySelector('h2'))).toBe('Réinitialiser la double authentification');
      const message = text(dialog.querySelector('.gbt-confirm-danger-modal__message'))!;
      expect(message).toContain('bob');
      expect(message).toContain('déconnecté');
      expect(message).toContain('configurer à nouveau la double authentification');
      expect(message).not.toContain('votre propre compte');
      expect(usersStub.resetMfa).not.toHaveBeenCalled();
    });

    it('does nothing when cancelled', async () => {
      const context = await loaded();
      await pick(context, 'bob', 'Réinitialiser la double authentification');

      button(context.el.querySelector('gbt-confirm-danger-modal')!, 'Annuler')!.click();
      await context.settle();

      expect(context.el.querySelector('gbt-confirm-danger-modal')).toBeNull();
      expect(usersStub.resetMfa).not.toHaveBeenCalled();
    });

    it('resets, shows the dialog busy, then updates the row and says so', async () => {
      const context = await loaded();
      await pick(context, 'bob', 'Réinitialiser la double authentification');

      button(context.el.querySelector('gbt-confirm-danger-modal')!, 'Réinitialiser')!.click();
      await context.settle();
      expect(usersStub.resetMfa).toHaveBeenCalledWith('u2');
      expect(context.el.querySelector('gbt-confirm-danger-modal button[aria-busy="true"]')).not.toBeNull();

      resetRequests[0].next();
      resetRequests[0].complete();
      await context.settle();

      expect(context.el.querySelector('gbt-confirm-danger-modal')).toBeNull();
      expect(toastStub.show).toHaveBeenCalledWith('Double authentification réinitialisée.');
      expect(badge(rowOf(context.el, 'bob'), 'Non configurée')).toBeDefined();
      expect(badge(rowOf(context.el, 'bob'), 'Double authentification active')).toBeUndefined();
      menuTrigger(rowOf(context.el, 'bob'))!.click();
      await context.settle();
      expect(menuItems(rowOf(context.el, 'bob'))).not.toContain('Réinitialiser la double authentification');
      expect(authStub.logout).not.toHaveBeenCalled();
    });

    it('keeps the account as it was, and says so, when the reset fails', async () => {
      const context = await loaded();
      await pick(context, 'bob', 'Réinitialiser la double authentification');
      button(context.el.querySelector('gbt-confirm-danger-modal')!, 'Réinitialiser')!.click();
      await context.settle();

      resetRequests[0].error(failure(500));
      await context.settle();

      expect(toastStub.show).toHaveBeenCalledWith("La double authentification n'a pas pu être réinitialisée. Réessayez plus tard.", 'error');
      expect(context.el.querySelector('gbt-confirm-danger-modal')).toBeNull();
      expect(badge(rowOf(context.el, 'bob'), 'Double authentification active')).toBeDefined();
    });

    it('warns an administrator resetting their own account, then signs them out and back to the login', async () => {
      const context = await loaded();
      await pick(context, 'alice', 'Réinitialiser la double authentification');

      const message = text(context.el.querySelector('gbt-confirm-danger-modal .gbt-confirm-danger-modal__message'))!;
      expect(message).toContain('votre propre compte');
      expect(message).toContain('déconnecté');

      button(context.el.querySelector('gbt-confirm-danger-modal')!, 'Réinitialiser')!.click();
      await context.settle();
      resetRequests[0].next();
      resetRequests[0].complete();
      await context.settle();

      expect(authStub.logout).toHaveBeenCalledTimes(1);
      expect(context.navigate).toHaveBeenCalledWith('/login');
    });

    it('does not sign the administrator out when their own reset fails', async () => {
      const context = await loaded();
      await pick(context, 'alice', 'Réinitialiser la double authentification');
      button(context.el.querySelector('gbt-confirm-danger-modal')!, 'Réinitialiser')!.click();
      await context.settle();

      resetRequests[0].error(failure(500));
      await context.settle();

      expect(authStub.logout).not.toHaveBeenCalled();
      expect(context.navigate).not.toHaveBeenCalled();
    });
  });

  describe('resetting a password', () => {
    const RESET_URL = 'http://localhost:4200/reset-password#token=abc';
    const dialog = (el: HTMLElement) => el.querySelector('gbt-confirm-danger-modal')!;
    const message = (el: HTMLElement) => text(dialog(el).querySelector('.gbt-confirm-danger-modal__message'))!;

    async function confirmed(username = 'bob') {
      const context = await loaded();
      await pick(context, username, 'Réinitialiser le mot de passe');
      button(dialog(context.el), 'Réinitialiser')!.click();
      await context.settle();
      return context;
    }

    it('asks first: the dialog says the current password stops working at once and a link is mailed', async () => {
      const context = await loaded();

      await pick(context, 'bob', 'Réinitialiser le mot de passe');

      expect(text(dialog(context.el).querySelector('h2'))).toBe('Réinitialiser le mot de passe');
      expect(message(context.el)).toBe(
        "Le mot de passe actuel de bob cessera de fonctionner immédiatement, et bob sera déconnecté de tous ses appareils. Il recevra par e-mail un lien valable 1 heure pour en choisir un nouveau : il ne pourra pas se connecter avant de l'avoir utilisé.",
      );
      expect(usersStub.resetPassword).not.toHaveBeenCalled();
    });

    it('does nothing when cancelled, and gives the focus back to the row\'s menu', async () => {
      const context = await loaded();
      await pick(context, 'bob', 'Réinitialiser le mot de passe');

      button(dialog(context.el), 'Annuler')!.click();
      await context.settle();

      expect(context.el.querySelector('gbt-confirm-danger-modal')).toBeNull();
      expect(usersStub.resetPassword).not.toHaveBeenCalled();
      expect(document.activeElement).toBe(menuTrigger(rowOf(context.el, 'bob')));
    });

    it('resets, shows the dialog busy, then says the link went out', async () => {
      const context = await confirmed();
      expect(usersStub.resetPassword).toHaveBeenCalledWith('u2');
      expect(context.el.querySelector('gbt-confirm-danger-modal button[aria-busy="true"]')).not.toBeNull();

      passwordResetRequests[0].next({ emailSent: true });
      passwordResetRequests[0].complete();
      await context.settle();

      expect(context.el.querySelector('gbt-confirm-danger-modal')).toBeNull();
      expect(toastStub.show).toHaveBeenCalledWith('Mot de passe réinitialisé. Un lien pour en choisir un nouveau a été envoyé à bob@example.com.');
      expect(context.el.querySelector('fg-link-mail-failed')).toBeNull();
      expect(document.activeElement).toBe(menuTrigger(rowOf(context.el, 'bob')));
      expect(authStub.logout).not.toHaveBeenCalled();
    });

    it('shows the reason and the reset link when the mail could not be sent, focused, until dismissed', async () => {
      const context = await confirmed();

      passwordResetRequests[0].next({ emailSent: false, emailError: 'the user has no deliverable e-mail address', resetUrl: RESET_URL });
      await context.settle();

      expect(context.el.querySelector('gbt-confirm-danger-modal')).toBeNull();
      const panel = context.el.querySelector('.admin-users__mail-failed')!;
      expect(text(panel)).toContain("Le mail n'a pas pu être envoyé");
      expect(text(panel)).toContain('the user has no deliverable e-mail address');
      expect(text(panel.querySelector('code'))).toBe(RESET_URL);
      expect(text(panel)).toContain('Transmettez ce lien à bob : il est valable 1 heure.');
      expect(panel.querySelector('button[aria-label="Copier le lien de réinitialisation"]')).not.toBeNull();
      expect(toastStub.show).not.toHaveBeenCalled();
      expect(document.activeElement).toBe(panel.querySelector('.link-mail-failed'));

      panel.querySelector<HTMLButtonElement>('button[aria-label="Fermer"]')!.click();
      await context.settle();
      expect(context.el.querySelector('.admin-users__mail-failed')).toBeNull();
    });

    it('replaces an earlier undelivered link with the new one', async () => {
      const context = await loaded();
      await pick(context, 'dave', "Renvoyer l'invitation");
      resendRequests[0].next({ user: PENDING, emailSent: false, activationUrl: 'http://localhost:4200/activate#token=old' });
      await context.settle();

      await pick(context, 'bob', 'Réinitialiser le mot de passe');
      button(dialog(context.el), 'Réinitialiser')!.click();
      await context.settle();
      passwordResetRequests[0].next({ emailSent: false, resetUrl: RESET_URL });
      await context.settle();

      expect(context.el.querySelectorAll('.admin-users__mail-failed')).toHaveLength(1);
      expect(text(context.el.querySelector('.admin-users__mail-failed code'))).toBe(RESET_URL);
    });

    it('says the account is still invited, and reloads, when the server refuses it as such (a stale list)', async () => {
      const context = await confirmed();

      passwordResetRequests[0].error(failure(400, 'the user has not activated their account yet; resend the invitation instead'));
      await context.settle();

      expect(context.el.querySelector('gbt-confirm-danger-modal')).toBeNull();
      expect(toastStub.show).toHaveBeenCalledWith("Ce compte n'est pas encore activé : renvoyez-lui plutôt l'invitation.", 'error');
      expect(usersStub.list).toHaveBeenCalledTimes(2);
    });

    it('points to the account settings when the server refuses the administrator\'s own account', async () => {
      const context = await confirmed();

      passwordResetRequests[0].error(failure(400, 'use your account settings to change your own password'));
      await context.settle();

      expect(toastStub.show).toHaveBeenCalledWith('Changez votre propre mot de passe depuis les paramètres de votre compte.', 'error');
      expect(usersStub.list).toHaveBeenCalledTimes(1);
    });

    it('says the account is gone, and reloads, on a 404', async () => {
      const context = await confirmed();

      passwordResetRequests[0].error(failure(404, 'user'));
      await context.settle();

      expect(toastStub.show).toHaveBeenCalledWith("Ce compte n'existe plus.", 'error');
      expect(usersStub.list).toHaveBeenCalledTimes(2);
    });

    it('says so, and gives the focus back to the row\'s menu, when the reset fails', async () => {
      const context = await confirmed();

      passwordResetRequests[0].error(failure(500));
      await context.settle();

      expect(context.el.querySelector('gbt-confirm-danger-modal')).toBeNull();
      expect(toastStub.show).toHaveBeenCalledWith("Le mot de passe n'a pas pu être réinitialisé. Réessayez plus tard.", 'error');
      expect(document.activeElement).toBe(menuTrigger(rowOf(context.el, 'bob')));
    });
  });

  describe('the administrator flag', () => {
    const OTHER_ADMIN = user({ id: 'u6', username: 'frank', isAdmin: true, createdAt: minutesAgo(60 * 24 * 60) });
    const dialog = (el: HTMLElement) => el.querySelector('gbt-confirm-danger-modal');
    const message = (el: HTMLElement) => text(dialog(el)!.querySelector('.gbt-confirm-danger-modal__message'))!;

    describe('granting it', () => {
      it('is immediate: no dialog, the row busy meanwhile', async () => {
        const context = await loaded();

        await pick(context, 'carol', 'Nommer super-administrateur');

        expect(dialog(context.el)).toBeNull();
        expect(usersStub.setAdmin).toHaveBeenCalledExactlyOnceWith('u3', true);
        expect(rowOf(context.el, 'carol').getAttribute('aria-busy')).toBe('true');
        expect(rowOf(context.el, 'carol').querySelector('gbt-spinner')?.textContent).toContain('Enregistrement en cours');
      });

      it('marks the row as an administrator, says so, and turns the item into the removal', async () => {
        const context = await loaded();
        await pick(context, 'carol', 'Nommer super-administrateur');

        setAdminRequests[0].next();
        setAdminRequests[0].complete();
        await context.settle();

        expect(badges(rowOf(context.el, 'carol'))).toContain('Super-administrateur');
        expect(toastStub.show).toHaveBeenCalledWith('carol est maintenant super-administrateur.');
        expect(document.activeElement).toBe(menuTrigger(rowOf(context.el, 'carol')));
        menuTrigger(rowOf(context.el, 'carol'))!.click();
        await context.settle();
        expect(menuItems(rowOf(context.el, 'carol'))).toContain("Retirer les droits de super-administrateur");
        expect(usersStub.list).toHaveBeenCalledTimes(1);
      });

      it('keeps the row as it was, and says so, when it fails', async () => {
        const context = await loaded();
        await pick(context, 'carol', 'Nommer super-administrateur');

        setAdminRequests[0].error(failure(500));
        await context.settle();

        expect(badges(rowOf(context.el, 'carol'))).not.toContain('Super-administrateur');
        expect(rowOf(context.el, 'carol').getAttribute('aria-busy')).toBeNull();
        expect(toastStub.show).toHaveBeenCalledWith("Les droits de super-administrateur n'ont pas pu être accordés. Réessayez plus tard.", 'error');
      });
    });

    describe('removing it', () => {
      async function confirmed(username: string) {
        const context = await loaded([...USERS, OTHER_ADMIN]);
        await pick(context, username, "Retirer les droits de super-administrateur");
        button(dialog(context.el)!, 'Retirer les droits')!.click();
        await context.settle();
        return context;
      }

      it('asks first, saying what the administrator will lose', async () => {
        const context = await loaded([...USERS, OTHER_ADMIN]);

        await pick(context, 'frank', "Retirer les droits de super-administrateur");

        expect(text(dialog(context.el)!.querySelector('h2'))).toBe("Retirer les droits de super-administrateur");
        expect(message(context.el)).toBe("frank ne pourra plus gérer les utilisateurs, les réglages de l'instance ni les runners. Vous pourrez lui rendre ces droits plus tard.");
        expect(usersStub.setAdmin).not.toHaveBeenCalled();
      });

      it('does nothing when cancelled', async () => {
        const context = await loaded([...USERS, OTHER_ADMIN]);
        await pick(context, 'frank', "Retirer les droits de super-administrateur");

        button(dialog(context.el)!, 'Annuler')!.click();
        await context.settle();

        expect(dialog(context.el)).toBeNull();
        expect(usersStub.setAdmin).not.toHaveBeenCalled();
        expect(document.activeElement).toBe(menuTrigger(rowOf(context.el, 'frank')));
      });

      it('removes it, shows the dialog busy, then updates the row and says so', async () => {
        const context = await confirmed('frank');
        expect(usersStub.setAdmin).toHaveBeenCalledExactlyOnceWith('u6', false);
        expect(context.el.querySelector('gbt-confirm-danger-modal button[aria-busy="true"]')).not.toBeNull();

        setAdminRequests[0].next();
        setAdminRequests[0].complete();
        await context.settle();

        expect(dialog(context.el)).toBeNull();
        expect(badges(rowOf(context.el, 'frank'))).not.toContain('Super-administrateur');
        expect(toastStub.show).toHaveBeenCalledWith("frank n'est plus super-administrateur.");
        expect(meStub.load).not.toHaveBeenCalled();
        expect(context.navigate).not.toHaveBeenCalled();
      });

      it('says exactly why when the server refuses to remove the last active administrator (409)', async () => {
        const context = await confirmed('frank');

        setAdminRequests[0].error(failure(409, 'cannot remove the last administrator'));
        await context.settle();

        expect(dialog(context.el)).toBeNull();
        expect(toastStub.show).toHaveBeenCalledExactlyOnceWith("Impossible de retirer les droits de super-administrateur : l'instance n'aurait plus aucun super-administrateur actif.", 'error');
        expect(badges(rowOf(context.el, 'frank'))).toContain('Super-administrateur');
      });

      it('says the account is gone, and reloads, on a 404', async () => {
        const context = await confirmed('frank');

        setAdminRequests[0].error(failure(404, 'user'));
        await context.settle();

        expect(toastStub.show).toHaveBeenCalledWith("Ce compte n'existe plus.", 'error');
        expect(usersStub.list).toHaveBeenCalledTimes(2);
      });

      it('says so when it fails otherwise', async () => {
        const context = await confirmed('frank');

        setAdminRequests[0].error(failure(500));
        await context.settle();

        expect(toastStub.show).toHaveBeenCalledWith("Les droits de super-administrateur n'ont pas pu être retirés. Réessayez plus tard.", 'error');
        expect(badges(rowOf(context.el, 'frank'))).toContain('Super-administrateur');
      });

      it('warns an administrator removing their own flag, then refreshes their profile and takes them out of the administration', async () => {
        const context = await loaded([...USERS, OTHER_ADMIN]);
        await pick(context, 'alice', "Retirer les droits de super-administrateur");

        expect(message(context.el)).toBe(
          "Vous allez vous retirer vous-même des super-administrateurs : vous perdrez aussitôt l'accès à l'administration (utilisateurs, réglages de l'instance, runners). Seul un autre super-administrateur pourra vous rendre ces droits.",
        );

        button(dialog(context.el)!, 'Retirer les droits')!.click();
        await context.settle();
        expect(usersStub.setAdmin).toHaveBeenCalledExactlyOnceWith('u1', false);
        setAdminRequests[0].next();
        setAdminRequests[0].complete();
        await context.settle();

        expect(meStub.isAdmin()).toBe(false);
        expect(meStub.load).toHaveBeenCalledTimes(1);
        expect(toastStub.show).toHaveBeenCalledWith("Vous n'êtes plus super-administrateur.");
        expect(context.navigate).toHaveBeenCalledWith('/home');
        expect(authStub.logout).not.toHaveBeenCalled();
      });

      it('keeps the administrator where they are when their own removal is refused (the last one)', async () => {
        const context = await confirmed('alice');

        setAdminRequests[0].error(failure(409, 'cannot remove the last administrator'));
        await context.settle();

        expect(toastStub.show).toHaveBeenCalledWith("Impossible de retirer les droits de super-administrateur : l'instance n'aurait plus aucun super-administrateur actif.", 'error');
        expect(meStub.load).not.toHaveBeenCalled();
        expect(context.navigate).not.toHaveBeenCalled();
        expect(badges(rowOf(context.el, 'alice'))).toContain('Super-administrateur');
      });
    });
  });

  describe('focus', () => {
    const kebabOf = (context: Awaited<ReturnType<typeof loaded>>, username: string) => menuTrigger(rowOf(context.el, username));

    it('returns to the row\'s menu when the reset is cancelled', async () => {
      const context = await loaded();
      await pick(context, 'bob', 'Réinitialiser la double authentification');

      button(context.el.querySelector('gbt-confirm-danger-modal')!, 'Annuler')!.click();
      await context.settle();

      expect(document.activeElement).toBe(kebabOf(context, 'bob'));
    });

    it('returns to the row\'s menu when the reset fails', async () => {
      const context = await loaded();
      await pick(context, 'bob', 'Réinitialiser la double authentification');
      button(context.el.querySelector('gbt-confirm-danger-modal')!, 'Réinitialiser')!.click();
      await context.settle();

      resetRequests[0].error(failure(500));
      await context.settle();

      expect(document.activeElement).toBe(kebabOf(context, 'bob'));
    });

    it('returns to the row\'s menu once the reset is done (the row keeps its other actions)', async () => {
      const context = await loaded();
      await pick(context, 'bob', 'Réinitialiser la double authentification');
      button(context.el.querySelector('gbt-confirm-danger-modal')!, 'Réinitialiser')!.click();
      await context.settle();

      resetRequests[0].next();
      resetRequests[0].complete();
      await context.settle();

      expect(document.activeElement).toBe(kebabOf(context, 'bob'));
    });

    it('returns to the row\'s menu after a resend, whichever way it ended', async () => {
      const context = await loaded();
      await pick(context, 'dave', "Renvoyer l'invitation");
      resendRequests[0].next({ user: { ...PENDING, invitationExpiresAt: inHours(24) }, emailSent: true });
      resendRequests[0].complete();
      await context.settle();
      expect(document.activeElement).toBe(kebabOf(context, 'dave'));

      await pick(context, 'erin', "Renvoyer l'invitation");
      resendRequests[1].error(failure(500));
      await context.settle();
      expect(document.activeElement).toBe(kebabOf(context, 'erin'));
    });

    it('falls back to the header button when the dialog was opened from the empty state and that button is gone', async () => {
      const context = await loaded([]);
      button(context.el.querySelector('gbt-empty-state')!, 'Inviter un utilisateur')!.click();
      await context.settle();
      context.component.refresh();
      listRequests[1].next([ADMIN]);
      await context.settle();
      (document.activeElement as HTMLElement | null)?.blur();

      button(context.el, 'Annuler')!.click();
      await context.settle();

      expect(document.activeElement).toBe(button(context.el.querySelector('gbt-page-header')!, 'Inviter un utilisateur'));
    });
  });

  describe('inviting', () => {
    it('opens the invitation dialog from the header, and drops it when it closes', async () => {
      const context = await loaded();
      expect(context.el.querySelector('fg-invite-user-modal')).toBeNull();

      button(context.el.querySelector('gbt-page-header')!, 'Inviter un utilisateur')!.click();
      await context.settle();
      expect(context.el.querySelector('fg-invite-user-modal [role="dialog"]')).not.toBeNull();

      button(context.el, 'Annuler')!.click();
      await context.settle();
      expect(context.el.querySelector('fg-invite-user-modal')).toBeNull();
    });

    it('rebuilds the dialog empty on each opening', async () => {
      const context = await loaded();
      const open = async () => {
        button(context.el.querySelector('gbt-page-header')!, 'Inviter un utilisateur')!.click();
        await context.settle();
      };
      await open();
      const modal = context.fixture.debugElement.query((d) => d.nativeElement.tagName === 'FG-INVITE-USER-MODAL').componentInstance as { username: { set(v: string): void } };
      modal.username.set('draft');
      button(context.el, 'Annuler')!.click();
      await context.settle();

      await open();
      expect(context.el.querySelector<HTMLInputElement>('fg-invite-user-modal input')!.value).toBe('');
    });

    it('reloads the list once an invitation is created, and leaves the dialog open on its result', async () => {
      const context = await loaded();
      button(context.el.querySelector('gbt-page-header')!, 'Inviter un utilisateur')!.click();
      await context.settle();
      const modal = context.fixture.debugElement.query((d) => d.nativeElement.tagName === 'FG-INVITE-USER-MODAL').componentInstance as { invited: { emit(r: InviteResult): void } };

      const created = user({ id: 'u6', username: 'frank', state: 'invited', mfaEnabled: false, invitationExpiresAt: inHours(24), createdAt: minutesAgo(0) });
      modal.invited.emit({ user: created, emailSent: true });
      await context.settle();

      expect(usersStub.list).toHaveBeenCalledTimes(2);
      listRequests[1].next([...USERS, created]);
      await context.settle();
      expect(names(context.el)[0]).toBe('frank');
      expect(context.el.querySelector('fg-invite-user-modal')).not.toBeNull();
    });
  });

  describe('when nothing shows', () => {
    it('says so and offers the invitation when there is no user at all', async () => {
      const { el } = await loaded([]);

      expect(el.querySelector('gbt-list-card .gbt-list-card')!.getAttribute('data-state')).toBe('empty');
      expect(el.querySelector('gbt-empty-state')).not.toBeNull();
      expect(text(el.querySelector('gbt-empty-state'))).toContain('Aucun utilisateur');
      expect(rows(el)).toHaveLength(0);
      expect(el.querySelector('gbt-list-toolbar')).toBeNull();
    });

    it('says the list could not be loaded (not "no user") in one alert, without a toast on top, and reloads on "Réessayer"', async () => {
      const { fixture, el, settle } = create();
      fixture.detectChanges();

      listRequests[0].error(failure(500));
      await settle();

      const card = el.querySelector('gbt-list-card')!;
      expect(card.querySelector('.gbt-list-card')!.getAttribute('data-state')).toBe('failed');
      const alerts = el.querySelectorAll('[role="alert"]');
      expect(alerts).toHaveLength(1);
      expect(card.contains(alerts[0])).toBe(true);
      expect(text(alerts[0])).toContain('Impossible de charger les utilisateurs');
      expect(alerts[0].querySelector('.gbt-empty-state')!.getAttribute('data-tone')).toBe('error');
      expect(card.querySelector('[aria-live]')).toBeNull();
      expect(card.querySelector('.gbt-empty-state__illustration')).toBeNull();
      expect(toastStub.show).not.toHaveBeenCalled();

      button(card, 'Réessayer')!.click();
      await settle();
      expect(usersStub.list).toHaveBeenCalledTimes(2);
      expect(card.querySelector('.gbt-list-card')!.getAttribute('data-state')).toBe('loading');
      expect(el.querySelector('[aria-busy="true"]')).not.toBeNull();
      expect(text(card.querySelector('[role="status"]'))).toBe('Chargement des utilisateurs…');

      listRequests[1].next(USERS);
      await settle();
      expect(el.querySelector('[role="alert"]')).toBeNull();
      expect(rows(el)).toHaveLength(5);
    });

    it('keeps the list it has when a reload fails', async () => {
      const context = await loaded();
      context.component.refresh();
      listRequests[1].error(failure(500));
      await context.settle();

      expect(rows(context.el)).toHaveLength(5);
      expect(context.el.querySelector('[role="alert"]')).toBeNull();
      expect(toastStub.show).toHaveBeenCalledWith('Impossible de charger les utilisateurs. Réessayez plus tard.', 'error');
    });
  });
});
