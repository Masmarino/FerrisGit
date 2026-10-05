import { HttpErrorResponse } from '@angular/common/http';
import { LOCALE_ID, signal, WritableSignal } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { ActivatedRoute, convertToParamMap, provideRouter, Router } from '@angular/router';
import { BehaviorSubject, Subject } from 'rxjs';
import { formatBytes, formatDateTime } from '@masmarino/gabarit/format';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { AdminUser, AdminUserRepository, AdminUsersService, InviteResult, PasswordResetResult } from '../../admin-users.service';
import { MeService } from '../../../shell/me.service';
import { PageTitleService } from '../../../shell/page-title.service';
import { AdminUserDetail, deletionMessage, deletionRefusal, totalSize } from './admin-user-detail';

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();
const minutesAgo = (minutes: number) => new Date(Date.now() - minutes * 60_000).toISOString();
const inHours = (hours: number) => new Date(Date.now() + hours * 3_600_000).toISOString();
const size = (bytes: number) => formatBytes(bytes, 'fr', { binaryUnits: 'legacy' });
const shownSize = (bytes: number) => size(bytes).replace(/\s+/g, ' ');

const user = (overrides: Partial<AdminUser> & Pick<AdminUser, 'id' | 'username'>): AdminUser => ({
  named: true,
  email: `${overrides.username}@example.com`,
  isAdmin: false,
  createdAt: minutesAgo(60 * 24 * 5),
  state: 'active',
  invitationExpiresAt: null,
  mfaEnabled: true,
  ...overrides,
});

const ME = user({ id: 'u1', username: 'alice', isAdmin: true });
const BOB = user({ id: 'u2', username: 'bob', createdAt: minutesAgo(60 * 24 * 20) });
const CAROL = user({ id: 'u3', username: 'carol', isAdmin: true, mfaEnabled: false });
const DAVE = user({ id: 'u4', username: 'dave', state: 'invited', mfaEnabled: false, invitationExpiresAt: inHours(20) });
const USERS = [ME, BOB, CAROL, DAVE];

const repository = (overrides: Partial<AdminUserRepository> & Pick<AdminUserRepository, 'id' | 'name'>): AdminUserRepository => ({
  description: '',
  visibility: 'private',
  createdAt: minutesAgo(60 * 24 * 3),
  sizeBytes: 1024,
  ...overrides,
});

const REPOSITORIES = [
  repository({ id: 'r1', name: 'widget', description: 'Le widget', visibility: 'public', sizeBytes: 3 * 1024 * 1024 }),
  repository({ id: 'r2', name: 'notes', sizeBytes: 512 * 1024 }),
];

const failure = (status: number, error?: string) => new HttpErrorResponse({ status, error: error === undefined ? null : { error } });

