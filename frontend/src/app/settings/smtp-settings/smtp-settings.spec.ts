import { ComponentFixture, TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { provideHttpClient } from '@angular/common/http';
import { HttpTestingController, provideHttpClientTesting, TestRequest } from '@angular/common/http/testing';
import { Icon, SegmentedControl, GbtToastService } from '@masmarino/gabarit';
import { isValidMailbox, SmtpSettings, validateSmtpForm } from './smtp-settings';
import { SmtpSettings as SmtpSettingsData } from '../settings.service';

const URL = '/api/admin/settings/smtp';
const TEST_URL = '/api/admin/settings/smtp/test';

const CONFIGURED: SmtpSettingsData = {
  configured: true,
  host: 'smtp.example.com',
  port: 465,
  security: 'tls',
  username: 'mailer',
  passwordSet: true,
  fromAddress: 'ferrisgit@example.com',
  fromName: 'Équipe FerrisGit',
};

const UNCONFIGURED: SmtpSettingsData = {
  configured: false,
  host: '',
  port: 587,
  security: 'starttls',
  username: '',
  passwordSet: false,
  fromAddress: '',
  fromName: 'FerrisGit',
};

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();

const describedBy = (el: HTMLElement, field: string) => {
  const input = el.querySelector(`[data-field="${field}"] input`);
  return (input?.getAttribute('aria-describedby') ?? '')
    .split(' ')
    .filter(Boolean)
    .map((id) => text(el.querySelector(`#${id}`)));
};

async function settle(fixture: ComponentFixture<unknown>) {
  fixture.detectChanges();
  await fixture.whenStable();
  fixture.detectChanges();
}

async function setup(initial: SmtpSettingsData | 'pending' | 'fail' = CONFIGURED) {
  const toast = { show: vi.fn() };
  TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting(), { provide: GbtToastService, useValue: toast }] });
  const http = TestBed.inject(HttpTestingController);
  const fixture = TestBed.createComponent(SmtpSettings);
  fixture.detectChanges();
  const el = fixture.nativeElement as HTMLElement;
  if (initial === 'fail') {
    http.expectOne(URL).flush({ message: 'boom' }, { status: 500, statusText: 'Server Error' });
  } else if (initial !== 'pending') {
    http.expectOne(URL).flush(initial);
  }
  await settle(fixture);

  const input = (label: string) => {
    const host = [...el.querySelectorAll('gbt-input')].find((h) => h.querySelector('label')?.textContent?.includes(label));
    if (!host) throw new Error(`Aucun champ intitulé « ${label} »`);
    return host.querySelector('input') as HTMLInputElement;
  };
  const type = async (label: string, value: string) => {
    const field = input(label);
    field.value = value;
    field.dispatchEvent(new Event('input'));
    await settle(fixture);
  };
  const button = (label: string) => [...el.querySelectorAll('button')].find((b) => text(b)?.includes(label)) as HTMLButtonElement;
  const pick = async (label: string) => {
    [...el.querySelectorAll<HTMLButtonElement>('[role="radio"]')].find((o) => text(o) === label)!.click();
    await settle(fixture);
  };
  const save = async () => {
    button('Enregistrer').click();
    await settle(fixture);
  };
  const security = () => text([...el.querySelectorAll('[role="radio"]')].find((o) => o.getAttribute('aria-checked') === 'true'));
  const errors = () => [...el.querySelectorAll('.gbt-input__error, [role="alert"]')].map((e) => text(e));
  const warning = () => el.querySelector('[data-warning="no-encryption"]');
  const testStatus = () => text(el.querySelector('.smtp-settings__test-status'));
  return { fixture, http, el, toast, input, type, button, pick, save, security, errors, warning, testStatus };
}

async function answer(fixture: ComponentFixture<unknown>, req: TestRequest, body: object | null, fail: boolean | number = false) {
  await Promise.resolve();
  if (fail) {
    req.flush({ message: 'refused' }, { status: fail === true ? 500 : fail, statusText: 'Server Error' });
  } else {
    req.flush(body);
  }
  await settle(fixture);
}

