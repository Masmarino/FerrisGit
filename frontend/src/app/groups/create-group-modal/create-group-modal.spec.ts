import { TestBed } from '@angular/core/testing';
import { Subject, of, throwError } from 'rxjs';
import { GbtToastService } from '@masmarino/gabarit/toaster';
import { CreateGroupModal } from './create-group-modal';
import { Group, GroupsService } from '../groups.service';

const GROUP: Group = { id: 'g1', parentGroupId: null, name: 'acme', description: '', createdAt: '2026-01-01T00:00:00Z' };

describe('CreateGroupModal', () => {
  const text = (el: Element | null | undefined) => (el?.textContent ?? '').replace(/\s+/g, ' ').trim();
  const dialog = () => document.querySelector<HTMLElement>('[role="dialog"]')!;

  function setup() {
    const groupsStub = { createRoot: vi.fn(() => of(GROUP)) };
    const toastStub = { show: vi.fn() };
    TestBed.configureTestingModule({
      providers: [
        { provide: GroupsService, useValue: groupsStub },
        { provide: GbtToastService, useValue: toastStub },
      ],
    });
    const fixture = TestBed.createComponent(CreateGroupModal);
    const created = vi.fn();
    const closed = vi.fn();
    fixture.componentInstance.created.subscribe(created);
    fixture.componentInstance.close.subscribe(closed);
    fixture.detectChanges();
    return { fixture, component: fixture.componentInstance, groupsStub, toastStub, created, closed };
  }

  afterEach(() => {
    TestBed.resetTestingModule();
  });

  it('opens a "Nouveau groupe" dialog with a name, a description and a create button', () => {
    setup();

    expect(text(dialog().querySelector('h2'))).toBe('Nouveau groupe');
    expect(text(dialog().querySelector('gbt-input label'))).toContain('Nom du groupe');
    expect(text(dialog().querySelector('gbt-textarea label'))).toBe('Description');
    expect(text(dialog().querySelector('button[type="submit"]'))).toBe('Créer le groupe');
  });

  it('refuses an empty name without calling the server', () => {
    const { component, fixture, groupsStub } = setup();
    component.name.set('   ');

    component.submit();
    fixture.detectChanges();

    expect(groupsStub.createRoot).not.toHaveBeenCalled();
    expect(text(dialog().querySelector('.gbt-input__error'))).toBe('Le nom est requis');
  });

  it('creates the root group with the trimmed name and description, then tells the parent', () => {
    const { component, groupsStub, toastStub, created } = setup();
    component.name.set('  acme  ');
    component.description.set('  Notre équipe  ');

    component.submit();

    expect(groupsStub.createRoot).toHaveBeenCalledWith('acme', 'Notre équipe');
    expect(toastStub.show).toHaveBeenCalledWith('Groupe créé.');
    expect(created).toHaveBeenCalledTimes(1);
  });

  it('ignores a second submit while the creation is running', () => {
    const { component, groupsStub } = setup();
    groupsStub.createRoot.mockReturnValue(new Subject<Group>() as never);
    component.name.set('acme');

    component.submit();
    component.submit();

    expect(groupsStub.createRoot).toHaveBeenCalledTimes(1);
    expect(component.creating()).toBe(true);
  });

  it.each([
    [400, 'Nom invalide : lettres, chiffres, - et _ uniquement'],
    [409, 'Ce nom est déjà utilisé'],
    [500, 'Impossible de créer le groupe. Réessayez plus tard.'],
  ])('keeps the dialog and the draft open with a message when the server answers %i', (status, message) => {
    const { component, fixture, groupsStub, created } = setup();
    groupsStub.createRoot.mockReturnValue(throwError(() => ({ status })));
    component.name.set('acme');

    component.submit();
    fixture.detectChanges();

    expect(component.error()).toBe(message);
    expect(text(dialog().querySelector('gbt-alert'))).toContain(message);
    expect(component.name()).toBe('acme');
    expect(component.creating()).toBe(false);
    expect(created).not.toHaveBeenCalled();
  });

  it('asks the parent to close it on cancel', () => {
    const { closed } = setup();

    Array.from(dialog().querySelectorAll<HTMLButtonElement>('button')).find((b) => text(b) === 'Annuler')!.click();

    expect(closed).toHaveBeenCalledTimes(1);
  });
});
