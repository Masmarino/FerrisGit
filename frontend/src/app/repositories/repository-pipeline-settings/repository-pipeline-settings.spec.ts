import { TestBed } from '@angular/core/testing';
import { of, Subject, throwError } from 'rxjs';
import { RepositoryPipelineSettings } from './repository-pipeline-settings';
import { RepositorySettings as RepositorySettingsModel, RepositorySettingsService } from '../repository-settings.service';
import { GbtToastService } from '@masmarino/gabarit';

describe('RepositoryPipelineSettings', () => {
  function setup() {
    const repositorySettingsStub = {
      get: vi.fn(() => of<RepositorySettingsModel>({ pipelineFilePath: '.ferrisgit-ci.yml', ciEnabled: true, requiredApprovals: 0 })),
      update: vi.fn(() => of<RepositorySettingsModel>({ pipelineFilePath: '.ferrisgit-ci.yml', ciEnabled: false, requiredApprovals: 1 })),
    };
    const toastStub = { show: vi.fn() };
    TestBed.configureTestingModule({
      providers: [
        { provide: RepositorySettingsService, useValue: repositorySettingsStub },
        { provide: GbtToastService, useValue: toastStub },
      ],
    });
    const fixture = TestBed.createComponent(RepositoryPipelineSettings);
    fixture.componentRef.setInput('repositoryId', 'repo-1');
    return { fixture, component: fixture.componentInstance, repositorySettingsStub, toastStub };
  }

  it('loads and displays the pipeline settings for the given repository', () => {
    const { component, fixture, repositorySettingsStub } = setup();
    fixture.detectChanges();

    expect(repositorySettingsStub.get).toHaveBeenCalledWith('repo-1');
    // The value is bound via [ngModel] into gbt-input's own <input>, so assert on the loaded signal, not the DOM text.
    expect(component['settings']()?.pipelineFilePath).toBe('.ferrisgit-ci.yml');
  });

  it('surfaces an error when loading the pipeline settings fails', () => {
    const { fixture, repositorySettingsStub, toastStub } = setup();
    repositorySettingsStub.get.mockReturnValue(throwError(() => ({ status: 500 })));
    fixture.detectChanges();

    expect(toastStub.show).toHaveBeenCalledWith('Impossible de charger les réglages. Réessayez plus tard.', 'error');
  });

  it('toggles CI enabled and applies the update it gets back', () => {
    const { component, fixture, repositorySettingsStub } = setup();
    fixture.detectChanges();

    component.togglePipelineEnabled();

    expect(repositorySettingsStub.update).toHaveBeenCalledWith('repo-1', { ciEnabled: false });
    expect(component['settings']()?.ciEnabled).toBe(false);
  });

  it('does nothing when toggling CI enabled before settings have loaded', () => {
    const { component, repositorySettingsStub } = setup();

    component.togglePipelineEnabled();

    expect(repositorySettingsStub.update).not.toHaveBeenCalled();
  });

  it('updates the pipeline file path on commit, trimmed', () => {
    const { component, fixture, repositorySettingsStub } = setup();
    fixture.detectChanges();

    component.updatePipelineFilePath('  .gitlab-ci.yml  ');

    expect(repositorySettingsStub.update).toHaveBeenCalledWith('repo-1', { pipelineFilePath: '.gitlab-ci.yml' });
  });

  it('does not update the pipeline file path when unchanged or blank', () => {
    const { component, fixture, repositorySettingsStub } = setup();
    fixture.detectChanges();

    component.updatePipelineFilePath('.ferrisgit-ci.yml');
    component.updatePipelineFilePath('   ');

    expect(repositorySettingsStub.update).not.toHaveBeenCalled();
  });

  it('updates the required approvals count', () => {
    const { component, fixture, repositorySettingsStub } = setup();
    fixture.detectChanges();

    component.updateRequiredApprovals('2');

    expect(repositorySettingsStub.update).toHaveBeenCalledWith('repo-1', { requiredApprovals: 2 });
  });

  it('does not update required approvals for an invalid, negative or unchanged value', () => {
    const { component, fixture, repositorySettingsStub } = setup();
    fixture.detectChanges();

    component.updateRequiredApprovals('not-a-number');
    component.updateRequiredApprovals('-1');
    component.updateRequiredApprovals('0');

    expect(repositorySettingsStub.update).not.toHaveBeenCalled();
  });

  describe('layout', () => {
    const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();
    const button = (root: HTMLElement, label: string) =>
      Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find((b) => b.textContent?.trim() === label);

    it('shows skeleton cards while the settings load, then the forms', () => {
      const { fixture, repositorySettingsStub } = setup();
      const pending = new Subject<RepositorySettingsModel>();
      repositorySettingsStub.get.mockReturnValue(pending);
      fixture.detectChanges();
      const el = fixture.nativeElement as HTMLElement;

      expect(el.querySelector('[aria-busy="true"]')).not.toBeNull();
      expect(el.querySelector('gbt-switch')).toBeNull();
      const skeletons = Array.from(el.querySelectorAll('gbt-card[aria-hidden="true"]'));
      expect(skeletons.length).toBe(2);
      for (const skeleton of skeletons) {
        const header = skeleton.querySelector(':scope > .gbt-card__header');
        const box = skeleton.querySelector(':scope > .gbt-card');
        expect(Array.from(skeleton.children)).toEqual([header, box]);
        expect(box?.getAttribute('data-variant')).toBe('outlined');
        expect(header?.querySelector('gbt-skeleton')).toBeTruthy();
      }

      pending.next({ pipelineFilePath: '.ferrisgit-ci.yml', ciEnabled: true, requiredApprovals: 0 });
      fixture.detectChanges();
      expect(el.querySelector('[aria-busy="true"]')).toBeNull();
      expect(el.querySelector('gbt-switch')).not.toBeNull();
    });

    it('groups the settings in two cards with a heading and a one-line help: CI (switch, file path) and merge requests (approvals)', () => {
      const { fixture } = setup();
      fixture.detectChanges();
      const el = fixture.nativeElement as HTMLElement;

      const cards = Array.from(el.querySelectorAll('gbt-card'));
      expect(cards.map((card) => text(card.querySelector('h2')))).toEqual(['Intégration continue', 'Demandes de fusion']);
      expect(cards.every((card) => !!text(card.querySelector('.gbt-card__description')))).toBe(true);

      const [ci, mergeRequests] = cards;
      expect(ci.querySelector('gbt-switch')).not.toBeNull();
      expect(text(ci.querySelector('gbt-input label'))).toBe('Chemin du fichier pipeline');
      expect(text(mergeRequests.querySelector('gbt-input label'))).toBe('Approbations requises avant fusion');
    });

    it('says what the CI switch does in both states', () => {
      const { fixture, repositorySettingsStub } = setup();
      fixture.detectChanges();
      const el = fixture.nativeElement as HTMLElement;
      expect(text(el.querySelector('.repository-pipeline-settings__switch-hint'))).toContain('à chaque push');

      repositorySettingsStub.update.mockReturnValue(of({ pipelineFilePath: '.ferrisgit-ci.yml', ciEnabled: false, requiredApprovals: 0 }));
      fixture.componentInstance.togglePipelineEnabled();
      fixture.detectChanges();
      expect(text(el.querySelector('.repository-pipeline-settings__switch-hint'))).toContain('Aucun pipeline');
    });

    it('marks a field "Enregistré" once it is saved on blur, and "Non enregistré" when the save fails', () => {
      const { component, fixture, repositorySettingsStub, toastStub } = setup();
      fixture.detectChanges();
      const el = fixture.nativeElement as HTMLElement;
      const state = (field: string) => el.querySelector(`[data-field="${field}"] [role="status"]`);

      expect(text(state('pipelineFilePath'))).toBe('');
      repositorySettingsStub.update.mockReturnValue(of({ pipelineFilePath: '.gitlab-ci.yml', ciEnabled: true, requiredApprovals: 0 }));
      component.updatePipelineFilePath('.gitlab-ci.yml');
      fixture.detectChanges();
      expect(text(state('pipelineFilePath'))).toBe('Enregistré');
      expect(state('pipelineFilePath')?.getAttribute('data-state')).toBe('saved');

      repositorySettingsStub.update.mockReturnValue(throwError(() => ({ status: 500 })));
      component.updateRequiredApprovals('3');
      fixture.detectChanges();
      expect(text(state('requiredApprovals'))).toBe('Non enregistré');
      expect(state('requiredApprovals')?.getAttribute('data-state')).toBe('error');
      expect(toastStub.show).toHaveBeenCalledWith("Impossible de mettre à jour le nombre d'approbations requises.", 'error');
      expect(text(state('pipelineFilePath'))).toBe('Enregistré');

      component.togglePipelineEnabled();
      fixture.detectChanges();
      expect(text(state('ciEnabled'))).toBe('Non enregistré');
    });

    describe('the CI switch', () => {
      const switchInput = (el: HTMLElement) => el.querySelector<HTMLInputElement>('gbt-switch input[role="switch"]')!;
      async function settle(fixture: { detectChanges(): void; whenStable(): Promise<unknown> }) {
        fixture.detectChanges();
        await fixture.whenStable();
        fixture.detectChanges();
      }

      it('sends the state it shows when clicked, and keeps it once saved', async () => {
        const { fixture, repositorySettingsStub } = setup();
        const el = fixture.nativeElement as HTMLElement;
        await settle(fixture);
        expect(switchInput(el).checked).toBe(true);

        switchInput(el).click();
        await settle(fixture);

        expect(repositorySettingsStub.update).toHaveBeenCalledExactlyOnceWith('repo-1', { ciEnabled: false });
        expect(switchInput(el).checked).toBe(false);
      });

      it('shows the previous state again when the save fails, and the next click sends what it then shows', async () => {
        const { fixture, repositorySettingsStub, toastStub } = setup();
        const el = fixture.nativeElement as HTMLElement;
        await settle(fixture);
        const refused = new Subject<RepositorySettingsModel>();
        repositorySettingsStub.update.mockReturnValue(refused);

        switchInput(el).click();
        await settle(fixture);
        expect(repositorySettingsStub.update).toHaveBeenLastCalledWith('repo-1', { ciEnabled: false });
        expect(switchInput(el).checked).toBe(false);

        refused.error({ status: 500 });
        await settle(fixture);

        expect(toastStub.show).toHaveBeenCalledWith('Impossible de mettre à jour le pipeline.', 'error');
        expect(switchInput(el).checked).toBe(true);
        expect(text(el.querySelector('.repository-pipeline-settings__switch-hint'))).toContain('à chaque push');

        repositorySettingsStub.update.mockReturnValue(of({ pipelineFilePath: '.ferrisgit-ci.yml', ciEnabled: false, requiredApprovals: 0 }));
        switchInput(el).click();
        await settle(fixture);
        expect(repositorySettingsStub.update).toHaveBeenCalledTimes(2);
        expect(repositorySettingsStub.update).toHaveBeenLastCalledWith('repo-1', { ciEnabled: false });
        expect(switchInput(el).checked).toBe(false);
      });
    });

    it('explains an invalid approvals count or a blank path instead of ignoring it silently, and clears it once valid', () => {
      const { component, fixture, repositorySettingsStub } = setup();
      fixture.detectChanges();
      const el = fixture.nativeElement as HTMLElement;
      const error = (field: string) => text(el.querySelector(`[data-field="${field}"] .gbt-input__error`));

      component.updateRequiredApprovals('deux');
      component.updatePipelineFilePath('   ');
      fixture.detectChanges();
      expect(error('requiredApprovals')).toBe('Entrez un nombre entier, 0 ou plus');
      expect(error('pipelineFilePath')).toBe('Indiquez le chemin du fichier pipeline');
      expect(repositorySettingsStub.update).not.toHaveBeenCalled();

      component.updateRequiredApprovals('2');
      component.updatePipelineFilePath('.ferrisgit-ci.yml');
      fixture.detectChanges();
      expect(error('requiredApprovals')).toBeUndefined();
      expect(error('pipelineFilePath')).toBeUndefined();
    });

    it('shows a failed state with a retry button when loading fails; retrying loads again', () => {
      const { fixture, repositorySettingsStub } = setup();
      repositorySettingsStub.get.mockReturnValue(throwError(() => ({ status: 500 })));
      fixture.detectChanges();
      const el = fixture.nativeElement as HTMLElement;

      const failed = el.querySelector('gbt-alert .gbt-alert');
      expect(text(failed)).toContain("Les réglages du pipeline n'ont pas pu être chargés");
      expect(failed?.getAttribute('data-variant')).toBe('error');
      // The error toast announces the failure, so the inline block stays silent (one live region, not two).
      expect(failed?.getAttribute('role')).toBeNull();
      expect(failed?.getAttribute('aria-live')).toBeNull();
      expect(failed?.querySelector('button')?.textContent?.trim()).toBe('Réessayer');
      expect(el.querySelectorAll('[role="alert"], [aria-live]')).toHaveLength(0);
      expect(el.querySelector('gbt-switch')).toBeNull();

      repositorySettingsStub.get.mockReturnValue(of({ pipelineFilePath: '.ferrisgit-ci.yml', ciEnabled: true, requiredApprovals: 0 }));
      button(el, 'Réessayer')!.click();
      fixture.detectChanges();

      expect(repositorySettingsStub.get).toHaveBeenCalledTimes(2);
      expect(el.querySelector('gbt-alert .gbt-alert[data-variant="error"]')).toBeNull();
      expect(el.querySelector('gbt-switch')).not.toBeNull();
    });
  });
});
