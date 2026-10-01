import { TestBed } from '@angular/core/testing';
import { Subject, of, throwError } from 'rxjs';
import { CreateRepositoryModal } from './create-repository-modal';
import { RepositoriesService } from '../repositories.service';
import { GroupsService } from '../../groups/groups.service';

describe('CreateRepositoryModal', () => {
  const text = (el: Element | null | undefined) => (el?.textContent ?? '').replace(/\s+/g, ' ').trim();
  const dialog = () => document.querySelector<HTMLElement>('[role="dialog"]')!;

  function setup(options: { defaultLocation?: string } = {}) {
    const repositoriesStub = {
      create: vi.fn(() =>
        of({
          id: 'repo-1',
          name: 'demo',
          description: '',
          owner: 'alice',
          role: 'owner',
          visibility: 'private',
          createdAt: '2026-01-01T00:00:00Z',
          path: ['alice', 'demo'],
        }),
      ),
    };
    const groupsStub = {
      listWritable: vi.fn(() => of([{ id: 'group-1', path: 'acme' }])),
    };
    TestBed.configureTestingModule({
      providers: [
        { provide: RepositoriesService, useValue: repositoriesStub },
        { provide: GroupsService, useValue: groupsStub },
      ],
    });
    const fixture = TestBed.createComponent(CreateRepositoryModal);
    if (options.defaultLocation !== undefined) {
      fixture.componentRef.setInput('defaultLocation', options.defaultLocation);
    }
    fixture.detectChanges();
    return { fixture, component: fixture.componentInstance, repositoriesStub };
  }

  it('rejects submission with an empty (or whitespace-only) name and never calls create', () => {
    const { component, repositoriesStub } = setup();
    component.name.set('   ');
    component.submit();
    expect(component.error()).toBeTruthy();
    expect(repositoriesStub.create).not.toHaveBeenCalled();
  });

  it('falls back to 0 required approvals when the field holds a non-numeric value', () => {
    const { component, repositoriesStub } = setup();
    component.name.set('demo');
    component.requiredApprovals.set('not-a-number');
    component.submit();
    expect(repositoriesStub.create).toHaveBeenCalledWith('demo', 'private', expect.objectContaining({ requiredApprovals: 0 }));
  });

  it('sends the typed required-approvals count when it is numeric', () => {
    const { component, repositoriesStub } = setup();
    component.name.set('demo');
    component.requiredApprovals.set('2');
    component.submit();
    expect(repositoriesStub.create).toHaveBeenCalledWith('demo', 'private', expect.objectContaining({ requiredApprovals: 2 }));
  });

  it('falls back to the default pipeline file path when the field is left blank', () => {
    const { component, repositoriesStub } = setup();
    component.name.set('demo');
    component.pipelineFilePath.set('   ');
    component.submit();
    expect(repositoriesStub.create).toHaveBeenCalledWith('demo', 'private', expect.objectContaining({ pipelineFilePath: '.ferrisgit-ci.yml' }));
  });

  it('sends a custom pipeline file path when the user provides one', () => {
    const { component, repositoriesStub } = setup();
    component.name.set('demo');
    component.pipelineFilePath.set('ci/pipeline.yml');
    component.submit();
    expect(repositoriesStub.create).toHaveBeenCalledWith('demo', 'private', expect.objectContaining({ pipelineFilePath: 'ci/pipeline.yml' }));
  });

  it('creates in the personal namespace (no groupPath) when no location is selected', () => {
    const { component, repositoriesStub } = setup();
    component.name.set('demo');
    component.submit();
    expect(repositoriesStub.create).toHaveBeenCalledWith('demo', 'private', expect.objectContaining({ groupPath: undefined }));
  });

  it('creates under the selected group when a location is chosen', () => {
    const { component, repositoriesStub } = setup();
    component.name.set('demo');
    component.location.set('acme');
    component.submit();
    expect(repositoriesStub.create).toHaveBeenCalledWith('demo', 'private', expect.objectContaining({ groupPath: 'acme' }));
  });

  it('emits created and clears no error on a successful submission', () => {
    const { component } = setup();
    component.name.set('demo');
    let created = false;
    component.created.subscribe(() => (created = true));
    component.submit();
    expect(created).toBe(true);
  });

  it('shows an error and does not emit created when the create call fails', () => {
    const { component, repositoriesStub } = setup();
    repositoriesStub.create.mockReturnValueOnce(throwError(() => ({ status: 409 })));
    component.name.set('demo');
    let created = false;
    component.created.subscribe(() => (created = true));
    component.submit();
    expect(component.error()).toBe('Ce nom est déjà utilisé');
    expect(created).toBe(false);
  });

  it('reports an invalid name on a 400', () => {
    const { component, repositoriesStub } = setup();
    repositoriesStub.create.mockReturnValueOnce(throwError(() => ({ status: 400 })));
    component.name.set('demo');
    component.submit();
    expect(component.error()).toContain('Nom invalide');
  });

  it('does not claim the name is taken when the server fails for another reason', () => {
    const { component, repositoriesStub } = setup();
    repositoriesStub.create.mockReturnValueOnce(throwError(() => ({ status: 500 })));
    component.name.set('demo');
    component.submit();
    expect(component.error()).toBe('Impossible de créer le dépôt. Réessayez plus tard.');
  });

  describe('the restyled dialog', () => {
    it('puts the location and the name side by side as the repository path, with the visibility as two choices', () => {
      setup();
      expect(dialog().querySelector('.create-repository__location')).not.toBeNull();
      expect(dialog().querySelector('.create-repository__name input')).not.toBeNull();
      const choices = Array.from(dialog().querySelectorAll('.create-repository__visibility [role="radio"]'), (b) => text(b));
      expect(choices).toEqual(['Privé', 'Public']);
    });

    it('describes the name field by its rules, and names the visibility group by its visible label', () => {
      setup();
      const input = dialog().querySelector<HTMLInputElement>('.create-repository__name input')!;
      expect(text(dialog().querySelector(`#${input.getAttribute('aria-describedby')}`))).toBe('Lettres, chiffres, - et _ uniquement.');
      const group = dialog().querySelector('.create-repository__visibility [role="radiogroup"]')!;
      expect(group.hasAttribute('aria-label')).toBe(false);
      expect(text(dialog().querySelector(`#${group.getAttribute('aria-labelledby')}`))).toBe('Visibilité');
    });

    it('sends the visibility picked in the choice', () => {
      const { fixture, component, repositoriesStub } = setup();
      Array.from(dialog().querySelectorAll<HTMLButtonElement>('.create-repository__visibility [role="radio"]'))
        .find((b) => text(b) === 'Public')!
        .click();
      fixture.detectChanges();
      component.name.set('demo');
      component.submit();
      expect(repositoriesStub.create).toHaveBeenCalledWith('demo', 'public', expect.anything());
    });

    it('keeps the advanced options folded behind a disclosure button', () => {
      const { fixture } = setup();
      const toggle = Array.from(dialog().querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === 'Options avancées')!;
      expect(toggle.getAttribute('aria-expanded')).toBe('false');
      expect(dialog().querySelector('#create-repository-modal-advanced')).toBeNull();

      toggle.click();
      fixture.detectChanges();
      expect(toggle.getAttribute('aria-expanded')).toBe('true');
      expect(dialog().querySelector('#create-repository-modal-advanced')).not.toBeNull();
    });

    it('shows a failed creation in an error alert, the draft kept', async () => {
      const { fixture, component, repositoriesStub } = setup();
      repositoriesStub.create.mockReturnValueOnce(throwError(() => ({ status: 409 })));
      component.name.set('demo');
      component.submit();
      fixture.detectChanges();
      await fixture.whenStable();
      fixture.detectChanges();

      expect(text(dialog().querySelector('gbt-alert'))).toBe('Ce nom est déjà utilisé');
      expect(dialog().querySelector<HTMLInputElement>('.create-repository__name input')!.value).toBe('demo');
    });

    it('reports a missing name on the name field itself', () => {
      const { fixture, component } = setup();
      component.submit();
      fixture.detectChanges();
      expect(text(dialog().querySelector('.create-repository__name'))).toContain('Le nom est requis');
    });

    it('has one primary action, "Créer le dépôt"', () => {
      setup();
      expect(Array.from(dialog().querySelectorAll('.gbt-button--primary'), (b) => text(b))).toEqual(['Créer le dépôt']);
    });

    it('closes on "Annuler"', () => {
      const { component } = setup();
      let closed = 0;
      component.close.subscribe(() => closed++);
      Array.from(dialog().querySelectorAll<HTMLButtonElement>('button'))
        .find((b) => text(b) === 'Annuler')!
        .click();
      expect(closed).toBe(1);
    });

    it('submits from the "Créer le dépôt" button in the footer, outside the fields', () => {
      const { component, repositoriesStub } = setup();
      component.name.set('demo');
      const submit = Array.from(dialog().querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === 'Créer le dépôt')!;
      expect(submit.type).toBe('submit');
      expect(submit.closest('.gbt-modal__footer')).not.toBeNull();

      submit.click();

      expect(repositoriesStub.create).toHaveBeenCalledTimes(1);
    });

    it('while the request runs: says so, disables "Annuler" and ignores Escape, the backdrop and the close button', () => {
      const { fixture, component, repositoriesStub } = setup();
      repositoriesStub.create.mockReturnValue(new Subject<never>() as never);
      let closed = 0;
      component.close.subscribe(() => closed++);
      component.name.set('demo');

      component.submit();
      fixture.detectChanges();

      const cancel = () => Array.from(dialog().querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === 'Annuler')!;
      expect(text(dialog().querySelector('[role="status"]'))).toBe('Création en cours');
      expect(cancel().disabled).toBe(true);
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
      document.querySelector<HTMLElement>('.gbt-modal__backdrop')!.click();
      dialog().querySelector<HTMLButtonElement>('.gbt-modal__close')!.click();
      cancel().click();
      expect(closed).toBe(0);
    });
  });

  describe('default location', () => {
    it('starts on the given group when the caller can create repositories in it', () => {
      const { component, repositoriesStub } = setup({ defaultLocation: 'acme' });
      expect(component.location()).toBe('acme');
      component.name.set('demo');
      component.submit();
      expect(repositoriesStub.create).toHaveBeenCalledWith('demo', 'private', expect.objectContaining({ groupPath: 'acme' }));
    });

    it('stays personal when the given group is not one the caller can write to', () => {
      const { component } = setup({ defaultLocation: 'someone-else' });
      expect(component.location()).toBe('');
    });
  });
});