describe('isValidMailbox', () => {
  it('accepts an address with a dotted domain', () => {
    expect(isValidMailbox('a@b.co')).toBe(true);
    expect(isValidMailbox('first.last@mail.example.com')).toBe(true);
  });

  it('refuses what the server refuses', () => {
    for (const bad of ['', 'nobody', 'a@b', 'a@@b.co', 'a b@c.co', '@b.co', 'a@.co', 'a@b.', 'a@b..co']) {
      expect(isValidMailbox(bad), bad).toBe(false);
    }
  });
});

describe('validateSmtpForm', () => {
  const valid = { host: 'smtp.example.com', port: '587', username: '', password: '', passwordSet: false, fromAddress: 'a@b.c', fromName: 'FerrisGit' };

  it('has no error for a valid form', () => {
    expect(validateSmtpForm(valid)).toEqual({ host: null, port: null, password: null, fromAddress: null, fromName: null });
  });

  it('requires a host, a valid port and an address with an @', () => {
    const errors = validateSmtpForm({ ...valid, host: '  ', port: '0', fromAddress: 'nobody' });
    expect(errors.host).not.toBeNull();
    expect(errors.port).not.toBeNull();
    expect(errors.fromAddress).not.toBeNull();
  });

  it.each(['', 'abc', '0', '65536', '25.5', '-1', '5 87'])('rejects the port "%s"', (port) => {
    expect(validateSmtpForm({ ...valid, port }).port).not.toBeNull();
  });

  it.each(['1', '25', '65535', ' 587 '])('accepts the port "%s"', (port) => {
    expect(validateSmtpForm({ ...valid, port }).port).toBeNull();
  });

  it('refuses whitespace inside the host, as the server does, but not around it', () => {
    expect(validateSmtpForm({ ...valid, host: 'smtp .example.com' }).host).not.toBeNull();
    expect(validateSmtpForm({ ...valid, host: 'smtp\texample.com' }).host).not.toBeNull();
    expect(validateSmtpForm({ ...valid, host: ' smtp.example.com ' }).host).toBeNull();
  });

  it('accepts a sender name up to 100 characters and refuses a longer one', () => {
    expect(validateSmtpForm({ ...valid, fromName: 'x'.repeat(100) }).fromName).toBeNull();
    expect(validateSmtpForm({ ...valid, fromName: ' ' + 'x'.repeat(100) + ' ' }).fromName).toBeNull();
    expect(validateSmtpForm({ ...valid, fromName: 'x'.repeat(101) }).fromName).not.toBeNull();
    expect(validateSmtpForm({ ...valid, fromName: '' }).fromName).toBeNull();
  });

  it('requires a password only when a username is set and none is stored', () => {
    expect(validateSmtpForm({ ...valid, username: 'mailer' }).password).not.toBeNull();
    expect(validateSmtpForm({ ...valid, username: 'mailer', password: 'x' }).password).toBeNull();
    expect(validateSmtpForm({ ...valid, username: 'mailer', passwordSet: true }).password).toBeNull();
    expect(validateSmtpForm({ ...valid, username: ' ' }).password).toBeNull();
  });
});

