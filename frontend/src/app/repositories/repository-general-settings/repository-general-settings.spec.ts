import { TestBed } from '@angular/core/testing';
import { Router } from '@angular/router';
import { of, Subject, throwError } from 'rxjs';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { RepositoryGeneralSettings } from './repository-general-settings';
import { Repository, RepositoriesService } from '../repositories.service';
import { repositoryFixture } from '../repository-fixtures';

const REPOSITORY = repositoryFixture({ description: 'Une plateforme Git.', path: ['florian', 'ferrisgit'] });

const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();

function setup() {
  const repositoriesStub = {
    getById: vi.fn(() => of(REPOSITORY)),
    update: vi.fn((_id: string, update: { description?: string; visibility?: 'private' | 'public'; name?: string; namespace?: string }) =>
      of<Repository>({ ...REPOSITORY, ...update, path: [update.namespace ?? 'florian', update.name ?? REPOSITORY.name] }),
    ),
  };
  const toastStub = { show: vi.fn() };
  const routerStub = { navigate: vi.fn(() => Promise.resolve(true)) };
  TestBed.configureTestingModule({
    providers: [
      { provide: RepositoriesService, useValue: repositoriesStub },
      { provide: GbtToastService, useValue: toastStub },
      { provide: Router, useValue: routerStub },
    ],
  });
  const fixture = TestBed.createComponent(RepositoryGeneralSettings);
  fixture.componentRef.setInput('repositoryId', 'repo-1');
  const el = fixture.nativeElement as HTMLElement;
  const component = fixture.componentInstance as unknown as {
    updateDescription(value: string): void;
    askVisibility(visibility: 'private' | 'public'): void;
    confirmVisibility(): void;
    cancelVisibility(): void;
    visibilityShown(): string;
    repository(): Repository | null;
    newName: { set(value: string): void };
    newNamespace: { set(value: string): void };
    moveChanged(): boolean;
    nameProblem(): string | null;
    askMove(): void;
    confirmingMove(): boolean;
    confirmMove(): void;
  };
  return { fixture, el, component, repositoriesStub, toastStub, routerStub };
}

