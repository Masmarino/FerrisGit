import { TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { provideHttpClient } from '@angular/common/http';
import { provideHttpClientTesting, HttpTestingController } from '@angular/common/http/testing';
import { provideRouter } from '@angular/router';
import { RouterTestingHarness } from '@angular/router/testing';
import { Icon } from '@masmarino/gabarit/icon';
import { GbtToastService } from '@masmarino/gabarit/toaster';
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

    it('shows the execution section by default: the engine, its own settings and the job log cards, each with its icon', async () => {
      const { harness, el } = await setup();

      const cards = harness.routeDebugElement!.queryAll(By.css('gbt-card'));
      expect(cards.map((card) => text(card.nativeElement.querySelector('h2')))).toEqual(["Moteur d'exécution", 'Runners Docker', 'Journaux des jobs']);
      expect(cards.map((card) => (card.query(By.directive(Icon))?.componentInstance as Icon | undefined)?.name())).toEqual(['server', 'key', 'clock']);
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
      expect(el.querySelectorAll('gbt-card')).toHaveLength(3);
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

  describe('execution', () => {
    const buttonNamed = (root: ParentNode, label: string) => Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === label);
    const cardTitles = (root: HTMLElement) => Array.from(root.querySelectorAll('gbt-card h2')).map((h) => text(h));
    const summary = (root: HTMLElement) => text(root.querySelector('.settings-save-bar__summary'));
    const saveButton = (root: HTMLElement) => buttonNamed(root, 'Enregistrer')!;

    function type(input: HTMLInputElement, value: string) {
      input.value = value;
      input.dispatchEvent(new Event('input'));
    }

    async function save(harness: RouterTestingHarness, root: HTMLElement) {
      saveButton(root).click();
      await settle(harness);
    }

    describe('engine', () => {
      it("shows only the settings of the engine picked: the runners' for Docker", async () => {
        const { el } = await setup();

        expect(checkedEngine(el())).toBe('Docker / runners');
        expect(cardTitles(el())).toEqual(["Moteur d'exécution", 'Runners Docker', 'Journaux des jobs']);
      });

      it("shows only the cluster's settings for Kubernetes", async () => {
        const { el } = await setup(SETTINGS_URL, { ...SETTINGS, executionEngine: 'kubernetes' });

        expect(checkedEngine(el())).toBe('Kubernetes');
        expect(cardTitles(el())).toEqual(["Moteur d'exécution", 'Kubernetes', 'Journaux des jobs']);
      });

      it('switches the cards at once, saves nothing, and says the engine changes once saved', async () => {
        const { harness, el, http } = await setup();

        engineOptions(el())[1].click();
        await settle(harness);

        http.expectNone('/api/admin/settings');
        expect(cardTitles(el())).toEqual(["Moteur d'exécution", 'Kubernetes', 'Journaux des jobs']);
        expect(text(el().querySelector('[data-field="executionEngine"]'))).toContain('Le moteur change une fois les réglages enregistrés.');
        expect(summary(el())).toBe('Modifications non enregistrées');
      });

      it('saves the engine with "Enregistrer" and confirms it with a toast', async () => {
        const { harness, el, http, toastStub } = await setup();
        engineOptions(el())[1].click();
        await settle(harness);

        await save(harness, el());
        const saved = http.expectOne('/api/admin/settings');
        expect(saved.request.method).toBe('PUT');
        expect(saved.request.body).toEqual({ executionEngine: 'kubernetes' });
        saved.flush({ ...SETTINGS, executionEngine: 'kubernetes' });
        await settle(harness);

        expect(toastStub.show).toHaveBeenCalledWith("Réglages d'exécution enregistrés");
        expect(summary(el())).toBe('');
        expect(saveButton(el()).disabled).toBe(true);
      });

      it('keeps the changes and says so with a toast when the save fails', async () => {
        const { harness, el, http, toastStub } = await setup();
        engineOptions(el())[1].click();
        await settle(harness);

        await save(harness, el());
        http.expectOne('/api/admin/settings').flush({ message: 'boom' }, { status: 500, statusText: 'Server Error' });
        await settle(harness);

        expect(toastStub.show).toHaveBeenCalledWith("Échec de l'enregistrement. Réessayez.", 'error');
        expect(checkedEngine(el())).toBe('Kubernetes');
        expect(summary(el())).toBe('Modifications non enregistrées');
      });

      it('says the values were refused when the server answers 400', async () => {
        const { harness, el, http, toastStub } = await setup();
        engineOptions(el())[1].click();
        await settle(harness);

        await save(harness, el());
        http.expectOne('/api/admin/settings').flush({ error: 'invalid' }, { status: 400, statusText: 'Bad Request' });
        await settle(harness);

        expect(toastStub.show).toHaveBeenCalledWith('Réglages refusés : vérifiez les champs.', 'error');
      });

      it('puts everything back as saved with "Annuler les modifications"', async () => {
        const { harness, el, http } = await setup(SETTINGS_URL, { ...SETTINGS, logRetentionDays: 30 });
        engineOptions(el())[1].click();
        type(fieldLabelled(el(), 'Durée de conservation des journaux (jours)'), '7');
        await settle(harness);

        buttonNamed(el(), 'Annuler les modifications')!.click();
        await settle(harness);

        http.expectNone('/api/admin/settings');
        expect(checkedEngine(el())).toBe('Docker / runners');
        expect(fieldLabelled(el(), 'Durée de conservation des journaux (jours)').value).toBe('30');
        expect(summary(el())).toBe('');
        expect(buttonNamed(el(), 'Annuler les modifications')).toBeUndefined();
      });

      it('offers no save while nothing has changed', async () => {
        const { el } = await setup();

        expect(summary(el())).toBe('');
        expect(saveButton(el()).disabled).toBe(true);
      });
    });

    it('saves everything changed in one request', async () => {
      const { harness, el, http } = await setup();

      type(fieldLabelled(el(), 'Jobs simultanés'), '3');
      type(fieldLabelled(el(), 'Durée de conservation des journaux (jours)'), '90');
      type(fieldLabelled(el(), 'Définir un jeton'), '  mon-jeton-secret  ');
      await settle(harness);
      await save(harness, el());

      const saved = http.expectOne('/api/admin/settings');
      expect(saved.request.body).toEqual({ maxConcurrentJobs: 3, logRetentionDays: 90, runnerRegistrationToken: 'mon-jeton-secret' });
      saved.flush({ ...SETTINGS, maxConcurrentJobs: 3, logRetentionDays: 90, runnerRegistrationTokenConfigured: true });
      await settle(harness);

      expect(fieldLabelled(el(), 'Remplacer le jeton').value).toBe('');
      expect(el().textContent).not.toContain('mon-jeton-secret');
      expect(summary(el())).toBe('');
    });

    it("sends only the shown engine's settings: Kubernetes edits stay out of a Docker save", async () => {
      const { harness, el, http } = await setup();
      engineOptions(el())[1].click();
      await settle(harness);
      type(fieldLabelled(el(), 'Namespace Kubernetes'), 'ci');
      engineOptions(el())[0].click();
      await settle(harness);
      type(fieldLabelled(el(), 'Jobs simultanés'), '2');
      await settle(harness);

      await save(harness, el());

      const saved = http.expectOne('/api/admin/settings');
      expect(saved.request.body).toEqual({ maxConcurrentJobs: 2 });
      saved.flush({ ...SETTINGS, maxConcurrentJobs: 2 });
    });

    describe('Docker runners', () => {
      const CONFIGURED: SystemSettings = { ...SETTINGS, runnerRegistrationTokenConfigured: true };
      const tokenField = (root: HTMLElement) => root.querySelector('[data-field="runnerRegistrationToken"]')!;
      const dialog = (root: HTMLElement) => root.querySelector('gbt-confirm-danger-modal');

      it('says that no token is configured, and offers no removal', async () => {
        const { el } = await setup();

        expect(text(tokenField(el()).querySelector('gbt-badge'))).toBe('Non configuré');
        expect(text(tokenField(el()).querySelector('gbt-input label'))).toBe('Définir un jeton');
        expect(buttonNamed(tokenField(el()), 'Supprimer le jeton')).toBeUndefined();
      });

      it('says that a token is configured without ever showing it: the field is masked and empty', async () => {
        const { el } = await setup(SETTINGS_URL, CONFIGURED);

        expect(text(tokenField(el()).querySelector('gbt-badge'))).toBe('Configuré');
        expect(fieldLabelled(el(), 'Remplacer le jeton').type).toBe('password');
        expect(fieldLabelled(el(), 'Remplacer le jeton').value).toBe('');
      });

      it('saves nothing for a token field left blank: removing the token has its own button', async () => {
        const { harness, el } = await setup(SETTINGS_URL, CONFIGURED);

        type(fieldLabelled(el(), 'Remplacer le jeton'), '   ');
        await settle(harness);

        expect(saveButton(el()).disabled).toBe(true);
      });

      it('generates a random 256-bit token at once when none exists, and shows it once', async () => {
        const { harness, el, http } = await setup();

        buttonNamed(tokenField(el()), 'Générer un jeton')!.click();
        await settle(harness);
        const saved = http.expectOne('/api/admin/settings');
        const token = (saved.request.body as { runnerRegistrationToken: string }).runnerRegistrationToken;
        expect(token).toMatch(/^[0-9a-f]{64}$/);
        saved.flush(CONFIGURED);
        await settle(harness);

        expect(text(tokenField(el()).querySelector('gbt-secret-reveal'))).toContain(token);
        buttonNamed(tokenField(el()), "J'ai copié le jeton")!.click();
        await settle(harness);
        expect(el().textContent).not.toContain(token);
      });

      it('does not show a generated token whose save was refused', async () => {
        const { harness, el, http } = await setup();

        buttonNamed(tokenField(el()), 'Générer un jeton')!.click();
        http.expectOne('/api/admin/settings').flush({ message: 'boom' }, { status: 500, statusText: 'Server Error' });
        await settle(harness);

        expect(tokenField(el()).querySelector('gbt-secret-reveal')).toBeNull();
      });

      it('asks before replacing an existing token, and saves nothing on cancel', async () => {
        const { harness, el, http } = await setup(SETTINGS_URL, CONFIGURED);

        buttonNamed(tokenField(el()), 'Générer un nouveau jeton')!.click();
        await settle(harness);
        expect(text(dialog(el()))).toContain("Remplacer le jeton d'enregistrement");

        buttonNamed(dialog(el())!, 'Annuler')!.click();
        await settle(harness);
        http.expectNone('/api/admin/settings');
      });

      it('removes the token after a confirmation that says its consequence', async () => {
        const { harness, el, http } = await setup(SETTINGS_URL, CONFIGURED);

        buttonNamed(tokenField(el()), 'Supprimer le jeton')!.click();
        await settle(harness);
        expect(text(dialog(el()))).toContain("ne pourront plus s'enregistrer eux-mêmes");
        buttonNamed(dialog(el())!, 'Supprimer le jeton')!.click();
        await settle(harness);
        const saved = http.expectOne('/api/admin/settings');
        expect(saved.request.body).toEqual({ runnerRegistrationToken: null });
        saved.flush(SETTINGS);
        await settle(harness);

        expect(text(tokenField(el()).querySelector('gbt-badge'))).toBe('Non configuré');
      });

      describe('simultaneous jobs', () => {
        const error = (root: HTMLElement) => text(root.querySelector('[data-field="maxConcurrentJobs"] .gbt-input__error'));

        it('is empty when unlimited, and shows the saved ceiling', async () => {
          const unlimited = await setup();
          expect(fieldLabelled(unlimited.el(), 'Jobs simultanés').placeholder).toBe('Illimité');
          TestBed.resetTestingModule();

          const { el } = await setup(SETTINGS_URL, { ...SETTINGS, maxConcurrentJobs: 4 });
          expect(fieldLabelled(el(), 'Jobs simultanés').value).toBe('4');
        });

        it('saves an emptied field as null: no limit', async () => {
          const { harness, el, http } = await setup(SETTINGS_URL, { ...SETTINGS, maxConcurrentJobs: 4 });

          type(fieldLabelled(el(), 'Jobs simultanés'), '  ');
          await settle(harness);
          await save(harness, el());

          expect(http.expectOne('/api/admin/settings').request.body).toEqual({ maxConcurrentJobs: null });
        });

        it('has nothing to save when the value is typed back to the saved one', async () => {
          const { harness, el } = await setup(SETTINGS_URL, { ...SETTINGS, maxConcurrentJobs: 4 });

          type(fieldLabelled(el(), 'Jobs simultanés'), '4');
          await settle(harness);

          expect(saveButton(el()).disabled).toBe(true);
        });

        // A number field can't hold letters, so only numbers that aren't whole counts reach the handler.
        it.each(['0', '-2', '1.5'])('explains on save that "%s" is not valid, and sends nothing', async (value) => {
          const { harness, el, http } = await setup();

          type(fieldLabelled(el(), 'Jobs simultanés'), value);
          await settle(harness);
          expect(error(el())).toBeUndefined();
          await save(harness, el());

          expect(error(el())).toBe('Entrez un nombre entier, 1 ou plus, ou laissez vide pour ne pas limiter');
          http.expectNone('/api/admin/settings');
        });

        it('clears the message once the value is valid', async () => {
          const { harness, el, http } = await setup();
          type(fieldLabelled(el(), 'Jobs simultanés'), '0');
          await settle(harness);
          await save(harness, el());

          type(fieldLabelled(el(), 'Jobs simultanés'), '8');
          await settle(harness);

          expect(error(el())).toBeUndefined();
          http.expectNone('/api/admin/settings');
        });
      });
    });

    describe('job log retention', () => {
      const LABEL = 'Durée de conservation des journaux (jours)';
      const error = (root: HTMLElement) => text(root.querySelector('[data-field="logRetentionDays"] .gbt-input__error'));

      it('is empty when the logs are kept forever, and explains what the retention deletes and keeps', async () => {
        const { el } = await setup();

        expect(fieldLabelled(el(), LABEL).placeholder).toBe('Illimitée');
        const hint = text(el().querySelector('[data-field="logRetentionDays"]'));
        expect(hint).toContain("Le journal d'un job terminé depuis plus longtemps est supprimé");
        expect(hint).toContain('Les pipelines, les jobs et leurs statuts sont conservés');
      });

      it('saves an emptied field as null: keep the logs without limit', async () => {
        const { harness, el, http } = await setup(SETTINGS_URL, { ...SETTINGS, logRetentionDays: 30 });

        type(fieldLabelled(el(), LABEL), '');
        await settle(harness);
        await save(harness, el());

        expect(http.expectOne('/api/admin/settings').request.body).toEqual({ logRetentionDays: null });
      });

      it.each(['0', '-1', '2.5'])('explains on save that "%s" is not valid', async (value) => {
        const { harness, el, http } = await setup();

        type(fieldLabelled(el(), LABEL), value);
        await settle(harness);
        await save(harness, el());

        expect(error(el())).toBe('Entrez un nombre entier de jours, 1 ou plus, ou laissez vide pour tout conserver');
        http.expectNone('/api/admin/settings');
      });
    });

    describe('Kubernetes', () => {
      const ON_K8S: SystemSettings = { ...SETTINGS, executionEngine: 'kubernetes' };
      const DETECTED: SystemSettings = { ...ON_K8S, detectedK8sNamespace: 'ferrisgit', detectedK8sDefaultStorageClass: 'standard' };

      it('pre-fills the namespace and storage-class fields with the detected values when unset', async () => {
        const { el } = await setup(SETTINGS_URL, DETECTED);

        expect(fieldLabelled(el(), 'Namespace Kubernetes').value).toBe('ferrisgit');
        expect(fieldLabelled(el(), 'StorageClass pour le cache').value).toBe('standard');
        expect(el().textContent).toContain('Détecté automatiquement depuis le cluster : ferrisgit');
        expect(el().querySelector('.admin-settings__hint--warning')?.textContent).toContain('Détecté automatiquement depuis le cluster : standard');
      });

      it('prefers an explicitly configured namespace over a detected one, with no hint shown', async () => {
        const { el } = await setup(SETTINGS_URL, { ...ON_K8S, k8sNamespace: 'ci', detectedK8sNamespace: 'ferrisgit' });

        expect(fieldLabelled(el(), 'Namespace Kubernetes').value).toBe('ci');
        expect(el().textContent).not.toContain('Détecté automatiquement');
      });

      it('has nothing to save for a detected value only shown', async () => {
        const { el } = await setup(SETTINGS_URL, DETECTED);

        expect(saveButton(el()).disabled).toBe(true);
      });

      it('saves a namespace trimmed, and a cleared one as null', async () => {
        const { harness, el, http } = await setup(SETTINGS_URL, { ...ON_K8S, k8sNamespace: 'ci' });

        type(fieldLabelled(el(), 'Namespace Kubernetes'), '  ferrisgit-ci  ');
        await settle(harness);
        await save(harness, el());
        const first = http.expectOne('/api/admin/settings');
        expect(first.request.body).toEqual({ k8sNamespace: 'ferrisgit-ci' });
        first.flush({ ...ON_K8S, k8sNamespace: 'ferrisgit-ci' });
        await settle(harness);

        type(fieldLabelled(el(), 'Namespace Kubernetes'), '   ');
        await settle(harness);
        await save(harness, el());
        const second = http.expectOne('/api/admin/settings');
        expect(second.request.body).toEqual({ k8sNamespace: null });
        second.flush(ON_K8S);
      });

      it('saves the detected StorageClass once the field was edited back to it: saving it is the confirmation', async () => {
        const { harness, el, http } = await setup(SETTINGS_URL, DETECTED);

        type(fieldLabelled(el(), 'StorageClass pour le cache'), 'standar');
        type(fieldLabelled(el(), 'StorageClass pour le cache'), 'standard');
        await settle(harness);
        await save(harness, el());
        const saved = http.expectOne('/api/admin/settings');
        expect(saved.request.body).toEqual({ k8sCacheStorageClass: 'standard' });
        saved.flush({ ...DETECTED, k8sCacheStorageClass: 'standard' });
        await settle(harness);

        expect(el().textContent).not.toContain('Détecté automatiquement depuis le cluster : standard');
      });

      it('saves a cleared StorageClass as null, then shows the detected one again', async () => {
        const { harness, el, http } = await setup(SETTINGS_URL, { ...DETECTED, k8sCacheStorageClass: 'standard-rwx' });

        type(fieldLabelled(el(), 'StorageClass pour le cache'), '');
        await settle(harness);
        await save(harness, el());
        const saved = http.expectOne('/api/admin/settings');
        expect(saved.request.body).toEqual({ k8sCacheStorageClass: null });
        saved.flush(DETECTED);
        await settle(harness);

        expect(fieldLabelled(el(), 'StorageClass pour le cache').value).toBe('standard');
      });

      it('says a job with cache keys fails without a StorageClass', async () => {
        const { el } = await setup(SETTINGS_URL, ON_K8S);
        expect(text(el().querySelector('[data-field="k8sCacheStorageClass"]'))).toContain("Un job qui déclare des clés cache: échouera explicitement si aucune StorageClass n'est configurée ici.");
      });
    });
  });

  describe('security', () => {
    const buttonNamed = (root: ParentNode, label: string) => Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === label);
    const summary = (root: HTMLElement) => text(root.querySelector('.settings-save-bar__summary'));
    const saveButton = (root: HTMLElement) => buttonNamed(root, 'Enregistrer')!;
    const toggle = (root: HTMLElement, field: string) => root.querySelector<HTMLInputElement>(`[data-field="${field}"] input[role="switch"]`)!;
    const jwtField = (root: HTMLElement) => fieldLabelled(root, 'Durée de vie du JWT');
    const jwtError = (root: HTMLElement) => text(root.querySelector('[data-field="jwtTtlHours"] .gbt-input__error'));
    const sliderValue = (root: HTMLElement) => text(root.querySelector('[data-field="maxPushSizeMb"] .gbt-slider__value'));

    function type(input: HTMLInputElement, value: string) {
      input.value = value;
      input.dispatchEvent(new Event('input'));
    }

    async function save(harness: RouterTestingHarness, root: HTMLElement) {
      saveButton(root).click();
      await settle(harness);
    }

    function security(harness: RouterTestingHarness) {
      return harness.routeDebugElement!.query(By.directive(SecuritySettings)).componentInstance as unknown as { maxPushSize: { set(mb: number): void } };
    }

    it('saves nothing while the admin types or toggles, and says there are unsaved changes', async () => {
      const { harness, el, http } = await setup(SECURITY_URL);

      type(jwtField(el()), '24');
      toggle(el(), 'registrationEnabled').click();
      await settle(harness);

      http.expectNone('/api/admin/settings');
      expect(summary(el())).toBe('Modifications non enregistrées');
      expect(saveButton(el()).disabled).toBe(false);
    });

    it('offers no save while nothing has changed', async () => {
      const { el } = await setup(SECURITY_URL);

      expect(summary(el())).toBe('');
      expect(saveButton(el()).disabled).toBe(true);
    });

    it('saves every change in one request with "Enregistrer", and confirms it with a toast', async () => {
      const { harness, el, http, toastStub } = await setup(SECURITY_URL);

      type(jwtField(el()), '24');
      toggle(el(), 'registrationEnabled').click();
      toggle(el(), 'seoIndexingEnabled').click();
      security(harness).maxPushSize.set(250);
      await settle(harness);
      await save(harness, el());

      const saved = http.expectOne('/api/admin/settings');
      expect(saved.request.method).toBe('PUT');
      expect(saved.request.body).toEqual({ jwtTtlHours: 24, maxPushSizeMb: 250, registrationEnabled: true, seoIndexingEnabled: true });
      saved.flush({ ...SETTINGS, jwtTtlHours: 24, maxPushSizeMb: 250, registrationEnabled: true, seoIndexingEnabled: true });
      await settle(harness);

      expect(toastStub.show).toHaveBeenCalledWith('Réglages de sécurité enregistrés');
      expect(summary(el())).toBe('');
      expect(toggle(el(), 'registrationEnabled').checked).toBe(true);
      expect(sliderValue(el())).toBe('250 Mio');
    });

    it('keeps the changes and says so with a toast when the save fails', async () => {
      const { harness, el, http, toastStub } = await setup(SECURITY_URL);
      toggle(el(), 'publicPagesEnabled').click();
      await settle(harness);

      await save(harness, el());
      http.expectOne('/api/admin/settings').flush({ message: 'boom' }, { status: 500, statusText: 'Server Error' });
      await settle(harness);

      expect(toastStub.show).toHaveBeenCalledWith("Échec de l'enregistrement. Réessayez.", 'error');
      expect(toggle(el(), 'publicPagesEnabled').checked).toBe(false);
      expect(summary(el())).toBe('Modifications non enregistrées');
    });

    it('says the values were refused when the server answers 400', async () => {
      const { harness, el, http, toastStub } = await setup(SECURITY_URL);
      type(jwtField(el()), '48');
      await settle(harness);

      await save(harness, el());
      http.expectOne('/api/admin/settings').flush({ error: 'invalid' }, { status: 400, statusText: 'Bad Request' });
      await settle(harness);

      expect(toastStub.show).toHaveBeenCalledWith('Réglages refusés : vérifiez les champs.', 'error');
    });

    it('puts everything back as saved with "Annuler les modifications"', async () => {
      const { harness, el, http } = await setup(SECURITY_URL);
      type(jwtField(el()), '48');
      toggle(el(), 'registrationEnabled').click();
      security(harness).maxPushSize.set(250);
      await settle(harness);

      buttonNamed(el(), 'Annuler les modifications')!.click();
      await settle(harness);

      http.expectNone('/api/admin/settings');
      expect(jwtField(el()).value).toBe('8');
      expect(toggle(el(), 'registrationEnabled').checked).toBe(false);
      expect(sliderValue(el())).toBe('100 Mio');
      expect(summary(el())).toBe('');
    });

    describe('sessions (JWT)', () => {
      it('explains an invalid duration on save, sends nothing, and clears the message once valid', async () => {
        const { harness, el, http } = await setup(SECURITY_URL);

        type(jwtField(el()), '0');
        await settle(harness);
        expect(jwtError(el())).toBeUndefined();
        await save(harness, el());
        expect(jwtError(el())).toBe("Entrez un nombre entier d'heures, 1 ou plus");
        http.expectNone('/api/admin/settings');

        type(jwtField(el()), '12');
        await settle(harness);
        expect(jwtError(el())).toBeUndefined();
      });

      it('has nothing to save when the duration is typed back to the saved one', async () => {
        const { harness, el } = await setup(SECURITY_URL);

        type(jwtField(el()), '9');
        type(jwtField(el()), '8');
        await settle(harness);

        expect(saveButton(el()).disabled).toBe(true);
      });
    });

    describe('registration', () => {
      const HELP =
        "Quand elle est active, toute personne qui peut joindre cette instance peut demander un compte : elle reçoit par e-mail un lien pour confirmer son adresse et choisir son mot de passe, puis configure la double authentification. Demande que l'envoi d'e-mails soit configuré, sinon la page d'inscription n'est pas proposée.";

      it('is a card "Inscription" with the switch, off by default, and its consequence spelt out', async () => {
        const { el } = await setup(SECURITY_URL);

        expect(text(toggle(el(), 'registrationEnabled').closest('label'))).toBe("Autoriser l'inscription libre");
        expect(toggle(el(), 'registrationEnabled').checked).toBe(false);
        expect(text(el().querySelector('[data-field="registrationEnabled"]'))).toContain(HELP);
      });

      it('switches it off again the same way', async () => {
        const { harness, el, http } = await setup(SECURITY_URL, { ...SETTINGS, registrationEnabled: true });

        toggle(el(), 'registrationEnabled').click();
        await settle(harness);
        await save(harness, el());

        const saved = http.expectOne('/api/admin/settings');
        expect(saved.request.body).toEqual({ registrationEnabled: false });
        saved.flush({ ...SETTINGS, registrationEnabled: false });
      });
    });

    describe('public pages', () => {
      const PAGES_HELP =
        "Toute personne peut parcourir les dépôts publics sans compte : catalogue, fichiers, commits et releases. Cela inclut le clonage Git anonyme, wiki compris. Désactivé, ces dépôts ne sont plus lisibles qu'une fois connecté.";
      const SEO_HELP = 'Autorise les moteurs de recherche à indexer les pages publiques. Nécessite les pages publiques.';

      it('is a card "Pages publiques" with its two switches and what each one does', async () => {
        const { el } = await setup(SECURITY_URL);

        expect(text(toggle(el(), 'publicPagesEnabled').closest('label'))).toBe('Pages publiques');
        expect(text(toggle(el(), 'seoIndexingEnabled').closest('label'))).toBe('Référencement par les moteurs de recherche');
        expect(text(el().querySelector('[data-field="publicPagesEnabled"]'))).toContain(PAGES_HELP);
        expect(text(el().querySelector('[data-field="seoIndexingEnabled"]'))).toContain(SEO_HELP);
      });

      it('greys out the indexing switch while the public pages are off, as soon as they are switched off', async () => {
        const { harness, el } = await setup(SECURITY_URL);

        toggle(el(), 'publicPagesEnabled').click();
        await settle(harness);

        expect(toggle(el(), 'seoIndexingEnabled').disabled).toBe(true);
      });
    });

    describe('push size', () => {
      it('shows the saved maximum', async () => {
        const { el } = await setup(SECURITY_URL);
        expect(sliderValue(el())).toBe('100 Mio');
      });
    });
  });
});