describe('SmtpSettings', () => {
  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  describe('loading', () => {
    it('shows skeleton cards while the settings load, then the two cards with their icons', async () => {
      const { fixture, http, el } = await setup('pending');
      expect(el.querySelector('[aria-busy="true"]')).not.toBeNull();
      expect(el.querySelector('gbt-card:not([aria-hidden="true"])')).toBeNull();
      const skeletons = Array.from(el.querySelectorAll('gbt-card[aria-hidden="true"]'));
      expect(skeletons).toHaveLength(2);
      for (const skeleton of skeletons) {
        const header = skeleton.querySelector(':scope > .gbt-card__header');
        const box = skeleton.querySelector(':scope > .gbt-card');
        expect(Array.from(skeleton.children)).toEqual([header, box]);
        expect(box?.getAttribute('data-variant')).toBe('outlined');
        expect(header?.querySelector('gbt-skeleton')).toBeTruthy();
      }

      http.expectOne(URL).flush(CONFIGURED);
      await settle(fixture);

      expect(el.querySelector('[aria-busy="true"]')).toBeNull();
      const cards = [...el.querySelectorAll('gbt-card')];
      expect(cards.map((c) => text(c.querySelector('h2')))).toEqual(['Serveur SMTP', "Tester l'envoi"]);
      const icons = fixture.debugElement.queryAll(By.css('.gbt-card__icon')).map((d) => (d.query(By.directive(Icon)).componentInstance as Icon).name());
      expect(icons).toEqual(['mail', 'send']);
      expect(text(cards[0].querySelector('.gbt-card__description'))).toContain('invitations, alertes de sécurité');
    });

    it('fills the fields with the saved settings, the password empty with a placeholder saying it is kept', async () => {
      const { input, security } = await setup();

      expect(input('Hôte').value).toBe('smtp.example.com');
      expect(input('Port').value).toBe('465');
      expect(security()).toBe('TLS');
      expect(input('Identifiant').value).toBe('mailer');
      expect(input('Mot de passe').value).toBe('');
      expect(input('Mot de passe').placeholder).toBe('•••••••• (inchangé)');
      expect(input('Mot de passe').type).toBe('password');
      expect(input('Mot de passe').autocomplete).toBe('new-password');
      expect(input("Adresse d'expédition").value).toBe('ferrisgit@example.com');
      expect(input("Adresse d'expédition").type).toBe('email');
      expect(input("Nom d'expéditeur").value).toBe('Équipe FerrisGit');
    });

    it('describes the fields that need a word of help by that help (aria-describedby), not the others', async () => {
      const { el } = await setup();

      expect(describedBy(el, 'username')).toEqual(["Laissez vide si le serveur n'exige pas d'authentification."]);
      expect(describedBy(el, 'password')).toEqual(['Chiffré dans la base, jamais réaffiché.']);
      expect(describedBy(el, 'host')).toEqual([]);
    });

    it('starts from the defaults when nothing is configured: port 587, STARTTLS, the name FerrisGit, no password placeholder', async () => {
      const { input, security, el } = await setup(UNCONFIGURED);

      expect(input('Hôte').value).toBe('');
      expect(input('Hôte').placeholder).toBe('smtp.example.com');
      expect(input('Port').value).toBe('587');
      expect(security()).toBe('STARTTLS');
      expect(input("Nom d'expéditeur").value).toBe('FerrisGit');
      expect(input('Mot de passe').placeholder).toBe('');
      expect(text(el.querySelector('.smtp-settings__summary'))).toContain('Aucun serveur configuré');
    });

    it('shows a toast and a retry card when loading fails; retrying loads again', async () => {
      const { fixture, http, el, toast, button } = await setup('fail');

      expect(toast.show).toHaveBeenCalledWith('Impossible de charger les réglages e-mail. Réessayez plus tard.', 'error');
      const failed = el.querySelector('gbt-alert .gbt-alert');
      expect(text(failed)).toContain("n'ont pas pu être chargés");
      expect(failed?.getAttribute('data-variant')).toBe('error');
      expect(failed?.getAttribute('role')).toBeNull();
      expect(failed?.contains(button('Réessayer'))).toBe(true);
      expect(el.querySelectorAll('[role="alert"], [role="status"], [aria-live]')).toHaveLength(0);
      expect(el.querySelector('form')).toBeNull();

      button('Réessayer').click();
      fixture.detectChanges();
      expect(el.querySelector('[aria-busy="true"]')).not.toBeNull();
      http.expectOne(URL).flush(CONFIGURED);
      await settle(fixture);
      expect(el.querySelector('gbt-alert')).toBeNull();
      expect(el.querySelectorAll('gbt-card')).toHaveLength(2);
    });
  });

  describe('saving', () => {
    it('disables "Enregistrer" while the form is unchanged, and enables it once edited', async () => {
      const { button, type } = await setup();
      expect(button('Enregistrer').disabled).toBe(true);

      await type('Nom d\'expéditeur', 'Autre nom');
      expect(button('Enregistrer').disabled).toBe(false);

      await type('Nom d\'expéditeur', 'Équipe FerrisGit');
      expect(button('Enregistrer').disabled).toBe(true);
    });

    it('has exactly one primary button in the whole section, "Enregistrer"', async () => {
      const { el } = await setup();
      const primary = el.querySelectorAll('.gbt-button--primary');
      expect(primary).toHaveLength(1);
      expect(text(primary[0])).toBe('Enregistrer');
    });

    it('sends the values trimmed, the port as a number and no password key when none was typed', async () => {
      const { fixture, http, type, save } = await setup();
      await type('Hôte', '  smtp.other.org ');
      await type('Port', '587');

      await save();
      const put = http.expectOne(URL);
      expect(put.request.method).toBe('PUT');
      expect(put.request.body).toEqual({
        host: 'smtp.other.org',
        port: 587,
        security: 'tls',
        username: 'mailer',
        fromAddress: 'ferrisgit@example.com',
        fromName: 'Équipe FerrisGit',
      });
      expect('password' in put.request.body).toBe(false);
      await answer(fixture, put, { ...CONFIGURED, host: 'smtp.other.org', port: 587 });
    });

    it('sends the password when one was typed', async () => {
      const { fixture, http, type, save } = await setup();
      await type('Mot de passe', 'S3cret!');

      await save();
      const put = http.expectOne(URL);
      expect(put.request.body.password).toBe('S3cret!');
      await answer(fixture, put, CONFIGURED);
    });

    it('sends the picked security', async () => {
      const { fixture, http, pick, save } = await setup();
      await pick('STARTTLS');

      await save();
      const put = http.expectOne(URL);
      expect(put.request.body.security).toBe('starttls');
      await answer(fixture, put, { ...CONFIGURED, security: 'starttls' });
    });

    it('shows a toast, re-hydrates the form from the response and clears the password on success', async () => {
      const { fixture, http, type, save, input, toast, button, el } = await setup(UNCONFIGURED);
      await type('Hôte', 'smtp.example.com');
      await type('Identifiant', 'mailer');
      await type('Mot de passe', 'S3cret!');
      await type("Adresse d'expédition", 'ferrisgit@example.com');

      await save();
      const put = http.expectOne(URL);
      expect(button('Enregistrer').classList.contains('gbt-button--loading') || button('Enregistrer').disabled).toBe(true);
      await answer(fixture, put, { ...CONFIGURED, port: 587, security: 'starttls', fromName: 'FerrisGit' });

      expect(toast.show).toHaveBeenCalledWith('Réglages e-mail enregistrés');
      expect(input('Mot de passe').value).toBe('');
      expect(input('Mot de passe').placeholder).toBe('•••••••• (inchangé)');
      expect(input('Hôte').value).toBe('smtp.example.com');
      expect(button('Enregistrer').disabled).toBe(true);
      expect(text(el.querySelector('.smtp-settings__summary'))).toBe('Réglages enregistrés');
    });

    it('shows a toast and keeps the fields as typed when the save fails', async () => {
      const { fixture, http, type, save, input, toast, button, el } = await setup();
      await type('Hôte', 'smtp.other.org');
      await type('Mot de passe', 'S3cret!');

      await save();
      await answer(fixture, http.expectOne(URL), null, true);

      expect(toast.show).toHaveBeenCalledWith("Échec de l'enregistrement. Réessayez.", 'error');
      expect(input('Hôte').value).toBe('smtp.other.org');
      expect(input('Mot de passe').value).toBe('S3cret!');
      expect(button('Enregistrer').disabled).toBe(false);
      expect(text(el.querySelector('.smtp-settings__summary'))).toBe('Modifications non enregistrées');
    });

    it('says the settings were refused, not to retry, when the server answers 400', async () => {
      const { fixture, http, type, save, toast, input, button } = await setup();
      await type('Hôte', 'smtp.other.org');

      await save();
      await answer(fixture, http.expectOne(URL), null, 400);

      expect(toast.show).toHaveBeenCalledWith('Réglages refusés : vérifiez les champs.', 'error');
      expect(toast.show).not.toHaveBeenCalledWith("Échec de l'enregistrement. Réessayez.", 'error');
      expect(input('Hôte').value).toBe('smtp.other.org');
      expect(button('Enregistrer').disabled).toBe(false);
    });

    it('refuses a host with a space inside and a sender name over 100 characters, without a request', async () => {
      const { http, type, save, errors } = await setup();
      await type('Hôte', 'smtp .example.com');
      await type("Nom d'expéditeur", 'x'.repeat(101));

      await save();

      http.expectNone(URL);
      expect(errors()).toEqual(["Le serveur ne peut pas contenir d'espace", '100 caractères au maximum']);
    });

    it('sends nothing and shows no error until a save is attempted', async () => {
      const { type, errors } = await setup(UNCONFIGURED);
      await type('Port', 'abc');
      expect(errors()).toEqual([]);
    });

    it('refuses a missing host, a bad port and a bad address, with an error on each field and no request', async () => {
      const { http, type, save, errors } = await setup(UNCONFIGURED);
      await type('Port', '70000');
      await type("Adresse d'expédition", 'nobody');

      await save();

      http.expectNone(URL);
      expect(errors()).toEqual(['Indiquez le serveur SMTP', 'Entre 1 et 65535', 'Entrez une adresse e-mail valide']);
    });

    it('refuses a username without a password when none is stored, and accepts one when it is', async () => {
      const { http, type, save, errors, fixture, input } = await setup(UNCONFIGURED);
      await type('Hôte', 'smtp.example.com');
      await type("Adresse d'expédition", 'ferrisgit@example.com');
      await type('Identifiant', 'mailer');

      await save();
      http.expectNone(URL);
      expect(errors()).toEqual(['Indiquez le mot de passe de ce compte']);

      await type('Mot de passe', 'S3cret!');
      expect(errors()).toEqual([]);
      expect(input('Mot de passe').value).toBe('S3cret!');
      await save();
      await answer(fixture, http.expectOne(URL), { ...CONFIGURED, port: 587 });
    });

    it('does not ask again for a password that is already stored', async () => {
      const { fixture, http, type, save } = await setup();
      await type('Identifiant', 'someone-else');

      await save();
      await answer(fixture, http.expectOne(URL), { ...CONFIGURED, username: 'someone-else' });
    });
  });

  describe('security', () => {
    it('is a radio group named by its visible label "Sécurité"', async () => {
      const { el } = await setup();

      const group = el.querySelector('[data-field="security"] [role="radiogroup"]')!;
      expect(group.hasAttribute('aria-label')).toBe(false);
      expect(text(el.querySelector(`#${group.getAttribute('aria-labelledby')}`))).toBe('Sécurité');
    });

    it('lists Aucune, STARTTLS and TLS', async () => {
      const { el } = await setup();
      expect([...el.querySelectorAll('[role="radio"]')].map(text)).toEqual(['Aucune', 'STARTTLS', 'TLS']);
    });

    it('warns that credentials travel in clear text when there is no encryption, and only then', async () => {
      const { fixture, pick, warning } = await setup();
      expect(warning()).toBeNull();

      await pick('Aucune');
      expect(text(warning())).toBe("Sans chiffrement, l'identifiant et le mot de passe transitent en clair. À réserver à un relais interne de confiance.");
      expect(warning()?.querySelector('[role="alert"]')).not.toBeNull();
      const icon = fixture.debugElement.query(By.css('[data-warning="no-encryption"] gbt-icon')).componentInstance as Icon;
      expect(icon.name()).toBe('alert-triangle');

      await pick('STARTTLS');
      expect(warning()).toBeNull();
    });

    it('shows the warning for settings saved without encryption', async () => {
      const { warning } = await setup({ ...CONFIGURED, security: 'none', port: 25 });
      expect(warning()).not.toBeNull();
    });
  });

  describe('test send', () => {
    const sendButton = (button: (label: string) => HTMLButtonElement) => button('Envoyer un e-mail de test');

    it('is disabled for an empty or invalid recipient, then enabled', async () => {
      const { button, type } = await setup();
      expect(sendButton(button).disabled).toBe(true);

      await type('Destinataire', 'nobody');
      expect(sendButton(button).disabled).toBe(true);

      await type('Destinataire', 'admin@example.com');
      expect(sendButton(button).disabled).toBe(false);
    });

    it('is disabled, with the reason, while the form has unsaved changes', async () => {
      const { button, type, el } = await setup();
      await type('Destinataire', 'admin@example.com');
      expect(describedBy(el, 'testRecipient')).toEqual(['Le test utilise les réglages enregistrés.']);

      await type('Port', '25');
      expect(sendButton(button).disabled).toBe(true);
      expect(describedBy(el, 'testRecipient')).toEqual(['Enregistrez les réglages avant de tester.']);

      await type('Port', '465');
      expect(sendButton(button).disabled).toBe(false);
    });

    it('is disabled while nothing is saved yet', async () => {
      const { button, type } = await setup(UNCONFIGURED);
      await type('Destinataire', 'admin@example.com');
      expect(sendButton(button).disabled).toBe(true);
    });

    it('posts the trimmed recipient, disables the button while sending, then says the e-mail was sent', async () => {
      const { fixture, http, button, type, testStatus, el } = await setup();
      await type('Destinataire', ' admin@example.com ');

      sendButton(button).click();
      await settle(fixture);
      const post = http.expectOne(TEST_URL);
      expect(post.request.method).toBe('POST');
      expect(post.request.body).toEqual({ to: 'admin@example.com' });
      expect(sendButton(button).disabled).toBe(true);
      expect(testStatus()).toBe('');

      await answer(fixture, post, { sent: true });
      expect(testStatus()).toBe('E-mail de test envoyé à admin@example.com');
      expect(el.querySelector('.smtp-settings__test-status')?.getAttribute('role')).toBe('status');
      expect(el.querySelector('.smtp-settings__banner')?.getAttribute('data-state')).toBe('sent');
      expect(sendButton(button).disabled).toBe(false);
    });

    it('shows the error the server reports when the send failed', async () => {
      const { fixture, http, button, type, testStatus, el } = await setup();
      await type('Destinataire', 'admin@example.com');

      sendButton(button).click();
      await settle(fixture);
      await answer(fixture, http.expectOne(TEST_URL), { sent: false, error: 'authentication failed' });

      expect(testStatus()).toBe("Échec de l'envoi : authentication failed");
      expect(el.querySelector('.smtp-settings__test-status')?.getAttribute('role')).toBe('status');
      expect(el.querySelector('.smtp-settings__test-status [role="alert"]')).toBeNull();
      expect(el.querySelector('.smtp-settings__banner')?.getAttribute('data-state')).toBe('failed');
    });

    it('shows a generic failure when the request itself fails', async () => {
      const { fixture, http, button, type, testStatus } = await setup();
      await type('Destinataire', 'admin@example.com');

      sendButton(button).click();
      await settle(fixture);
      await answer(fixture, http.expectOne(TEST_URL), null, true);

      expect(testStatus()).toContain("Échec de l'envoi");
      expect(testStatus()).not.toContain('refused');
    });

    it('clears the previous result when sending again', async () => {
      const { fixture, http, button, type, testStatus } = await setup();
      await type('Destinataire', 'admin@example.com');
      sendButton(button).click();
      await settle(fixture);
      await answer(fixture, http.expectOne(TEST_URL), { sent: true });
      expect(testStatus()).not.toBe('');

      sendButton(button).click();
      await settle(fixture);
      expect(testStatus()).toBe('');
      await answer(fixture, http.expectOne(TEST_URL), { sent: true });
    });
  });

  describe('bindings', () => {
    it('gives the segmented control the same options array on every change detection (no fresh literal)', async () => {
      const { fixture, pick } = await setup();
      const control = () => fixture.debugElement.query(By.directive(SegmentedControl)).componentInstance as SegmentedControl<string>;

      const before = control().options();
      fixture.detectChanges();
      await pick('Aucune');
      fixture.detectChanges();

      expect(control().options()).toBe(before);
    });
  });
});
