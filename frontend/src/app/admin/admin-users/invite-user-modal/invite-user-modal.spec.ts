import { HttpErrorResponse } from '@angular/common/http';
import { TestBed } from '@angular/core/testing';
import { Subject } from 'rxjs';
import { AdminUser, AdminUsersService, InviteResult } from '../../admin-users.service';
import { InviteUserModal } from './invite-user-modal';

const USER: AdminUser = {
  id: 'u2',
  username: 'bob@example.com',
  named: false,
  email: 'bob@example.com',
  isAdmin: false,
  createdAt: '2026-09-25T10:00:00Z',
  state: 'invited',
  invitationExpiresAt: '2026-09-26T10:00:00Z',
  mfaEnabled: false,
};
const SENT: InviteResult = { user: USER, emailSent: true };
const NOT_SENT: InviteResult = { user: USER, emailSent: false, emailError: 'connection refused', activationUrl: 'http://localhost:4200/activate#token=abc123' };

const text = (el: Element | null | undefined) => (el?.textContent ?? '').replace(/\s+/g, ' ').trim();
const dialog = () => document.querySelector<HTMLElement>('[role="dialog"]')!;
const failure = (status: number, error?: string) => new HttpErrorResponse({ status, error: error === undefined ? null : { error } });

describe('InviteUserModal', () => {
  let request: Subject<InviteResult>;
  let invite: ReturnType<typeof vi.fn>;

  function setup() {
    request = new Subject<InviteResult>();
    invite = vi.fn(() => request);
    TestBed.configureTestingModule({ providers: [{ provide: AdminUsersService, useValue: { invite } }] });
    const fixture = TestBed.createComponent(InviteUserModal);
    const component = fixture.componentInstance;
    const invited = vi.fn();
    const closed = vi.fn();
    component.invited.subscribe(invited);
    component.close.subscribe(closed);
    fixture.detectChanges();
    const fill = (email: string) => component.email.set(email);
    const button = (label: string) => Array.from(dialog().querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === label);
    const flush = async () => {
      await fixture.whenStable();
      fixture.detectChanges();
    };
    return { fixture, component, invited, closed, fill, button, flush };
  }

  afterEach(() => {
    document.querySelectorAll('[role="dialog"]').forEach((el) => el.remove());
  });

  describe('the form', () => {
    it('is a dialog "Inviter un utilisateur" with the e-mail and the administrator switch: the invitee chooses the name', () => {
      setup();

      expect(dialog().getAttribute('aria-label')).toBe('Inviter un utilisateur');
      const labels = Array.from(dialog().querySelectorAll('label')).map((l) => text(l));
      expect(labels).toEqual(['Adresse e-mail', 'Super-administrateur']);
      expect(dialog().querySelector('input[role="switch"]')).not.toBeNull();
      expect(text(dialog())).toContain("La personne choisit son nom d'utilisateur et son mot de passe");
    });

    it('describes the switch by what an administrator can do (aria-describedby)', () => {
      setup();
      const described = (control: Element | null) =>
        (control?.getAttribute('aria-describedby') ?? '')
          .split(' ')
          .filter(Boolean)
          .map((id) => text(dialog().querySelector(`#${id}`)));

      expect(described(dialog().querySelector('input[role="switch"]'))).toEqual(["Un super-administrateur gère les utilisateurs, les réglages de l'instance et les runners."]);
      expect(described(dialog().querySelector('gbt-input[name="email"] input'))).toEqual([]);
    });

    it('has one primary action, "Envoyer l\'invitation", and a secondary "Annuler"', () => {
      const { button } = setup();

      expect(button("Envoyer l'invitation")).toBeDefined();
      expect(button('Annuler')).toBeDefined();
      expect(dialog().querySelectorAll('.gbt-button--primary')).toHaveLength(1);
    });

    it('closes with "Annuler" and with the close button', () => {
      const { button, closed } = setup();

      button('Annuler')!.click();
      dialog().querySelector<HTMLButtonElement>('.gbt-modal__close')!.click();
      expect(closed).toHaveBeenCalledTimes(2);
    });
  });

  describe('checks before the round trip', () => {
    it('asks for an e-mail, and sends nothing', () => {
      const { component } = setup();

      component.submit();

      expect(component.emailError()).toBe("Saisissez l'adresse e-mail");
      expect(invite).not.toHaveBeenCalled();
    });

    it('refuses an invalid e-mail address', () => {
      const { component, fill } = setup();

      for (const email of ['bob', 'bob@', '@example.com', 'bob@example', 'bo b@example.com']) {
        fill(email);
        component.submit();
        expect(component.emailError(), email).toBe('Saisissez une adresse e-mail valide, par exemple nom@exemple.fr');
      }
      expect(invite).not.toHaveBeenCalled();
    });

    it('clears the message as soon as the address is edited', () => {
      const { component } = setup();
      component.submit();

      expect(component.emailError()).not.toBeNull();
      component.onEmailChange('bob@example.com');
      expect(component.emailError()).toBeNull();
    });
  });

  describe('sending', () => {
    it('invites the trimmed e-mail, as a plain user by default', () => {
      const { component, fill } = setup();
      fill(' bob@example.com ');

      component.submit();

      expect(invite).toHaveBeenCalledWith('bob@example.com', false);
    });

    it('invites an administrator when the switch is on', () => {
      const { component, fill } = setup();
      fill('bob@example.com');
      component.isAdmin.set(true);

      component.submit();

      expect(invite).toHaveBeenCalledWith('bob@example.com', true);
    });

    it('shows the button busy while the request runs, and ignores a second submit', () => {
      const { fixture, component, fill } = setup();
      fill('bob@example.com');

      component.submit();
      component.submit();
      fixture.detectChanges();

      expect(invite).toHaveBeenCalledTimes(1);
      const submit = dialog().querySelector<HTMLButtonElement>('button[type="submit"]')!;
      expect(submit.getAttribute('aria-busy')).toBe('true');
      expect(submit.disabled).toBe(true);
    });

    it('ignores Escape, the backdrop, the close button and "Annuler" while the request runs: the outcome is never dropped', async () => {
      const { fixture, component, fill, closed, invited, flush } = setup();
      fill('bob@example.com');
      component.submit();
      fixture.detectChanges();

      dialog().dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
      document.querySelector<HTMLElement>('.gbt-modal__backdrop')!.click();
      dialog().querySelector<HTMLButtonElement>('.gbt-modal__close')!.click();
      const cancel = Array.from(dialog().querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === 'Annuler')!;
      expect(cancel.disabled).toBe(true);
      cancel.click();
      expect(closed).not.toHaveBeenCalled();

      request.next(NOT_SENT);
      request.complete();
      await flush();
      expect(invited).toHaveBeenCalledWith(NOT_SENT);
      expect(text(dialog().querySelector('fg-link-mail-failed code'))).toBe('http://localhost:4200/activate#token=abc123');

      dialog().dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
      expect(closed).toHaveBeenCalledTimes(1);
    });

    it('drops the send icon while sending, so the buttons stay on one line', () => {
      const { fixture, component, fill } = setup();
      const submit = () => dialog().querySelector<HTMLButtonElement>('button[type="submit"]')!;
      expect(submit().querySelector('gbt-icon')).not.toBeNull();
      fill('bob@example.com');

      component.submit();
      fixture.detectChanges();

      expect(submit().querySelector('gbt-icon')).toBeNull();
    });

    it('emits the result and switches to "Invitation envoyée à …" when the mail went out', async () => {
      const { component, fill, invited, flush } = setup();
      fill('bob@example.com');

      component.submit();
      request.next(SENT);
      request.complete();
      await flush();

      expect(invited).toHaveBeenCalledWith(SENT);
      expect(dialog().getAttribute('aria-label')).toBe('Inviter un utilisateur');
      expect(text(dialog())).toContain('Invitation envoyée à bob@example.com');
      expect(dialog().querySelector('gbt-alert .gbt-alert')?.getAttribute('data-variant')).toBe('success');
      expect(dialog().querySelector('form')).toBeNull();
      expect(dialog().querySelector('fg-link-mail-failed')).toBeNull();
    });

    it('shows the mail failure, its reason and the activation link when the mail could not be sent', async () => {
      const { component, fill, invited, flush } = setup();
      fill('bob@example.com');

      component.submit();
      request.next(NOT_SENT);
      request.complete();
      await flush();

      expect(invited).toHaveBeenCalledWith(NOT_SENT);
      const panel = dialog().querySelector('fg-link-mail-failed')!;
      expect(text(panel)).toContain("Le mail n'a pas pu être envoyé");
      expect(text(panel)).toContain('connection refused');
      expect(text(panel.querySelector('code'))).toBe('http://localhost:4200/activate#token=abc123');
      expect(text(dialog())).not.toContain('Invitation envoyée à');
    });

    it('closes from the result view with "Terminé"', async () => {
      const { component, fill, closed, button, flush } = setup();
      fill('bob@example.com');
      component.submit();
      request.next(SENT);
      await flush();

      button('Terminé')!.click();

      expect(closed).toHaveBeenCalledTimes(1);
    });

    it('moves the focus onto the result, since the form it was in is gone', async () => {
      const { component, fill, flush } = setup();
      fill('bob@example.com');
      dialog().querySelector<HTMLInputElement>('input')!.focus();

      component.submit();
      request.next(SENT);
      await flush();
      await flush();

      expect(dialog().contains(document.activeElement)).toBe(true);
      expect(document.activeElement?.tagName).not.toBe('BODY');
    });
  });

  describe('refusals keep the draft', () => {
    async function refused(status: number, error?: string) {
      const context = setup();
      context.fill('bob@example.com');
      context.component.isAdmin.set(true);
      context.component.submit();
      request.error(failure(status, error));
      await context.flush();
      return context;
    }

    it('409: the address is already used, under the e-mail field; the draft stays and can be sent again', async () => {
      const { component, invited } = await refused(409, 'email already in use');

      expect(component.emailError()).toBe('Cette adresse e-mail est déjà utilisée par un compte');
      expect(dialog().querySelector('gbt-alert')).toBeNull();
      expect(component.email()).toBe('bob@example.com');
      expect(component.isAdmin()).toBe(true);
      expect(invited).not.toHaveBeenCalled();

      request = new Subject<InviteResult>();
      invite.mockReturnValue(request);
      component.submit();
      expect(invite).toHaveBeenCalledTimes(2);
    });

    it('400 on the e-mail: under the e-mail field', async () => {
      const { component } = await refused(400, 'email is not a valid address');
      expect(component.emailError()).toBe('Saisissez une adresse e-mail valide, par exemple nom@exemple.fr');
    });

    it('any other 400 asks to check the fields', async () => {
      await refused(400, 'something else');
      expect(text(dialog().querySelector('gbt-alert'))).toBe('Vérifiez les champs');
    });

    it('a server error says the invitation was not sent, and to try again', async () => {
      const { component } = await refused(500);

      expect(text(dialog().querySelector('gbt-alert'))).toBe("L'invitation n'a pas pu être envoyée. Réessayez plus tard.");
      expect(component.email()).toBe('bob@example.com');
    });

    it('drops the previous refusal when the form is submitted again', async () => {
      const { fixture, component } = await refused(400, 'something else');
      request = new Subject<InviteResult>();
      invite.mockReturnValue(request);

      component.submit();
      fixture.detectChanges();

      expect(dialog().querySelector('gbt-alert')).toBeNull();
    });
  });
});