describe('RepositoryGeneralSettings', () => {
  it('loads the repository by id and shows its information in one card', async () => {
    const { fixture, el, repositoriesStub } = setup();
    fixture.detectChanges();
    await fixture.whenStable(); // ngModel writes the value to the field asynchronously
    fixture.detectChanges();

    expect(repositoriesStub.getById).toHaveBeenCalledWith('repo-1');
    const cards = Array.from(el.querySelectorAll('gbt-card'));
    expect(cards.map((card) => text(card.querySelector('h2')))).toEqual(['Informations', 'Nom et emplacement']);
    expect(text(el.querySelector('gbt-textarea label'))).toBe('Description');
    expect(el.querySelector<HTMLTextAreaElement>('gbt-textarea textarea')?.value).toBe('Une plateforme Git.');
    expect(text(el.querySelector('[data-field="visibility"] [role="radio"][aria-checked="true"]'))).toBe('Privé');
  });

  describe('name and location', () => {
    it('starts on the current name and namespace, and moves nothing until one of them changes', () => {
      const { fixture, component } = setup();
      fixture.detectChanges();

      expect(component.moveChanged()).toBe(false);
      component.newName.set('ferrisgit');
      expect(component.moveChanged()).toBe(false);
      component.newNamespace.set('acme/');
      expect(component.moveChanged()).toBe(true);
    });

    it('refuses a name the server would refuse', () => {
      const { fixture, component } = setup();
      fixture.detectChanges();

      component.newName.set('not valid');
      component.askMove();

      expect(component.nameProblem()).toBe('Lettres, chiffres, - et _ uniquement.');
      expect(component.confirmingMove()).toBe(false);
    });

    it('asks first, sends only what changed, then follows the repository to its new settings page', () => {
      const { fixture, component, repositoriesStub, routerStub, toastStub } = setup();
      fixture.detectChanges();

      component.newName.set('forge');
      component.askMove();
      expect(component.confirmingMove()).toBe(true);
      component.confirmMove();

      expect(repositoriesStub.update).toHaveBeenCalledWith('repo-1', { name: 'forge' });
      expect(routerStub.navigate).toHaveBeenCalledWith(['/repositories', 'florian', 'forge', '-', 'settings']);
      expect(toastStub.show).toHaveBeenCalledWith("Le dépôt a changé d'adresse.");
    });

    it('says when the new path is taken or not allowed', () => {
      const { fixture, component, repositoriesStub, toastStub } = setup();
      fixture.detectChanges();
      component.newNamespace.set('someone');

      repositoriesStub.update.mockReturnValueOnce(throwError(() => ({ status: 409 })));
      component.confirmMove();
      expect(toastStub.show).toHaveBeenLastCalledWith('Un dépôt porte déjà ce nom à cet endroit.', 'error');

      repositoriesStub.update.mockReturnValueOnce(throwError(() => ({ status: 400, error: { error: 'a repository can only be transferred to your own namespace or to a group you maintain' } })));
      component.confirmMove();
      expect(toastStub.show).toHaveBeenLastCalledWith('Vous ne pouvez transférer un dépôt que vers votre propre espace ou un groupe dont vous êtes Mainteneur.', 'error');
    });
  });

  it('shows skeletons while loading, then the form', () => {
    const { fixture, el, repositoriesStub } = setup();
    const pending = new Subject<Repository>();
    repositoriesStub.getById.mockReturnValue(pending as never);
    fixture.detectChanges();

    expect(el.querySelector('[aria-busy="true"]')).not.toBeNull();
    expect(el.querySelector('gbt-textarea')).toBeNull();

    pending.next(REPOSITORY);
    fixture.detectChanges();
    expect(el.querySelector('[aria-busy="true"]')).toBeNull();
    expect(el.querySelector('gbt-textarea')).not.toBeNull();
  });

  it('shows an alert with a retry when the repository cannot be loaded', () => {
    const { fixture, el, repositoriesStub, toastStub } = setup();
    repositoriesStub.getById.mockReturnValue(throwError(() => ({ status: 500 })));
    fixture.detectChanges();

    expect(toastStub.show).toHaveBeenCalledWith('Impossible de charger les informations du dépôt. Réessayez plus tard.', 'error');
    expect(text(el.querySelector('gbt-alert'))).toContain("n'ont pas pu être chargées");

    repositoriesStub.getById.mockReturnValue(of(REPOSITORY));
    el.querySelector<HTMLButtonElement>('gbt-alert button')!.click();
    fixture.detectChanges();
    expect(el.querySelector('gbt-textarea')).not.toBeNull();
  });

  describe('description', () => {
    it('is saved on commit, trimmed, and the updated repository replaces the displayed one', () => {
      const { fixture, component, repositoriesStub } = setup();
      fixture.detectChanges();

      component.updateDescription('  Nouvelle description  ');

      expect(repositoriesStub.update).toHaveBeenCalledWith('repo-1', { description: 'Nouvelle description' });
      expect(component.repository()?.description).toBe('Nouvelle description');
    });

    it('can be emptied', () => {
      const { fixture, component, repositoriesStub } = setup();
      fixture.detectChanges();

      component.updateDescription('   ');

      expect(repositoriesStub.update).toHaveBeenCalledWith('repo-1', { description: '' });
    });

    it('is not saved when it did not change', () => {
      const { fixture, component, repositoriesStub } = setup();
      fixture.detectChanges();

      component.updateDescription('Une plateforme Git.');

      expect(repositoriesStub.update).not.toHaveBeenCalled();
    });

    it('marks the field "Enregistré", or "Non enregistré" with a toast when the save fails', () => {
      const { fixture, el, component, repositoriesStub, toastStub } = setup();
      fixture.detectChanges();
      const state = () => text(el.querySelector('[data-field="description"] [role="status"]'));

      component.updateDescription('Une autre');
      fixture.detectChanges();
      expect(state()).toBe('Enregistré');

      repositoriesStub.update.mockReturnValue(throwError(() => ({ status: 500 })));
      component.updateDescription('Encore une');
      fixture.detectChanges();
      expect(state()).toBe('Non enregistré');
      expect(toastStub.show).toHaveBeenCalledWith('Impossible de mettre à jour la description.', 'error');
    });
  });

  describe('visibility', () => {
    it('asks for confirmation before making a private repository public, and saves nothing yet', () => {
      const { fixture, el, component, repositoriesStub } = setup();
      fixture.detectChanges();

      component.askVisibility('public');
      fixture.detectChanges();

      expect(repositoriesStub.update).not.toHaveBeenCalled();
      const modal = el.querySelector('gbt-confirm-danger-modal');
      expect(modal).not.toBeNull();
      expect(text(modal)).toContain('Rendre ce dépôt public');
      expect(text(modal)).toContain('catalogue public');
    });

    it('asks for confirmation before making a public repository private, and says what disappears', () => {
      const { fixture, el, component, repositoriesStub } = setup();
      repositoriesStub.getById.mockReturnValue(of({ ...REPOSITORY, visibility: 'public' }));
      fixture.detectChanges();

      component.askVisibility('private');
      fixture.detectChanges();

      const modal = el.querySelector('gbt-confirm-danger-modal');
      expect(text(modal)).toContain('Rendre ce dépôt privé');
      expect(text(modal)).toContain('disparaîtra du catalogue public');
      expect(repositoriesStub.update).not.toHaveBeenCalled();
    });

    it('saves the new visibility once confirmed and closes the dialog', () => {
      const { fixture, el, component, repositoriesStub } = setup();
      fixture.detectChanges();
      component.askVisibility('public');
      fixture.detectChanges();

      component.confirmVisibility();
      fixture.detectChanges();

      expect(repositoriesStub.update).toHaveBeenCalledWith('repo-1', { visibility: 'public' });
      expect(component.repository()?.visibility).toBe('public');
      expect(component.visibilityShown()).toBe('public');
      expect(el.querySelector('gbt-confirm-danger-modal')).toBeNull();
    });

    it('goes back to the saved value, without saving, when the dialog is cancelled', () => {
      const { fixture, el, component, repositoriesStub } = setup();
      fixture.detectChanges();
      component.askVisibility('public');
      fixture.detectChanges();
      expect(component.visibilityShown()).toBe('public');

      component.cancelVisibility();
      fixture.detectChanges();

      expect(repositoriesStub.update).not.toHaveBeenCalled();
      expect(component.visibilityShown()).toBe('private');
      expect(el.querySelector('gbt-confirm-danger-modal')).toBeNull();
    });

    it('does not ask when the value is the current one', () => {
      const { fixture, el, component } = setup();
      fixture.detectChanges();

      component.askVisibility('private');
      fixture.detectChanges();

      expect(el.querySelector('gbt-confirm-danger-modal')).toBeNull();
    });

    it('goes back to the saved value and says so when the save fails', () => {
      const { fixture, el, component, repositoriesStub, toastStub } = setup();
      fixture.detectChanges();
      component.askVisibility('public');
      repositoriesStub.update.mockReturnValue(throwError(() => ({ status: 403 })));

      component.confirmVisibility();
      fixture.detectChanges();

      expect(component.visibilityShown()).toBe('private');
      expect(component.repository()?.visibility).toBe('private');
      expect(toastStub.show).toHaveBeenCalledWith('Impossible de changer la visibilité du dépôt.', 'error');
      expect(text(el.querySelector('[data-field="visibility"] [role="status"]'))).toBe('Non enregistré');
    });
  });
});