describe('AdminUserDetail', () => {
  let listRequests: Subject<AdminUser[]>[];
  let repositoryRequests: Subject<AdminUserRepository[]>[];
  let resendRequests: Subject<InviteResult>[];
  let resetRequests: Subject<void>[];
  let passwordResetRequests: Subject<PasswordResetResult>[];
  let setAdminRequests: Subject<void>[];
  let deleteRequests: Subject<void>[];
  let usersStub: Record<'list' | 'repositories' | 'resend' | 'resetMfa' | 'resetPassword' | 'setAdmin' | 'deleteUser', ReturnType<typeof vi.fn>>;
  let meStub: { id: WritableSignal<string> };
  let toastStub: { show: ReturnType<typeof vi.fn> };
  let pageTitleStub: { set: ReturnType<typeof vi.fn> };
  let params: BehaviorSubject<ReturnType<typeof convertToParamMap>>;

  const request = <T>(bucket: Subject<T>[]) =>
    vi.fn(() => {
      const subject = new Subject<T>();
      bucket.push(subject);
      return subject;
    });

  function create(id = 'u2', meId = 'u1') {
    listRequests = [];
    repositoryRequests = [];
    resendRequests = [];
    resetRequests = [];
    passwordResetRequests = [];
    setAdminRequests = [];
    deleteRequests = [];
    usersStub = {
      list: request(listRequests),
      repositories: request(repositoryRequests),
      resend: request(resendRequests),
      resetMfa: request(resetRequests),
      resetPassword: request(passwordResetRequests),
      setAdmin: request(setAdminRequests),
      deleteUser: request(deleteRequests),
    };
    meStub = { id: signal(meId) };
    toastStub = { show: vi.fn() };
    pageTitleStub = { set: vi.fn() };
    params = new BehaviorSubject(convertToParamMap({ id }));
    TestBed.configureTestingModule({
      providers: [
        provideRouter([]),
        { provide: LOCALE_ID, useValue: 'fr' },
        { provide: ActivatedRoute, useValue: { paramMap: params } },
        { provide: AdminUsersService, useValue: usersStub },
        { provide: GbtToastService, useValue: toastStub },
        { provide: PageTitleService, useValue: pageTitleStub },
        { provide: MeService, useValue: meStub },
      ],
    });
    const fixture = TestBed.createComponent(AdminUserDetail);
    const el = fixture.nativeElement as HTMLElement;
    const navigate = vi.spyOn(TestBed.inject(Router), 'navigateByUrl').mockResolvedValue(true);
    const settle = async () => {
      await fixture.whenStable();
      fixture.detectChanges();
    };
    return { fixture, component: fixture.componentInstance, el, settle, navigate };
  }

  async function loaded(options: { id?: string; meId?: string; users?: AdminUser[]; repositories?: AdminUserRepository[] } = {}) {
    const context = create(options.id, options.meId);
    context.fixture.detectChanges();
    listRequests[0].next(options.users ?? USERS);
    repositoryRequests[0].next(options.repositories ?? REPOSITORIES);
    await context.settle();
    return context;
  }

  const badges = (el: HTMLElement) => Array.from(el.querySelectorAll('gbt-page-header gbt-badge')).map((badge) => text(badge));
  const button = (root: ParentNode, label: string) => Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === label);
  const menuTrigger = (el: HTMLElement) => el.querySelector<HTMLButtonElement>('.admin-user-detail__menu button[aria-haspopup="menu"]');
  const menuItems = (el: HTMLElement) => Array.from(el.querySelectorAll<HTMLElement>('[role="menuitem"]')).map((item) => text(item));
  const deleteButton = (el: HTMLElement) => el.querySelector<HTMLButtonElement>('.admin-user-detail__delete button')!;
  const dialog = (el: HTMLElement) => el.querySelector('gbt-confirm-danger-modal');
  const message = (el: HTMLElement) => dialog(el)!.querySelector('.gbt-confirm-danger-modal__message')!.textContent!;
  const repositoryRows = (el: HTMLElement) => Array.from(el.querySelectorAll<HTMLElement>('.admin-user-detail__repositories > li > gbt-list-row'));

  async function pick(context: Awaited<ReturnType<typeof loaded>>, label: string) {
    menuTrigger(context.el)!.click();
    await context.settle();
    Array.from(context.el.querySelectorAll<HTMLElement>('[role="menuitem"]'))
      .find((item) => text(item) === label)!
      .click();
    await context.settle();
  }

  async function typeConfirmation(context: Awaited<ReturnType<typeof loaded>>, value: string) {
    const input = dialog(context.el)!.querySelector<HTMLInputElement>('input')!;
    input.value = value;
    input.dispatchEvent(new Event('input'));
    await context.settle();
  }

  async function confirmedDeletion(options: Parameters<typeof loaded>[0] = {}) {
    const context = await loaded(options);
    deleteButton(context.el).click();
    await context.settle();
    await typeConfirmation(context, 'bob');
    button(dialog(context.el)!, "Supprimer l'utilisateur")!.click();
    await context.settle();
    return context;
  }

  describe('loading the user', () => {
    it('reads the list (there is no single-user endpoint) and the repositories of the id in the route', () => {
      const { fixture } = create('u2');
      fixture.detectChanges();

      expect(usersStub.list).toHaveBeenCalledTimes(1);
      expect(usersStub.repositories).toHaveBeenCalledExactlyOnceWith('u2');
      expect(fixture.nativeElement.querySelector('[aria-busy="true"]')).not.toBeNull();
    });

    it('titles the page with the username once found', async () => {
      await loaded();
      expect(pageTitleStub.set).toHaveBeenLastCalledWith('bob');
    });

    it('says the user was not found when the id is not in the list, with the way back', async () => {
      const { el } = await loaded({ id: 'nope' });

      expect(text(el.querySelector('gbt-empty-state'))).toContain('Utilisateur introuvable');
      expect(el.querySelector('gbt-page-header')).toBeNull();
      expect(el.querySelector('gbt-empty-state a')?.getAttribute('href')).toBe('/admin/users');
      expect(pageTitleStub.set).toHaveBeenLastCalledWith('Utilisateur introuvable');
    });

    it('says the list could not be loaded, and retries', async () => {
      const context = create();
      context.fixture.detectChanges();
      listRequests[0].error(failure(500));
      repositoryRequests[0].next(REPOSITORIES);
      await context.settle();

      expect(text(context.el.querySelector('[role="alert"]'))).toContain('Impossible de charger cet utilisateur.');
      button(context.el, 'Réessayer')!.click();
      await context.settle();
      expect(usersStub.list).toHaveBeenCalledTimes(2);
      listRequests[1].next(USERS);
      await context.settle();
      expect(text(context.el.querySelector('gbt-page-header h1'))).toBe('bob');
    });

    it("does not manage the signed-in administrator's own account: no action, no deletion, the way to their settings", async () => {
      const { el } = await loaded({ id: 'u1' });

      expect(el.querySelector('gbt-page-header')).toBeNull();
      expect(el.querySelector('.admin-user-detail__delete')).toBeNull();
      expect(text(el.querySelector('gbt-empty-state'))).toContain("C'est votre propre compte");
      const links = Array.from(el.querySelectorAll('gbt-empty-state a')).map((a) => a.getAttribute('href'));
      expect(links).toEqual(['/account', '/admin/users']);
    });

    it('offers no action until the signed-in administrator is known', async () => {
      const { el } = await loaded({ meId: '' });

      expect(el.querySelector('gbt-page-header')).not.toBeNull();
      expect(el.querySelector('.admin-user-detail__actions')).toBeNull();
    });
  });

  describe('the header', () => {
    it('names the user, with their address, creation date and badges', async () => {
      const { el } = await loaded();

      expect(text(el.querySelector('gbt-page-header h1'))).toBe('bob');
      expect(text(el.querySelector('.admin-user-detail__email'))).toBe('bob@example.com');
      const time = el.querySelector<HTMLTimeElement>('.admin-user-detail__meta time')!;
      expect(text(time)).toBe('il y a 20 j');
      expect(time.title).toBe(formatDateTime(BOB.createdAt, 'fr', { day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' }));
      expect(badges(el)).toEqual(['Actif', 'Double authentification active']);
    });

    it('marks a super-administrator, and an account without its second factor', async () => {
      const { el } = await loaded({ id: 'u3' });
      expect(badges(el)).toEqual(['Super-administrateur', 'Actif', 'Non configurée']);
    });

    it('shows a pending invitation and its expiry', async () => {
      const { el } = await loaded({ id: 'u4' });
      expect(badges(el)).toEqual(['Invitation en attente']);
      expect(text(el.querySelector('.admin-user-detail__expiry'))).toContain('expire le');
    });
  });

  describe('the actions', () => {
    it('offers the same actions as the list row: an active account with its second factor', async () => {
      const context = await loaded();
      menuTrigger(context.el)!.click();
      await context.settle();
      expect(menuItems(context.el)).toEqual(['Réinitialiser le mot de passe', 'Réinitialiser la double authentification', 'Nommer super-administrateur']);
    });

    it('offers the resend to an invited account, and the removal of the flag to a super-administrator', async () => {
      const invited = await loaded({ id: 'u4' });
      menuTrigger(invited.el)!.click();
      await invited.settle();
      expect(menuItems(invited.el)).toEqual(["Renvoyer l'invitation", 'Nommer super-administrateur']);
      TestBed.resetTestingModule();

      const admin = await loaded({ id: 'u3' });
      menuTrigger(admin.el)!.click();
      await admin.settle();
      expect(menuItems(admin.el)).toEqual(['Réinitialiser le mot de passe', 'Retirer les droits de super-administrateur']);
    });

    it('resends an invitation and says so', async () => {
      const context = await loaded({ id: 'u4' });
      await pick(context, "Renvoyer l'invitation");

      expect(usersStub.resend).toHaveBeenCalledExactlyOnceWith('u4');
      expect(context.el.querySelector('.admin-user-detail__busy gbt-spinner')).not.toBeNull();
      resendRequests[0].next({ user: { ...DAVE, invitationExpiresAt: inHours(24) }, emailSent: true });
      await context.settle();

      expect(toastStub.show).toHaveBeenCalledWith('Invitation renvoyée à dave@example.com.');
      expect(context.el.querySelector('.admin-user-detail__busy')).toBeNull();
    });

    it('shows the activation link when the invitation mail could not be sent', async () => {
      const context = await loaded({ id: 'u4' });
      await pick(context, "Renvoyer l'invitation");
      resendRequests[0].next({ user: DAVE, emailSent: false, emailError: 'connection refused', activationUrl: 'http://localhost/activate#token=abc' });
      await context.settle();

      expect(text(context.el.querySelector('fg-link-mail-failed code'))).toBe('http://localhost/activate#token=abc');
    });

    it('resets the password after confirming, and says the link went out', async () => {
      const context = await loaded();
      await pick(context, 'Réinitialiser le mot de passe');

      expect(message(context.el)).toContain('Le mot de passe actuel de bob cessera de fonctionner immédiatement');
      button(dialog(context.el)!, 'Réinitialiser')!.click();
      await context.settle();
      expect(usersStub.resetPassword).toHaveBeenCalledExactlyOnceWith('u2');
      passwordResetRequests[0].next({ emailSent: true });
      await context.settle();

      expect(dialog(context.el)).toBeNull();
      expect(toastStub.show).toHaveBeenCalledWith('Mot de passe réinitialisé. Un lien pour en choisir un nouveau a été envoyé à bob@example.com.');
    });

    it('resets the double authentication after confirming, and updates the badge', async () => {
      const context = await loaded();
      await pick(context, 'Réinitialiser la double authentification');

      expect(message(context.el)).toContain('bob sera déconnecté de tous ses appareils');
      button(dialog(context.el)!, 'Réinitialiser')!.click();
      await context.settle();
      resetRequests[0].next();
      await context.settle();

      expect(toastStub.show).toHaveBeenCalledWith('Double authentification réinitialisée.');
      expect(badges(context.el)).toContain('Non configurée');
    });

    it('grants the super-administrator flag at once', async () => {
      const context = await loaded();
      await pick(context, 'Nommer super-administrateur');

      expect(dialog(context.el)).toBeNull();
      expect(usersStub.setAdmin).toHaveBeenCalledExactlyOnceWith('u2', true);
      setAdminRequests[0].next();
      await context.settle();

      expect(badges(context.el)).toContain('Super-administrateur');
      expect(toastStub.show).toHaveBeenCalledWith('bob est maintenant super-administrateur.');
    });

    it('removes the flag after confirming, and says why when it is the last active super-administrator (409)', async () => {
      const context = await loaded({ id: 'u3' });
      await pick(context, 'Retirer les droits de super-administrateur');

      expect(text(dialog(context.el)!.querySelector('h2'))).toBe('Retirer les droits de super-administrateur');
      button(dialog(context.el)!, 'Retirer les droits')!.click();
      await context.settle();
      setAdminRequests[0].error(failure(409, 'cannot remove the last administrator'));
      await context.settle();

      expect(toastStub.show).toHaveBeenCalledWith("Impossible de retirer les droits de super-administrateur : l'instance n'aurait plus aucun super-administrateur actif.", 'error');
      expect(badges(context.el)).toContain('Super-administrateur');
    });

    it('goes back to the list when an action finds the account gone (404)', async () => {
      const context = await loaded();
      await pick(context, 'Nommer super-administrateur');
      setAdminRequests[0].error(failure(404, 'user'));
      await context.settle();

      expect(toastStub.show).toHaveBeenCalledWith("Ce compte n'existe plus.", 'error');
      expect(context.navigate).toHaveBeenCalledWith('/admin/users');
    });
  });

  describe('the repositories', () => {
    it('lists the personal repositories: name, visibility, description, creation date and size', async () => {
      const { el } = await loaded();
      const rows = repositoryRows(el);

      expect(rows.map((row) => text(row.querySelector('.admin-user-detail__repository-name')))).toEqual(['widget', 'notes']);
      expect(text(rows[0].querySelector('gbt-badge'))).toBe('Public');
      expect(text(rows[1].querySelector('gbt-badge'))).toBe('Privé');
      expect(text(rows[0].querySelector('.admin-user-detail__description'))).toBe('Le widget');
      expect(text(rows[0].querySelector('time'))).toBe('il y a 3 j');
      expect(text(rows[0].querySelector('.admin-user-detail__size'))).toBe(shownSize(3 * 1024 * 1024));
      expect(text(el.querySelector('.admin-user-detail__count'))).toBe('2 dépôts');
    });

    it('links to none of them: this page is no way into their content', async () => {
      const { el } = await loaded();
      expect(el.querySelector('.admin-user-detail__repositories a')).toBeNull();
      expect(el.querySelector('gbt-list-card a')).toBeNull();
    });

    it('says a size could not be computed, and the total is then a floor', async () => {
      const { el } = await loaded({ repositories: [...REPOSITORIES, repository({ id: 'r3', name: 'broken', sizeBytes: null })] });

      expect(text(repositoryRows(el)[2].querySelector('.admin-user-detail__size'))).toBe('taille inconnue');
      expect(text(el.querySelector('.admin-user-detail__facts'))).toContain(`au moins ${shownSize(3 * 1024 * 1024 + 512 * 1024)}`);
    });

    it('sums their sizes in the aside', async () => {
      const { el } = await loaded();
      expect(text(el.querySelector('.admin-user-detail__facts'))).toContain(shownSize(3 * 1024 * 1024 + 512 * 1024));
    });

    it('says the user owns none', async () => {
      const { el } = await loaded({ repositories: [] });

      expect(text(el.querySelector('gbt-list-card gbt-empty-state'))).toContain('Aucun dépôt personnel');
      expect(repositoryRows(el)).toHaveLength(0);
    });

    it('says they could not be loaded, retries, and keeps the deletion closed meanwhile', async () => {
      const context = create();
      context.fixture.detectChanges();
      listRequests[0].next(USERS);
      repositoryRequests[0].error(failure(500));
      await context.settle();

      expect(text(context.el.querySelector('gbt-list-card [role="alert"]'))).toContain('Impossible de charger les dépôts');
      expect(deleteButton(context.el).disabled).toBe(true);
      button(context.el.querySelector('gbt-list-card')!, 'Réessayer')!.click();
      await context.settle();
      repositoryRequests[1].next(REPOSITORIES);
      await context.settle();
      expect(repositoryRows(context.el)).toHaveLength(2);
      expect(deleteButton(context.el).disabled).toBe(false);
    });
  });

  describe('deleting the user', () => {
    it('says everything it does before anything is typed, the repositories counted and weighed', async () => {
      const context = await loaded();
      deleteButton(context.el).click();
      await context.settle();

      expect(text(dialog(context.el)!.querySelector('h2'))).toBe("Supprimer l'utilisateur");
      expect(message(context.el).split('\n')).toEqual([
        'Le compte de bob sera définitivement supprimé.',
        `Ses 2 dépôts personnels (${size(3 * 1024 * 1024 + 512 * 1024)} au total) seront définitivement supprimés, fichiers sur le disque compris.`,
        "Les dépôts qu'il a créés dans un groupe seront conservés et vous seront réattribués.",
        'Ses demandes de fusion, ses commentaires sur les demandes de fusion et ses releases seront conservés et affichés comme « Utilisateur supprimé ».',
        "Les tickets et les commentaires de tickets qu'il a rédigés, ses revues et les pipelines qu'il a déclenchés seront supprimés.",
        'Cette action est irréversible.',
      ]);
      expect(usersStub.deleteUser).not.toHaveBeenCalled();
    });

    it('asks for the username typed out before it can be confirmed', async () => {
      const context = await loaded();
      deleteButton(context.el).click();
      await context.settle();
      const confirm = () => button(dialog(context.el)!, "Supprimer l'utilisateur")!;

      expect(text(dialog(context.el)!.querySelector('label'))).toContain("Tapez le nom d'utilisateur pour confirmer");
      expect(confirm().disabled).toBe(true);
      await typeConfirmation(context, 'bo');
      expect(confirm().disabled).toBe(true);
      await typeConfirmation(context, 'bob');
      expect(confirm().disabled).toBe(false);
    });

    it('does nothing when cancelled', async () => {
      const context = await loaded();
      deleteButton(context.el).click();
      await context.settle();

      button(dialog(context.el)!, 'Annuler')!.click();
      await context.settle();

      expect(dialog(context.el)).toBeNull();
      expect(usersStub.deleteUser).not.toHaveBeenCalled();
    });

    it('deletes, then says so and goes back to the list', async () => {
      const context = await confirmedDeletion();
      expect(usersStub.deleteUser).toHaveBeenCalledExactlyOnceWith('u2');
      expect(dialog(context.el)!.querySelector('button[aria-busy="true"]')).not.toBeNull();

      deleteRequests[0].next();
      deleteRequests[0].complete();
      await context.settle();

      expect(toastStub.show).toHaveBeenCalledWith('Le compte de bob a été supprimé.');
      expect(context.navigate).toHaveBeenCalledWith('/admin/users');
    });

    it('says exactly why when the user is the last active super-administrator (409), on the page, until dismissed', async () => {
      const context = await confirmedDeletion();
      deleteRequests[0].error(failure(409, 'cannot remove the last administrator'));
      await context.settle();

      expect(dialog(context.el)).toBeNull();
      const alert = context.el.querySelector('.admin-user-detail__refusal-alert')!;
      expect(text(alert)).toContain(
        "bob est le dernier super-administrateur actif de l'instance : son compte ne peut pas être supprimé. Nommez d'abord un autre super-administrateur, puis réessayez.",
      );
      expect(text(alert)).not.toContain('mainteneur');
      expect(document.activeElement).toBe(alert.querySelector('.admin-user-detail__refusal'));
      expect(toastStub.show).not.toHaveBeenCalled();
      expect(context.navigate).not.toHaveBeenCalled();

      alert.querySelector<HTMLButtonElement>('button[aria-label="Fermer"]')!.click();
      await context.settle();
      expect(context.el.querySelector('.admin-user-detail__refusal-alert')).toBeNull();
    });

    it("names the group when the user is a group's last maintainer (409)", async () => {
      const context = await confirmedDeletion();
      deleteRequests[0].error(failure(409, 'the user is the last maintainer of the group acme/backend; promote another member first'));
      await context.settle();

      const alert = text(context.el.querySelector('.admin-user-detail__refusal-alert'));
      expect(alert).toContain(
        "bob est le dernier mainteneur du groupe acme/backend : sans lui, plus personne ne pourrait gérer ce groupe. Nommez d'abord un autre membre mainteneur de ce groupe, puis réessayez.",
      );
      expect(alert).not.toContain('super-administrateur');
      expect(toastStub.show).not.toHaveBeenCalled();
    });

    it('goes back to the list when the account is already gone (404)', async () => {
      const context = await confirmedDeletion();
      deleteRequests[0].error(failure(404, 'user'));
      await context.settle();

      expect(toastStub.show).toHaveBeenCalledWith("Ce compte n'existe plus.", 'error');
      expect(context.navigate).toHaveBeenCalledWith('/admin/users');
    });

    it('says so when it fails otherwise, and stays', async () => {
      const context = await confirmedDeletion();
      deleteRequests[0].error(failure(500));
      await context.settle();

      expect(toastStub.show).toHaveBeenCalledWith("Le compte n'a pas pu être supprimé. Réessayez plus tard.", 'error');
      expect(context.el.querySelector('.admin-user-detail__refusal-alert')).toBeNull();
      expect(context.navigate).not.toHaveBeenCalled();
    });
  });

  describe('the deletion copy', () => {
    it('speaks of one repository, of none, and of sizes it does not know', () => {
      expect(deletionMessage('bob', [])).toContain('bob ne possède aucun dépôt personnel.');
      expect(deletionMessage('bob', [REPOSITORIES[1]])).toContain(`Son dépôt personnel (${size(512 * 1024)}) sera définitivement supprimé, fichiers sur le disque compris.`);
      expect(deletionMessage('bob', [repository({ id: 'x', name: 'x', sizeBytes: null }), repository({ id: 'y', name: 'y', sizeBytes: null })])).toContain(
        'Ses 2 dépôts personnels (taille inconnue) seront définitivement supprimés',
      );
      expect(totalSize([])).toBeNull();
    });

    it('recognises the two refusals exactly, and nothing else', () => {
      expect(deletionRefusal('bob', 'cannot remove the last administrator')).toContain('dernier super-administrateur actif');
      expect(deletionRefusal('bob', 'cannot remove the last administrator!')).toBeNull();
      expect(deletionRefusal('bob', 'the user is the last maintainer of the group a/b/c; promote another member first')).toContain('du groupe a/b/c :');
      expect(deletionRefusal('bob', 'something else')).toBeNull();
      expect(deletionRefusal('bob', undefined)).toBeNull();
    });
  });
});
