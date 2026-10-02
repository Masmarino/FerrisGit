import { TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { provideRouter } from '@angular/router';
import { RouterTestingHarness } from '@angular/router/testing';
import { Icon, GbtToastService } from '@masmarino/gabarit';
import { AdminSettings } from './admin-settings';
import { SecuritySettings } from '../security-settings/security-settings';
import { SystemSettings } from '../settings.service';
import { PageTitleService } from '../../shell/page-title.service';

const SETTINGS: SystemSettings = {
  executionEngine: 'docker-runners',
  jwtTtlHours: 8,
  maxPushSizeMb: 100,
  registrationEnabled: false,
  publicPagesEnabled: true,
  seoIndexingEnabled: false,
  k8sNamespace: null,
  k8sCacheStorageClass: null,
  runnerRegistrationTokenConfigured: false,
  logRetentionDays: null,
  maxConcurrentJobs: null,
  detectedK8sNamespace: null,
  detectedK8sDefaultStorageClass: null,
};

const SETTINGS_URL = '/admin/settings';
const SECURITY_URL = '/admin/settings?section=security';
const EMAIL_URL = '/admin/settings?section=email';

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();

async function setup(url = SETTINGS_URL, settings: SystemSettings = SETTINGS) {
  const toastStub = { show: vi.fn() };
  const pageTitleStub = { set: vi.fn() };
  TestBed.configureTestingModule({
    providers: [
      provideHttpClient(),
      provideHttpClientTesting(),
      provideRouter([{ path: 'admin/settings', component: AdminSettings }]),
      { provide: GbtToastService, useValue: toastStub },
      { provide: PageTitleService, useValue: pageTitleStub },
    ],
  });
  const http = TestBed.inject(HttpTestingController);
  const harness = await RouterTestingHarness.create(url);
  if (settings) {
    http.expectOne('/api/admin/settings').flush(settings);
  }
  await settle(harness);
  const el = () => harness.routeNativeElement as HTMLElement;
  const navigate = async (target: string) => {
    await harness.navigateByUrl(target);
    await settle(harness);
  };
  return { harness, http, el, navigate, toastStub, pageTitleStub };
}

async function settle(harness: RouterTestingHarness) {
  harness.fixture.detectChanges();
  await harness.fixture.whenStable();
  harness.fixture.detectChanges();
}

function fieldLabelled(root: HTMLElement, label: string): HTMLInputElement {
  const field = [...root.querySelectorAll('gbt-input')].find((host) => host.querySelector('label')?.textContent?.includes(label));
  if (!field) throw new Error(`Aucun champ intitulé « ${label} »`);
  return field.querySelector('input') as HTMLInputElement;
}

function commit(input: HTMLInputElement, value: string) {
  input.value = value;
  input.dispatchEvent(new Event('input'));
  input.dispatchEvent(new Event('blur'));
}

const saveState = (root: HTMLElement, field: string) => root.querySelector(`[data-field="${field}"] [role="status"]`);
const engineOptions = (root: HTMLElement) => Array.from(root.querySelectorAll<HTMLButtonElement>('[data-field="executionEngine"] [role="radio"]'));
const checkedEngine = (root: HTMLElement) => text(engineOptions(root).find((o) => o.getAttribute('aria-checked') === 'true'));

describe('AdminSettings', () => {
  afterEach(() => {
    TestBed.inject(HttpTestingController).verify();
  });

  it('sets the page title', async () => {
    const { pageTitleStub } = await setup();
    expect(pageTitleStub.set).toHaveBeenCalledWith("Réglages de l'instance");
  });

  describe('the page', () => {
    it('is a wide page: the h1 above a layout whose nav column holds the section navigation', async () => {
      const { el } = await setup();

      expect(text(el().querySelector('gbt-page-header h1'))).toBe("Réglages de l'instance");
      const layout = el().querySelector('gbt-page-layout')!;
      expect(layout.getAttribute('data-width')).toBe('wide');
      const nav = layout.querySelector('nav.gbt-page-layout__nav')!;
      expect(nav.getAttribute('aria-label')).toBe("Réglages de l'instance");
      expect(nav.querySelector('gbt-nav-tabs')).not.toBeNull();
      expect(el().querySelectorAll('nav')).toHaveLength(1);
    });

    it('lists the three sections as links to ?section=<key> (the default one on the bare path)', async () => {
      const { el } = await setup();

      const links = Array.from(el().querySelectorAll<HTMLAnchorElement>('gbt-nav-tabs a'));
      expect(links.map((a) => text(a.querySelector('.gbt-nav-tab__label')))).toEqual(['Exécution', 'Sécurité', 'E-mail']);
      expect(links.map((a) => a.getAttribute('href'))).toEqual([SETTINGS_URL, SECURITY_URL, EMAIL_URL]);
      expect(links.every((a) => a.querySelector('gbt-icon') !== null)).toBe(true);
      expect(links.map((a) => a.getAttribute('aria-current'))).toEqual(['page', null, null]);
    });

    it('shows the execution section by default: the engine, Docker runners, Kubernetes and job log cards, each with its icon', async () => {
      const { harness, el } = await setup();

      const cards = harness.routeDebugElement!.queryAll(By.css('gbt-card'));
      expect(cards.map((card) => text(card.nativeElement.querySelector('h2')))).toEqual(["Moteur d'exécution", 'Runners Docker', 'Kubernetes', 'Journaux des jobs']);
      expect(cards.map((card) => (card.query(By.directive(Icon))?.componentInstance as Icon | undefined)?.name())).toEqual(['server', 'key', 'layers', 'clock']);
      expect(cards.every((card) => !!text(card.nativeElement.querySelector('.gbt-card__description')))).toBe(true);
      expect(el().querySelector('gbt-slider')).toBeNull();
    });

    it('shows the security section for ?section=security: the sessions, registration, public pages and push-limit cards, each with its icon', async () => {
      const { harness, el } = await setup(SECURITY_URL);

      const cards = harness.routeDebugElement!.queryAll(By.css('gbt-card'));
      expect(cards.map((card) => text(card.nativeElement.querySelector('h2')))).toEqual(['Sessions', 'Inscription', 'Pages publiques', 'Pushs']);
      expect(cards.map((card) => (card.query(By.directive(Icon))?.componentInstance as Icon | undefined)?.name())).toEqual(['lock', 'user-plus', 'globe', 'upload']);
      expect(el().querySelector('gbt-segmented-control')).toBeNull();
      expect(text(el().querySelector('gbt-nav-tabs a[aria-current="page"] .gbt-nav-tab__label'))).toBe('Sécurité');
    });

    it('shows the e-mail section for ?section=email: the SMTP settings, loaded once the instance settings are', async () => {
      const { el, http, harness } = await setup(EMAIL_URL);

      const smtp = http.expectOne('/api/admin/settings/smtp');
      expect(el().querySelector('fg-smtp-settings')).not.toBeNull();
      smtp.flush({ configured: false, host: '', port: 587, security: 'starttls', username: '', passwordSet: false, fromAddress: '', fromName: 'FerrisGit' });
      await settle(harness);

      const cards = harness.routeDebugElement!.queryAll(By.css('gbt-card'));
      expect(cards.map((card) => text(card.nativeElement.querySelector('h2')))).toEqual(['Serveur SMTP', "Tester l'envoi"]);
      expect(el().querySelector('gbt-slider')).toBeNull();
      expect(text(el().querySelector('gbt-nav-tabs a[aria-current="page"] .gbt-nav-tab__label'))).toBe('E-mail');
    });

    it('does not load the SMTP settings in the other sections', async () => {
      const { el, http } = await setup();

      http.expectNone('/api/admin/settings/smtp');
      expect(el().querySelector('fg-smtp-settings')).toBeNull();
    });

    it('mentions the e-mails in the intro line', async () => {
      const { el } = await setup();
      expect(text(el().querySelector('.admin-settings__intro'))).toBe('Exécution des pipelines, sessions, limites et e-mails, pour tous les utilisateurs');
    });

    it('falls back to the execution section for an unknown section', async () => {
      const { el } = await setup('/admin/settings?section=inconnue');
      expect(text(el().querySelector('gbt-nav-tabs a[aria-current="page"] .gbt-nav-tab__label'))).toBe('Exécution');
      expect(el().querySelector('gbt-segmented-control')).not.toBeNull();
    });

    it('loads the settings once: moving between sections does not load them again', async () => {
      const { el, navigate, http } = await setup();

      await navigate(SECURITY_URL);
      await navigate(SETTINGS_URL);

      http.expectNone('/api/admin/settings');
      expect(el().querySelector('gbt-segmented-control')).not.toBeNull();
    });
  });

  describe('loading', () => {
    it('shows skeleton cards while the settings load, then the section', async () => {
      TestBed.configureTestingModule({
        providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([{ path: 'admin/settings', component: AdminSettings }]), { provide: GbtToastService, useValue: { show: vi.fn() } }],
      });
      const http = TestBed.inject(HttpTestingController);
      const harness = await RouterTestingHarness.create(SETTINGS_URL);
      const el = harness.routeNativeElement as HTMLElement;

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

      http.expectOne('/api/admin/settings').flush(SETTINGS);
      await settle(harness);
      expect(el.querySelector('[aria-busy="true"]')).toBeNull();
      expect(el.querySelectorAll('gbt-card')).toHaveLength(4);
    });

    it('shows a toast and a retry card when loading fails; retrying loads again', async () => {
      const toastStub = { show: vi.fn() };
      TestBed.configureTestingModule({
        providers: [provideHttpClient(), provideHttpClientTesting(), provideRouter([{ path: 'admin/settings', component: AdminSettings }]), { provide: GbtToastService, useValue: toastStub }],
      });
      const http = TestBed.inject(HttpTestingController);
      const harness = await RouterTestingHarness.create(SETTINGS_URL);
      http.expectOne('/api/admin/settings').flush({ message: 'boom' }, { status: 500, statusText: 'Server Error' });
      await settle(harness);
      const el = harness.routeNativeElement as HTMLElement;

      expect(toastStub.show).toHaveBeenCalledWith('Impossible de charger les réglages. Réessayez plus tard.', 'error');
      const failed = el.querySelector('gbt-alert .gbt-alert');
      expect(text(failed)).toContain("Les réglages n'ont pas pu être chargés");
      expect(failed?.getAttribute('data-variant')).toBe('error');
      expect(failed?.getAttribute('role')).toBeNull();
      expect(failed?.getAttribute('aria-live')).toBeNull();
      expect(failed?.querySelector('button')?.textContent?.trim()).toBe('Réessayer');
      expect(el.querySelectorAll('[role="alert"], [aria-live]')).toHaveLength(0);

      Array.from(el.querySelectorAll('button')).find((b) => text(b) === 'Réessayer')!.click();
      http.expectOne('/api/admin/settings').flush(SETTINGS);
      await settle(harness);
      expect(el.querySelector('gbt-alert .gbt-alert[data-variant="error"]')).toBeNull();
      expect(el.querySelector('gbt-segmented-control')).not.toBeNull();
    });
  });

  describe('execution engine', () => {
    it('saves the engine picked and marks it "Enregistré"', async () => {
      const { harness, el, http } = await setup();
      expect(checkedEngine(el())).toBe('Docker / runners');

      engineOptions(el())[1].click();
      await settle(harness);
      const saved = http.expectOne('/api/admin/settings');
      expect(saved.request.method).toBe('PUT');
      expect(saved.request.body).toEqual({ executionEngine: 'kubernetes' });
      expect(text(saveState(el(), 'executionEngine'))).toBe('Enregistrement…');

      saved.flush({ ...SETTINGS, executionEngine: 'kubernetes' });
      await settle(harness);
      expect(checkedEngine(el())).toBe('Kubernetes');
      expect(text(saveState(el(), 'executionEngine'))).toBe('Enregistré');
      expect(saveState(el(), 'executionEngine')?.getAttribute('data-state')).toBe('saved');
    });

    it('shows the engine picked while it is saved, and the previous one again when the save fails', async () => {
      const { harness, el, http, toastStub } = await setup();

      engineOptions(el())[1].click();
      await settle(harness);
      const refused = http.expectOne('/api/admin/settings');
      expect(checkedEngine(el())).toBe('Kubernetes');

      refused.flush({ message: 'boom' }, { status: 500, statusText: 'Server Error' });
      await settle(harness);

      expect(checkedEngine(el())).toBe('Docker / runners');
      expect(text(saveState(el(), 'executionEngine'))).toBe('Non enregistré');
      expect(saveState(el(), 'executionEngine')?.getAttribute('data-state')).toBe('error');
      expect(toastStub.show).toHaveBeenCalledWith("Échec de l'enregistrement. Réessayez.", 'error');
    });
  });

  describe('Docker runners', () => {
    const CONFIGURED: SystemSettings = { ...SETTINGS, runnerRegistrationTokenConfigured: true };
    const tokenField = (root: HTMLElement) => root.querySelector('[data-field="runnerRegistrationToken"]')!;
    const buttonNamed = (root: ParentNode, label: string) => Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === label);
    const dialog = (root: HTMLElement) => root.querySelector('gbt-confirm-danger-modal');

    describe('registration token', () => {
      it('says that no token is configured, and offers no removal', async () => {
        const { el } = await setup();

        expect(text(tokenField(el()).querySelector('gbt-badge'))).toBe('Non configuré');
        expect(text(tokenField(el()).querySelector('gbt-input label'))).toBe('Définir un jeton');
        expect(buttonNamed(tokenField(el()), 'Supprimer le jeton')).toBeUndefined();
      });

      it('says that a token is configured without ever showing it: the field is masked and empty', async () => {
        const { el } = await setup(SETTINGS_URL, CONFIGURED);

        expect(text(tokenField(el()).querySelector('gbt-badge'))).toBe('Configuré');
        expect(text(tokenField(el()).querySelector('gbt-input label'))).toBe('Remplacer le jeton');
        expect(buttonNamed(tokenField(el()), 'Supprimer le jeton')).toBeDefined();
        expect(fieldLabelled(el(), 'Remplacer le jeton').type).toBe('password');
        expect(fieldLabelled(el(), 'Remplacer le jeton').value).toBe('');
      });

      it('saves the token typed on blur, empties the field, and never shows it again', async () => {
        const { harness, el, http } = await setup();

        const field = fieldLabelled(el(), 'Définir un jeton');
        field.value = '  mon-jeton-secret  ';
        field.dispatchEvent(new Event('input'));
        await settle(harness); // as in a browser, the page renders between typing and leaving the field
        field.dispatchEvent(new Event('blur'));
        const saved = http.expectOne('/api/admin/settings');
        expect(saved.request.method).toBe('PUT');
        expect(saved.request.body).toEqual({ runnerRegistrationToken: 'mon-jeton-secret' });
        saved.flush(CONFIGURED);
        await settle(harness);

        expect(text(saveState(el(), 'runnerRegistrationToken'))).toBe('Enregistré');
        expect(text(tokenField(el()).querySelector('gbt-badge'))).toBe('Configuré');
        expect(fieldLabelled(el(), 'Remplacer le jeton').value).toBe('');
        expect(el().textContent).not.toContain('mon-jeton-secret');
        expect(tokenField(el()).querySelector('gbt-secret-reveal')).toBeNull();
      });

      it('saves nothing for an empty field: removing the token has its own button', async () => {
        const { el, http } = await setup(SETTINGS_URL, CONFIGURED);

        commit(fieldLabelled(el(), 'Remplacer le jeton'), '   ');

        http.expectNone('/api/admin/settings');
      });

      it('says "Non enregistré" with a toast when the save is refused', async () => {
        const { harness, el, http, toastStub } = await setup();

        commit(fieldLabelled(el(), 'Définir un jeton'), 'abc');
        http.expectOne('/api/admin/settings').flush({ message: 'boom' }, { status: 500, statusText: 'Server Error' });
        await settle(harness);

        expect(text(saveState(el(), 'runnerRegistrationToken'))).toBe('Non enregistré');
        expect(toastStub.show).toHaveBeenCalledWith("Échec de l'enregistrement. Réessayez.", 'error');
        expect(text(tokenField(el()).querySelector('gbt-badge'))).toBe('Non configuré');
      });

      it('generates a random 256-bit token with one click when none exists, and shows it once', async () => {
        const { harness, el, http } = await setup();

        buttonNamed(tokenField(el()), 'Générer un jeton')!.click();
        await settle(harness);
        const saved = http.expectOne('/api/admin/settings');
        const token = (saved.request.body as { runnerRegistrationToken: string }).runnerRegistrationToken;
        expect(token).toMatch(/^[0-9a-f]{64}$/);
        expect(dialog(el())).toBeNull();
        expect(tokenField(el()).querySelector('gbt-secret-reveal')).toBeNull();

        saved.flush(CONFIGURED);
        await settle(harness);
        const reveal = tokenField(el()).querySelector('gbt-secret-reveal');
        expect(reveal).not.toBeNull();
        expect(text(reveal)).toContain(token);
        expect(text(tokenField(el()))).toContain('Copiez-le maintenant : il ne sera plus jamais affiché.');

        buttonNamed(tokenField(el()), "J'ai copié le jeton")!.click();
        await settle(harness);
        expect(tokenField(el()).querySelector('gbt-secret-reveal')).toBeNull();
        expect(el().textContent).not.toContain(token);
      });

      it('generates a different token each time', async () => {
        const { harness, el, http } = await setup();
        const generated = async () => {
          buttonNamed(tokenField(el()), 'Générer un jeton')!.click();
          await settle(harness);
          const request = http.expectOne('/api/admin/settings');
          request.flush(SETTINGS); // not configured: the next click generates without a confirmation
          await settle(harness);
          return (request.request.body as { runnerRegistrationToken: string }).runnerRegistrationToken;
        };

        const first = await generated();
        const second = await generated();

        expect(first).not.toBe(second);
      });

      it('does not show a generated token whose save was refused', async () => {
        const { harness, el, http } = await setup();

        buttonNamed(tokenField(el()), 'Générer un jeton')!.click();
        http.expectOne('/api/admin/settings').flush({ message: 'boom' }, { status: 500, statusText: 'Server Error' });
        await settle(harness);

        expect(tokenField(el()).querySelector('gbt-secret-reveal')).toBeNull();
      });

      it('asks before replacing an existing token by a generated one, and saves nothing on cancel', async () => {
        const { harness, el, http } = await setup(SETTINGS_URL, CONFIGURED);

        buttonNamed(tokenField(el()), 'Générer un nouveau jeton')!.click();
        await settle(harness);
        http.expectNone('/api/admin/settings');
        expect(text(dialog(el()))).toContain("Remplacer le jeton d'enregistrement");
        expect(text(dialog(el()))).toContain('ne permettra plus d\'enregistrer de nouveaux runners');

        buttonNamed(dialog(el())!, 'Annuler')!.click();
        await settle(harness);
        expect(dialog(el())).toBeNull();
        http.expectNone('/api/admin/settings');
      });

      it('replaces the token once confirmed, and shows the new one once', async () => {
        const { harness, el, http } = await setup(SETTINGS_URL, CONFIGURED);
        buttonNamed(tokenField(el()), 'Générer un nouveau jeton')!.click();
        await settle(harness);

        buttonNamed(dialog(el())!, 'Générer un jeton')!.click();
        await settle(harness);
        const saved = http.expectOne('/api/admin/settings');
        const token = (saved.request.body as { runnerRegistrationToken: string }).runnerRegistrationToken;
        expect(token).toMatch(/^[0-9a-f]{64}$/);
        saved.flush(CONFIGURED);
        await settle(harness);

        expect(dialog(el())).toBeNull();
        expect(text(tokenField(el()).querySelector('gbt-secret-reveal'))).toContain(token);
      });

      it('removes the token after a confirmation that says its consequence', async () => {
        const { harness, el, http } = await setup(SETTINGS_URL, CONFIGURED);

        buttonNamed(tokenField(el()), 'Supprimer le jeton')!.click();
        await settle(harness);
        http.expectNone('/api/admin/settings');
        expect(text(dialog(el()))).toContain("Supprimer le jeton d'enregistrement");
        expect(text(dialog(el()))).toContain("ne pourront plus s'enregistrer eux-mêmes");

        buttonNamed(dialog(el())!, 'Supprimer le jeton')!.click();
        await settle(harness);
        const saved = http.expectOne('/api/admin/settings');
        expect(saved.request.body).toEqual({ runnerRegistrationToken: null });
        saved.flush(SETTINGS);
        await settle(harness);

        expect(text(tokenField(el()).querySelector('gbt-badge'))).toBe('Non configuré');
        expect(dialog(el())).toBeNull();
      });

      it('keeps the token when the removal is cancelled', async () => {
        const { harness, el, http } = await setup(SETTINGS_URL, CONFIGURED);
        buttonNamed(tokenField(el()), 'Supprimer le jeton')!.click();
        await settle(harness);

        buttonNamed(dialog(el())!, 'Annuler')!.click();
        await settle(harness);

        http.expectNone('/api/admin/settings');
        expect(text(tokenField(el()).querySelector('gbt-badge'))).toBe('Configuré');
      });
    });

    describe('simultaneous jobs', () => {
      const error = (root: HTMLElement) => text(root.querySelector('[data-field="maxConcurrentJobs"] .gbt-input__error'));

      it('is labelled for Docker runners, empty means unlimited, and says it does not apply to Kubernetes', async () => {
        const { el } = await setup();

        expect(fieldLabelled(el(), 'Jobs simultanés (runners Docker)').value).toBe('');
        expect(fieldLabelled(el(), 'Jobs simultanés (runners Docker)').placeholder).toBe('Illimité');
        expect(text(el().querySelector('[data-field="maxConcurrentJobs"]'))).toContain('ne s\'applique pas aux jobs lancés par Kubernetes');
      });

      it('shows the saved ceiling', async () => {
        const { el } = await setup(SETTINGS_URL, { ...SETTINGS, maxConcurrentJobs: 4 });
        expect(fieldLabelled(el(), 'Jobs simultanés (runners Docker)').value).toBe('4');
      });

      it('saves a whole number on blur', async () => {
        const { harness, el, http } = await setup();

        commit(fieldLabelled(el(), 'Jobs simultanés (runners Docker)'), '3');
        const saved = http.expectOne('/api/admin/settings');
        expect(saved.request.body).toEqual({ maxConcurrentJobs: 3 });
        saved.flush({ ...SETTINGS, maxConcurrentJobs: 3 });
        await settle(harness);

        expect(text(saveState(el(), 'maxConcurrentJobs'))).toBe('Enregistré');
      });

      it('saves an emptied field as null: no limit', async () => {
        const { el, http } = await setup(SETTINGS_URL, { ...SETTINGS, maxConcurrentJobs: 4 });

        commit(fieldLabelled(el(), 'Jobs simultanés (runners Docker)'), '  ');

        expect(http.expectOne('/api/admin/settings').request.body).toEqual({ maxConcurrentJobs: null });
      });

      it('saves nothing when the value is unchanged', async () => {
        const { el, http } = await setup(SETTINGS_URL, { ...SETTINGS, maxConcurrentJobs: 4 });

        commit(fieldLabelled(el(), 'Jobs simultanés (runners Docker)'), '4');

        http.expectNone('/api/admin/settings');
      });

      // A number field cannot hold letters, so only numbers that are not whole counts reach the handler.
      it.each(['0', '-2', '1.5'])('explains that "%s" is not valid instead of saving it', async (value) => {
        const { harness, el, http } = await setup();

        commit(fieldLabelled(el(), 'Jobs simultanés (runners Docker)'), value);
        await settle(harness);

        expect(error(el())).toBe('Entrez un nombre entier, 1 ou plus, ou laissez vide pour ne pas limiter');
        http.expectNone('/api/admin/settings');
      });

      it('clears the message once the value is valid, and puts the saved value back when the save is refused', async () => {
        const { harness, el, http } = await setup(SETTINGS_URL, { ...SETTINGS, maxConcurrentJobs: 4 });
        commit(fieldLabelled(el(), 'Jobs simultanés (runners Docker)'), '0');
        await settle(harness);
        expect(error(el())).toBeDefined();

        commit(fieldLabelled(el(), 'Jobs simultanés (runners Docker)'), '8');
        await settle(harness);
        expect(error(el())).toBeUndefined();
        http.expectOne('/api/admin/settings').flush({ message: 'boom' }, { status: 500, statusText: 'Server Error' });
        await settle(harness);

        expect(fieldLabelled(el(), 'Jobs simultanés (runners Docker)').value).toBe('4');
        expect(text(saveState(el(), 'maxConcurrentJobs'))).toBe('Non enregistré');
      });
    });
  });

  describe('job log retention', () => {
    const LABEL = 'Durée de conservation des journaux (jours)';
    const error = (root: HTMLElement) => text(root.querySelector('[data-field="logRetentionDays"] .gbt-input__error'));

    it('is empty when the logs are kept forever, and explains what the retention deletes and keeps', async () => {
      const { el } = await setup();

      expect(fieldLabelled(el(), LABEL).value).toBe('');
      expect(fieldLabelled(el(), LABEL).placeholder).toBe('Illimitée');
      const hint = text(el().querySelector('[data-field="logRetentionDays"]'));
      expect(hint).toContain("Le journal d'un job terminé depuis plus longtemps est supprimé");
      expect(hint).toContain('Les pipelines, les jobs et leurs statuts sont conservés');
    });

    it('shows the saved retention', async () => {
      const { el } = await setup(SETTINGS_URL, { ...SETTINGS, logRetentionDays: 30 });
      expect(fieldLabelled(el(), LABEL).value).toBe('30');
    });

    it('saves a whole number of days on blur', async () => {
      const { harness, el, http } = await setup();

      commit(fieldLabelled(el(), LABEL), '90');
      const saved = http.expectOne('/api/admin/settings');
      expect(saved.request.body).toEqual({ logRetentionDays: 90 });
      saved.flush({ ...SETTINGS, logRetentionDays: 90 });
      await settle(harness);

      expect(text(saveState(el(), 'logRetentionDays'))).toBe('Enregistré');
    });

    it('saves an emptied field as null: keep the logs without limit', async () => {
      const { el, http } = await setup(SETTINGS_URL, { ...SETTINGS, logRetentionDays: 30 });

      commit(fieldLabelled(el(), LABEL), '');

      expect(http.expectOne('/api/admin/settings').request.body).toEqual({ logRetentionDays: null });
    });

    it.each(['0', '-1', '2.5'])('explains that "%s" is not valid instead of saving it', async (value) => {
      const { harness, el, http } = await setup();

      commit(fieldLabelled(el(), LABEL), value);
      await settle(harness);

      expect(error(el())).toBe('Entrez un nombre entier de jours, 1 ou plus, ou laissez vide pour tout conserver');
      http.expectNone('/api/admin/settings');
    });

    it('puts the saved value back when the save is refused', async () => {
      const { harness, el, http, toastStub } = await setup(SETTINGS_URL, { ...SETTINGS, logRetentionDays: 30 });

      commit(fieldLabelled(el(), LABEL), '7');
      await settle(harness);
      http.expectOne('/api/admin/settings').flush({ message: 'boom' }, { status: 500, statusText: 'Server Error' });
      await settle(harness);

      expect(fieldLabelled(el(), LABEL).value).toBe('30');
      expect(text(saveState(el(), 'logRetentionDays'))).toBe('Non enregistré');
      expect(toastStub.show).toHaveBeenCalledWith("Échec de l'enregistrement. Réessayez.", 'error');
    });
  });

  describe('Kubernetes', () => {
    it('pre-fills the Kubernetes namespace and storage-class fields with the detected values when unset', async () => {
      const { el } = await setup(SETTINGS_URL, { ...SETTINGS, detectedK8sNamespace: 'ferrisgit', detectedK8sDefaultStorageClass: 'standard' });

      expect(fieldLabelled(el(), 'Namespace Kubernetes').value).toBe('ferrisgit');
      expect(fieldLabelled(el(), 'StorageClass pour le cache').value).toBe('standard');
      expect(el().textContent).toContain('Détecté automatiquement depuis le cluster : ferrisgit');
      expect(el().textContent).toContain('Détecté automatiquement depuis le cluster : standard');
      expect(el().querySelector('.admin-settings__hint--warning')?.textContent).toContain('Détecté automatiquement depuis le cluster : standard');
      expect(el().querySelector('[role="alert"]')).toBeNull();
    });

    it('prefers an explicitly configured namespace over a detected one, with no hint shown', async () => {
      const { el } = await setup(SETTINGS_URL, { ...SETTINGS, k8sNamespace: 'ci', detectedK8sNamespace: 'ferrisgit' });

      expect(fieldLabelled(el(), 'Namespace Kubernetes').value).toBe('ci');
      expect(el().textContent).not.toContain('Détecté automatiquement');
    });

    it("doesn't save a detected value the user only tabbed through", async () => {
      const { el, http } = await setup(SETTINGS_URL, { ...SETTINGS, detectedK8sNamespace: 'ferrisgit', detectedK8sDefaultStorageClass: 'standard' });

      fieldLabelled(el(), 'Namespace Kubernetes').dispatchEvent(new Event('blur'));
      fieldLabelled(el(), 'StorageClass pour le cache').dispatchEvent(new Event('blur'));

      http.expectNone('/api/admin/settings');
    });

    it('saves a namespace trimmed, and a cleared one as null', async () => {
      const { harness, el, http } = await setup(SETTINGS_URL, { ...SETTINGS, k8sNamespace: 'ci' });

      commit(fieldLabelled(el(), 'Namespace Kubernetes'), '  ferrisgit-ci  ');
      const first = http.expectOne('/api/admin/settings');
      expect(first.request.body).toEqual({ k8sNamespace: 'ferrisgit-ci' });
      first.flush({ ...SETTINGS, k8sNamespace: 'ferrisgit-ci' });
      await settle(harness);
      expect(text(saveState(el(), 'k8sNamespace'))).toBe('Enregistré');

      commit(fieldLabelled(el(), 'Namespace Kubernetes'), '   ');
      const second = http.expectOne('/api/admin/settings');
      expect(second.request.body).toEqual({ k8sNamespace: null });
      second.flush(SETTINGS);
    });

    it('puts the refused StorageClass back to the saved one when the save fails', async () => {
      const { harness, el, http } = await setup(SETTINGS_URL, { ...SETTINGS, k8sCacheStorageClass: 'standard-rwx' });

      commit(fieldLabelled(el(), 'StorageClass pour le cache'), 'nfs');
      await settle(harness);
      const refused = http.expectOne('/api/admin/settings');
      expect(refused.request.body).toEqual({ k8sCacheStorageClass: 'nfs' });
      expect(fieldLabelled(el(), 'StorageClass pour le cache').value).toBe('nfs');

      refused.flush({ message: 'boom' }, { status: 500, statusText: 'Server Error' });
      await settle(harness);

      expect(fieldLabelled(el(), 'StorageClass pour le cache').value).toBe('standard-rwx');
      expect(text(saveState(el(), 'k8sCacheStorageClass'))).toBe('Non enregistré');
    });

    describe('accepting a detected value', () => {
      const DETECTED: SystemSettings = { ...SETTINGS, detectedK8sNamespace: 'ferrisgit', detectedK8sDefaultStorageClass: 'standard' };
      function edit(input: HTMLInputElement, ...values: string[]) {
        for (const value of values) {
          input.value = value;
          input.dispatchEvent(new Event('input'));
        }
        input.dispatchEvent(new Event('blur'));
      }

      it('saves the detected StorageClass once the user has edited the field back to it: saving it is the confirmation', async () => {
        const { harness, el, http } = await setup(SETTINGS_URL, DETECTED);

        edit(fieldLabelled(el(), 'StorageClass pour le cache'), 'standar', 'standard');
        const saved = http.expectOne('/api/admin/settings');
        expect(saved.request.body).toEqual({ k8sCacheStorageClass: 'standard' });
        saved.flush({ ...DETECTED, k8sCacheStorageClass: 'standard' });
        await settle(harness);

        expect(text(saveState(el(), 'k8sCacheStorageClass'))).toBe('Enregistré');
        expect(el().textContent).not.toContain('Détecté automatiquement depuis le cluster : standard');
      });

      it('saves the detected namespace the same way', async () => {
        const { harness, el, http } = await setup(SETTINGS_URL, DETECTED);

        edit(fieldLabelled(el(), 'Namespace Kubernetes'), 'ferris', 'ferrisgit');
        const saved = http.expectOne('/api/admin/settings');
        expect(saved.request.body).toEqual({ k8sNamespace: 'ferrisgit' });
        saved.flush({ ...DETECTED, k8sNamespace: 'ferrisgit' });
        await settle(harness);
      });

      it('saves another StorageClass typed over the detected one', async () => {
        const { harness, el, http } = await setup(SETTINGS_URL, DETECTED);

        edit(fieldLabelled(el(), 'StorageClass pour le cache'), 'nfs-rwx');
        const saved = http.expectOne('/api/admin/settings');
        expect(saved.request.body).toEqual({ k8sCacheStorageClass: 'nfs-rwx' });
        saved.flush({ ...DETECTED, k8sCacheStorageClass: 'nfs-rwx' });
        await settle(harness);
      });

      it('saves a cleared StorageClass as null, then shows the detected one again', async () => {
        const { harness, el, http } = await setup(SETTINGS_URL, { ...DETECTED, k8sCacheStorageClass: 'standard-rwx' });

        edit(fieldLabelled(el(), 'StorageClass pour le cache'), '');
        await settle(harness);
        const saved = http.expectOne('/api/admin/settings');
        expect(saved.request.body).toEqual({ k8sCacheStorageClass: null });
        saved.flush(DETECTED);
        await settle(harness);

        expect(fieldLabelled(el(), 'StorageClass pour le cache').value).toBe('standard');
        expect(el().textContent).toContain('Détecté automatiquement depuis le cluster : standard');
      });

      it('puts a cleared StorageClass back when clearing it is refused', async () => {
        const { harness, el, http } = await setup(SETTINGS_URL, { ...DETECTED, k8sCacheStorageClass: 'standard-rwx' });

        edit(fieldLabelled(el(), 'StorageClass pour le cache'), '');
        await settle(harness);
        const refused = http.expectOne('/api/admin/settings');
        expect(fieldLabelled(el(), 'StorageClass pour le cache').value).toBe('');

        refused.flush({ message: 'boom' }, { status: 500, statusText: 'Server Error' });
        await settle(harness);

        expect(fieldLabelled(el(), 'StorageClass pour le cache').value).toBe('standard-rwx');
        expect(text(saveState(el(), 'k8sCacheStorageClass'))).toBe('Non enregistré');
      });

      it('saves nothing more on a later blur of the saved field left untouched', async () => {
        const { harness, el, http } = await setup(SETTINGS_URL, DETECTED);

        edit(fieldLabelled(el(), 'StorageClass pour le cache'), 'standar', 'standard');
        http.expectOne('/api/admin/settings').flush({ ...DETECTED, k8sCacheStorageClass: 'standard' });
        await settle(harness);

        fieldLabelled(el(), 'StorageClass pour le cache').dispatchEvent(new Event('blur'));
        http.expectNone('/api/admin/settings');
      });

      it('saves nothing more on a later untouched blur after a refused save either', async () => {
        const { harness, el, http } = await setup(SETTINGS_URL, DETECTED);

        edit(fieldLabelled(el(), 'Namespace Kubernetes'), 'ci');
        await settle(harness); // the request fails later, as a real one does
        http.expectOne('/api/admin/settings').flush({ message: 'boom' }, { status: 500, statusText: 'Server Error' });
        await settle(harness);
        expect(fieldLabelled(el(), 'Namespace Kubernetes').value).toBe('ferrisgit');

        fieldLabelled(el(), 'Namespace Kubernetes').dispatchEvent(new Event('blur'));
        http.expectNone('/api/admin/settings');
      });
    });

    it('says a job with cache keys fails without a StorageClass', async () => {
      const { el } = await setup();
      expect(text(el().querySelector('[data-field="k8sCacheStorageClass"]'))).toContain("Un job qui déclare des clés cache: échouera explicitement si aucune StorageClass n'est configurée ici.");
    });
  });

  describe('sessions (JWT)', () => {
    it('saves a settled value on blur, and nothing while the user types', async () => {
      const { harness, el, http } = await setup(SECURITY_URL);
      const input = fieldLabelled(el(), 'Durée de vie du JWT');

      for (const value of ['2', '24']) {
        input.value = value;
        input.dispatchEvent(new Event('input'));
      }
      http.expectNone('/api/admin/settings');

      input.dispatchEvent(new Event('blur'));
      const saved = http.expectOne('/api/admin/settings');
      expect(saved.request.method).toBe('PUT');
      expect(saved.request.body).toEqual({ jwtTtlHours: 24 });
      saved.flush({ ...SETTINGS, jwtTtlHours: 24 });
      await settle(harness);
      expect(text(saveState(el(), 'jwtTtlHours'))).toBe('Enregistré');
    });

    it("doesn't save when a blur left the value unchanged", async () => {
      const { el, http } = await setup(SECURITY_URL);
      fieldLabelled(el(), 'Durée de vie du JWT').dispatchEvent(new Event('blur'));
      http.expectNone('/api/admin/settings');
    });

    it('explains an invalid duration instead of ignoring it, and clears the message once valid', async () => {
      const { harness, el, http } = await setup(SECURITY_URL);
      const error = () => text(el().querySelector('[data-field="jwtTtlHours"] .gbt-input__error'));

      commit(fieldLabelled(el(), 'Durée de vie du JWT'), '0');
      await settle(harness);
      expect(error()).toBe("Entrez un nombre entier d'heures, 1 ou plus");
      commit(fieldLabelled(el(), 'Durée de vie du JWT'), 'huit');
      await settle(harness);
      expect(error()).toBe("Entrez un nombre entier d'heures, 1 ou plus");
      http.expectNone('/api/admin/settings');

      commit(fieldLabelled(el(), 'Durée de vie du JWT'), '8'); // back to the saved value: nothing to save, no error
      await settle(harness);
      expect(error()).toBeUndefined();
      http.expectNone('/api/admin/settings');
    });

    it('puts the refused duration back to the saved one when the save fails', async () => {
      const { harness, el, http, toastStub } = await setup(SECURITY_URL);

      commit(fieldLabelled(el(), 'Durée de vie du JWT'), '48');
      await settle(harness);
      const refused = http.expectOne('/api/admin/settings');
      expect(saveState(el(), 'jwtTtlHours')?.getAttribute('data-state')).toBe('saving');

      refused.flush({ message: 'boom' }, { status: 500, statusText: 'Server Error' });
      await settle(harness);

      expect(fieldLabelled(el(), 'Durée de vie du JWT').value).toBe('8');
      expect(text(saveState(el(), 'jwtTtlHours'))).toBe('Non enregistré');
      expect(toastStub.show).toHaveBeenCalledWith("Échec de l'enregistrement. Réessayez.", 'error');
    });

    it('keeps one live region per field through idle, saving and saved (it is announced, never re-created)', async () => {
      const { harness, el, http } = await setup(SECURITY_URL);
      const region = saveState(el(), 'jwtTtlHours')!;
      expect(region.getAttribute('role')).toBe('status');
      expect(region.getAttribute('data-state')).toBe('idle');
      expect(text(region)).toBe('');

      commit(fieldLabelled(el(), 'Durée de vie du JWT'), '48');
      await settle(harness);
      const request = http.expectOne('/api/admin/settings');
      expect(saveState(el(), 'jwtTtlHours')).toBe(region);
      expect(text(region)).toBe('Enregistrement…');

      request.flush({ ...SETTINGS, jwtTtlHours: 48 });
      await settle(harness);
      expect(saveState(el(), 'jwtTtlHours')).toBe(region);
      expect(text(region)).toBe('Enregistré');
      expect(el().querySelectorAll('[data-field="jwtTtlHours"] [role="status"]')).toHaveLength(1);
    });
  });

  describe('registration', () => {
    const toggle = (root: HTMLElement) => root.querySelector<HTMLInputElement>('[data-field="registrationEnabled"] input[role="switch"]')!;
    const HELP = 'Quand elle est active, toute personne qui peut joindre cette instance peut créer un compte. Chaque compte doit configurer la double authentification.';

    it('is a card "Inscription" with the switch, off by default, and its consequence spelt out', async () => {
      const { el } = await setup(SECURITY_URL);

      expect(text(toggle(el()).closest('label'))).toBe("Autoriser l'inscription libre");
      expect(toggle(el()).checked).toBe(false);
      expect(text(el().querySelector('[data-field="registrationEnabled"]'))).toContain(HELP);
      expect(saveState(el(), 'registrationEnabled')?.getAttribute('data-state')).toBe('idle');
    });

    it('shows the switch on when free registration is enabled', async () => {
      const { el } = await setup(SECURITY_URL, { ...SETTINGS, registrationEnabled: true });
      expect(toggle(el()).checked).toBe(true);
    });

    it('saves the switch as soon as it is toggled and marks it "Enregistré"', async () => {
      const { harness, el, http } = await setup(SECURITY_URL);

      toggle(el()).click();
      await settle(harness);
      const saved = http.expectOne('/api/admin/settings');
      expect(saved.request.method).toBe('PUT');
      expect(saved.request.body).toEqual({ registrationEnabled: true });
      expect(toggle(el()).checked).toBe(true);
      expect(text(saveState(el(), 'registrationEnabled'))).toBe('Enregistrement…');

      saved.flush({ ...SETTINGS, registrationEnabled: true });
      await settle(harness);
      expect(toggle(el()).checked).toBe(true);
      expect(text(saveState(el(), 'registrationEnabled'))).toBe('Enregistré');
      expect(saveState(el(), 'registrationEnabled')?.getAttribute('data-state')).toBe('saved');
    });

    it('switches it off again the same way', async () => {
      const { harness, el, http } = await setup(SECURITY_URL, { ...SETTINGS, registrationEnabled: true });

      toggle(el()).click();
      await settle(harness);
      const saved = http.expectOne('/api/admin/settings');
      expect(saved.request.body).toEqual({ registrationEnabled: false });
      saved.flush({ ...SETTINGS, registrationEnabled: false });
      await settle(harness);
      expect(toggle(el()).checked).toBe(false);
    });

    it('puts the switch back where it was and says "Non enregistré" when the save fails', async () => {
      const { harness, el, http, toastStub } = await setup(SECURITY_URL);

      toggle(el()).click();
      await settle(harness);
      const refused = http.expectOne('/api/admin/settings');
      expect(toggle(el()).checked).toBe(true);

      refused.flush({ message: 'boom' }, { status: 500, statusText: 'Server Error' });
      await settle(harness);

      expect(toggle(el()).checked).toBe(false);
      expect(text(saveState(el(), 'registrationEnabled'))).toBe('Non enregistré');
      expect(saveState(el(), 'registrationEnabled')?.getAttribute('data-state')).toBe('error');
      expect(toastStub.show).toHaveBeenCalledWith("Échec de l'enregistrement. Réessayez.", 'error');
    });

    it('puts an enabled registration back on when switching it off is refused', async () => {
      const { harness, el, http } = await setup(SECURITY_URL, { ...SETTINGS, registrationEnabled: true });

      toggle(el()).click();
      await settle(harness);
      http.expectOne('/api/admin/settings').flush({ message: 'boom' }, { status: 500, statusText: 'Server Error' });
      await settle(harness);

      expect(toggle(el()).checked).toBe(true);
    });

    it('can be toggled again after a refused save', async () => {
      const { harness, el, http } = await setup(SECURITY_URL);

      toggle(el()).click();
      await settle(harness);
      http.expectOne('/api/admin/settings').flush({ message: 'boom' }, { status: 500, statusText: 'Server Error' });
      await settle(harness);

      toggle(el()).click();
      await settle(harness);
      const retry = http.expectOne('/api/admin/settings');
      expect(retry.request.body).toEqual({ registrationEnabled: true });
      retry.flush({ ...SETTINGS, registrationEnabled: true });
      await settle(harness);
      expect(toggle(el()).checked).toBe(true);
    });
  });

  describe('public pages', () => {
    const toggle = (root: HTMLElement, field: string) => root.querySelector<HTMLInputElement>(`[data-field="${field}"] input[role="switch"]`)!;
    const PAGES_HELP =
      "Toute personne peut parcourir les dépôts publics sans compte : catalogue, fichiers, commits et releases. Cela inclut le clonage Git anonyme, wiki compris. Désactivé, ces dépôts ne sont plus lisibles qu'une fois connecté.";
    const SEO_HELP = 'Autorise les moteurs de recherche à indexer les pages publiques. Nécessite les pages publiques.';

    it('is a card "Pages publiques" with its two switches and what each one does', async () => {
      const { el } = await setup(SECURITY_URL);

      expect(text(toggle(el(), 'publicPagesEnabled').closest('label'))).toBe('Pages publiques');
      expect(text(toggle(el(), 'seoIndexingEnabled').closest('label'))).toBe('Référencement par les moteurs de recherche');
      expect(text(el().querySelector('[data-field="publicPagesEnabled"]'))).toContain(PAGES_HELP);
      expect(text(el().querySelector('[data-field="seoIndexingEnabled"]'))).toContain(SEO_HELP);
      expect(toggle(el(), 'publicPagesEnabled').checked).toBe(true);
      expect(toggle(el(), 'seoIndexingEnabled').checked).toBe(false);
    });

    it('saves the public pages switch on its own and marks it "Enregistré"', async () => {
      const { harness, el, http } = await setup(SECURITY_URL);

      toggle(el(), 'publicPagesEnabled').click();
      await settle(harness);
      const saved = http.expectOne('/api/admin/settings');
      expect(saved.request.method).toBe('PUT');
      expect(saved.request.body).toEqual({ publicPagesEnabled: false });
      saved.flush({ ...SETTINGS, publicPagesEnabled: false });
      await settle(harness);

      expect(toggle(el(), 'publicPagesEnabled').checked).toBe(false);
      expect(text(saveState(el(), 'publicPagesEnabled'))).toBe('Enregistré');
      expect(saveState(el(), 'seoIndexingEnabled')?.getAttribute('data-state')).toBe('idle');
    });

    it('saves the indexing switch on its own', async () => {
      const { harness, el, http } = await setup(SECURITY_URL);

      toggle(el(), 'seoIndexingEnabled').click();
      await settle(harness);
      const saved = http.expectOne('/api/admin/settings');
      expect(saved.request.body).toEqual({ seoIndexingEnabled: true });
      saved.flush({ ...SETTINGS, seoIndexingEnabled: true });
      await settle(harness);

      expect(toggle(el(), 'seoIndexingEnabled').checked).toBe(true);
      expect(text(saveState(el(), 'seoIndexingEnabled'))).toBe('Enregistré');
    });

    it('greys out the indexing switch while the public pages are off', async () => {
      const { el } = await setup(SECURITY_URL, { ...SETTINGS, publicPagesEnabled: false, seoIndexingEnabled: true });

      expect(toggle(el(), 'publicPagesEnabled').checked).toBe(false);
      expect(toggle(el(), 'seoIndexingEnabled').disabled).toBe(true);
    });

    it('puts the switch back and says "Non enregistré" when the save fails', async () => {
      const { harness, el, http, toastStub } = await setup(SECURITY_URL);

      toggle(el(), 'publicPagesEnabled').click();
      await settle(harness);
      http.expectOne('/api/admin/settings').flush({ message: 'boom' }, { status: 500, statusText: 'Server Error' });
      await settle(harness);

      expect(toggle(el(), 'publicPagesEnabled').checked).toBe(true);
      expect(text(saveState(el(), 'publicPagesEnabled'))).toBe('Non enregistré');
      expect(toastStub.show).toHaveBeenCalledWith("Échec de l'enregistrement. Réessayez.", 'error');
    });
  });

  describe('push size', () => {
    const sliderValue = (root: HTMLElement) => text(root.querySelector('[data-field="maxPushSizeMb"] .gbt-slider__value'));

    it('saves a new maximum and marks it "Enregistré"', async () => {
      const { harness, el, http } = await setup(SECURITY_URL);
      expect(sliderValue(el())).toBe('100 Mio');

      harness.routeDebugElement!.query(By.directive(SecuritySettings)).componentInstance.setMaxPushSizeMb(250);
      await settle(harness);
      const saved = http.expectOne('/api/admin/settings');
      expect(saved.request.body).toEqual({ maxPushSizeMb: 250 });
      saved.flush({ ...SETTINGS, maxPushSizeMb: 250 });
      await settle(harness);

      expect(sliderValue(el())).toBe('250 Mio');
      expect(text(saveState(el(), 'maxPushSizeMb'))).toBe('Enregistré');
    });

    it('puts the slider back on the saved maximum when the save fails', async () => {
      const { harness, el, http } = await setup(SECURITY_URL);

      harness.routeDebugElement!.query(By.directive(SecuritySettings)).componentInstance.setMaxPushSizeMb(250);
      await settle(harness);
      const refused = http.expectOne('/api/admin/settings');
      expect(sliderValue(el())).toBe('250 Mio');

      refused.flush({ message: 'boom' }, { status: 500, statusText: 'Server Error' });
      await settle(harness);

      expect(sliderValue(el())).toBe('100 Mio');
      expect(text(saveState(el(), 'maxPushSizeMb'))).toBe('Non enregistré');
    });
  });
});
