import { LOCALE_ID } from '@angular/core';
import { of, Subject, throwError } from 'rxjs';
import { RepositoryCollaboratorsSettings } from './repository-collaborators-settings';
import { CollaboratorSummary, RepositorySettingsService } from '../repository-settings.service';
import { createSettingsSection } from '../settings-section-testing';

describe('RepositoryCollaboratorsSettings', () => {
  function setup() {
    const repositorySettingsStub = {
      listCollaborators: vi.fn(() => of<CollaboratorSummary[]>([])),
      addCollaborator: vi.fn(() => of<void>(undefined)),
      setCollaboratorRole: vi.fn(() => of<void>(undefined)),
      removeCollaborator: vi.fn(() => of<void>(undefined)),
    };
    const providers = [
      { provide: RepositorySettingsService, useValue: repositorySettingsStub },
      { provide: LOCALE_ID, useValue: 'fr' },
    ];
    return { ...createSettingsSection(RepositoryCollaboratorsSettings, providers), repositorySettingsStub };
  }

  it('lists configured collaborators', () => {
    const { fixture, repositorySettingsStub } = setup();
    repositorySettingsStub.listCollaborators.mockReturnValue(
      of([{ userId: 'u1', username: 'alice', role: 'contributor', createdAt: '2026-01-01T00:00:00Z' }]),
    );
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('alice');
  });

  it('surfaces an error when loading collaborators fails', () => {
    const { fixture, repositorySettingsStub, toastStub } = setup();
    repositorySettingsStub.listCollaborators.mockReturnValue(throwError(() => ({ status: 500 })));
    fixture.detectChanges();

    expect(toastStub.show).toHaveBeenCalledWith('Impossible de charger les réglages. Réessayez plus tard.', 'error');
  });

  it('does not add a collaborator with a blank username', () => {
    const { component, repositorySettingsStub } = setup();

    component['newCollaboratorUsername'].set('   ');
    component.addCollaborator();

    expect(repositorySettingsStub.addCollaborator).not.toHaveBeenCalled();
  });

  it('adds a collaborator with the chosen role and clears the form', () => {
    const { component, repositorySettingsStub } = setup();

    component['newCollaboratorUsername'].set('bob');
    component['newCollaboratorRole'].set('maintainer');
    component.addCollaborator();

    expect(repositorySettingsStub.addCollaborator).toHaveBeenCalledWith('repo-1', 'bob', 'maintainer');
    expect(component['newCollaboratorUsername']()).toBe('');
    // The section's list was re-fetched once (`ngOnInit` never ran here).
    expect(repositorySettingsStub.listCollaborators).toHaveBeenCalledTimes(1);
  });

  it('surfaces an error and keeps the form filled in when adding a collaborator fails', () => {
    const { component, fixture, repositorySettingsStub, toastStub } = setup();
    repositorySettingsStub.addCollaborator.mockReturnValue(throwError(() => ({ status: 404 })));

    component['newCollaboratorUsername'].set('bob');
    component.addCollaborator();
    fixture.detectChanges();

    expect(component['newCollaboratorUsername']()).toBe('bob');
    expect(toastStub.show).toHaveBeenCalledWith(
      "Impossible d'ajouter ce collaborateur (nom d'utilisateur inconnu, ou vous n'êtes pas le propriétaire de ce dépôt).",
      'error',
    );
  });

  describe('layout', () => {
    const COLLABORATORS: CollaboratorSummary[] = [
      { userId: 'u1', username: 'alice', role: 'maintainer', createdAt: '2026-01-05T09:00:00Z' },
      { userId: 'u2', username: 'bob', role: 'reader', createdAt: '2026-02-12T14:30:00Z' },
    ];
    const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();
    const button = (root: Element, label: string) =>
      Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find((b) => b.textContent?.trim() === label || b.getAttribute('aria-label') === label);

    function loaded(collaborators: CollaboratorSummary[] = COLLABORATORS) {
      const ctx = setup();
      ctx.repositorySettingsStub.listCollaborators.mockReturnValue(of(collaborators));
      ctx.fixture.detectChanges();
      return { ...ctx, el: ctx.fixture.nativeElement as HTMLElement };
    }

    it('has a creation card and a list card with the number of collaborators', () => {
      const { el } = loaded();

      expect(Array.from(el.querySelectorAll('gbt-card h2')).map(text)).toEqual(['Ajouter un collaborateur', 'Collaborateurs 2']);
      const form = el.querySelector('form')!;
      expect(text(form.querySelector('gbt-input label'))).toBe("Nom d'utilisateur");
      expect(text(form.querySelector('gbt-select .gbt-select__label'))).toBe('Rôle');
    });

    it('adds the collaborator when the form is submitted (Enter in the field)', () => {
      const { component, el, repositorySettingsStub } = loaded();
      component['newCollaboratorUsername'].set('carol');

      el.querySelector('form')!.dispatchEvent(new Event('submit', { cancelable: true }));

      expect(repositorySettingsStub.addCollaborator).toHaveBeenCalledWith('repo-1', 'carol', 'contributor');
    });

    it('lists each collaborator as a row: avatar and name, when they were added, their role (a labelled select) and a remove action', async () => {
      const { el, fixture } = loaded();
      await fixture.whenStable();
      fixture.detectChanges();

      const rows = Array.from(el.querySelectorAll('ul > li > gbt-list-row'));
      expect(rows.map((row) => row.querySelector('gbt-user-chip .gbt-user-chip__name')?.textContent?.trim())).toEqual(['alice', 'bob']);
      expect(text(rows[0].querySelector('[row-meta]'))).toContain('ajouté');
      expect(text(rows[0].querySelector('gbt-select .gbt-select__label'))).toBe('Rôle de alice');
      // The row already says who: the label names the select for assistive technology only.
      expect(rows[0].querySelector('gbt-select .gbt-select__label')!.classList).toContain('gbt-select__label--hidden');
      expect(text(rows[0].querySelector('gbt-select .gbt-select__trigger'))).toContain('Mainteneur');
      expect(button(rows[1], 'Retirer bob')).toBeDefined();
    });

    it('gives each row a quiet icon-only remove button named after the member (red on hover only)', async () => {
      const { el, fixture } = loaded();
      await fixture.whenStable();
      fixture.detectChanges();

      const remove = button(el.querySelectorAll('ul > li > gbt-list-row')[1], 'Retirer bob')!;
      expect(remove.getAttribute('aria-label')).toBe('Retirer bob');
      expect(remove.classList).toContain('gbt-button--icon-only');
      expect(remove.classList).toContain('gbt-button--ghost-danger');
      expect(remove.textContent?.trim()).toBe('');
    });

    const dialog = (el: HTMLElement) => el.querySelector<HTMLElement>('gbt-confirm-danger-modal [role="dialog"]');

    it('confirms before removing, without making the user retype the name: confirming removes that collaborator and drops their row', () => {
      const { fixture, el, repositorySettingsStub, toastStub } = loaded();

      button(el.querySelectorAll('gbt-list-row')[1], 'Retirer bob')!.click();
      fixture.detectChanges();

      const modal = dialog(el)!;
      expect(modal.getAttribute('aria-label')).toBe('Retirer le collaborateur');
      expect(text(modal)).toContain('bob');
      expect(modal.querySelector('input')).toBeNull();
      expect(button(modal, 'Retirer')!.disabled).toBe(false);
      expect(repositorySettingsStub.removeCollaborator).not.toHaveBeenCalled();

      repositorySettingsStub.listCollaborators.mockReturnValue(of([COLLABORATORS[0]]));
      button(modal, 'Retirer')!.click();
      fixture.detectChanges();

      expect(repositorySettingsStub.removeCollaborator).toHaveBeenCalledExactlyOnceWith('repo-1', 'bob');
      expect(dialog(el)).toBeNull();
      expect(Array.from(el.querySelectorAll('gbt-list-row .gbt-user-chip__name')).map(text)).toEqual(['alice']);
      expect(toastStub.show).toHaveBeenCalledWith('Collaborateur retiré.');
    });

    it('keeps the confirmation open and busy while the removal runs: a second click sends nothing', () => {
      const { fixture, el, repositorySettingsStub } = loaded();
      const pending = new Subject<void>();
      repositorySettingsStub.removeCollaborator.mockReturnValue(pending);

      button(el.querySelectorAll('gbt-list-row')[0], 'Retirer alice')!.click();
      fixture.detectChanges();
      button(dialog(el)!, 'Retirer')!.click();
      fixture.detectChanges();

      const confirm = dialog(el)!.querySelector<HTMLButtonElement>('.gbt-button--danger')!;
      expect(confirm.getAttribute('aria-busy')).toBe('true');
      expect(text(confirm.querySelector('.sr-only'))).toBe('Retrait en cours');
      confirm.click();
      button(dialog(el)!, 'Annuler')!.click();
      fixture.detectChanges();
      expect(repositorySettingsStub.removeCollaborator).toHaveBeenCalledTimes(1);
      expect(dialog(el)).not.toBeNull();

      pending.next();
      pending.complete();
      fixture.detectChanges();
      expect(dialog(el)).toBeNull();
    });

    it('closes the confirmation and says so when removing fails, keeping the row', () => {
      const { fixture, el, repositorySettingsStub, toastStub } = loaded();
      repositorySettingsStub.removeCollaborator.mockReturnValue(throwError(() => ({ status: 403 })));

      button(el.querySelectorAll('gbt-list-row')[0], 'Retirer alice')!.click();
      fixture.detectChanges();
      button(dialog(el)!, 'Retirer')!.click();
      fixture.detectChanges();

      expect(repositorySettingsStub.removeCollaborator).toHaveBeenCalledWith('repo-1', 'alice');
      expect(dialog(el)).toBeNull();
      expect(toastStub.show).toHaveBeenCalledWith("Impossible de retirer ce collaborateur (vous n'êtes peut-être pas le propriétaire de ce dépôt).", 'error');
      expect(el.querySelectorAll('gbt-list-row')).toHaveLength(2);
    });

    describe('changing a role in place', () => {
      const trigger = (row: Element) => row.querySelector<HTMLButtonElement>('gbt-select .gbt-select__trigger')!;
      async function settle(fixture: { detectChanges(): void; whenStable(): Promise<unknown> }) {
        fixture.detectChanges();
        await fixture.whenStable();
        fixture.detectChanges();
      }
      async function pick(fixture: { detectChanges(): void; whenStable(): Promise<unknown> }, row: Element, label: string) {
        trigger(row).click();
        fixture.detectChanges();
        Array.from(row.querySelectorAll<HTMLButtonElement>('[role="option"]'))
          .find((option) => text(option) === label)!
          .click();
        await settle(fixture);
      }

      it('saves the picked role and keeps showing it, without reloading the list', async () => {
        const { fixture, el, repositorySettingsStub, toastStub } = loaded();
        await settle(fixture);
        const bob = el.querySelectorAll('gbt-list-row')[1];

        await pick(fixture, bob, 'Mainteneur');

        expect(repositorySettingsStub.setCollaboratorRole).toHaveBeenCalledExactlyOnceWith('repo-1', 'bob', 'maintainer');
        expect(text(trigger(bob))).toContain('Mainteneur');
        expect(toastStub.show).toHaveBeenCalledWith('Rôle mis à jour.');
        expect(repositorySettingsStub.listCollaborators).toHaveBeenCalledTimes(1);
      });

      it('shows the previous role again when the change is refused, and the next pick sends the right role', async () => {
        const { fixture, el, repositorySettingsStub, toastStub } = loaded();
        await settle(fixture);
        const bob = el.querySelectorAll('gbt-list-row')[1];
        const refused = new Subject<void>();
        repositorySettingsStub.setCollaboratorRole.mockReturnValue(refused);

        await pick(fixture, bob, 'Mainteneur');
        expect(text(trigger(bob))).toContain('Mainteneur');

        refused.error({ status: 403 });
        await settle(fixture);

        expect(toastStub.show).toHaveBeenCalledWith("Impossible de modifier le rôle de ce collaborateur (vous n'êtes peut-être pas mainteneur de ce dépôt).", 'error');
        expect(text(trigger(bob))).toContain('Lecteur');

        repositorySettingsStub.setCollaboratorRole.mockReturnValue(of(undefined));
        await pick(fixture, bob, 'Contributeur');
        expect(repositorySettingsStub.setCollaboratorRole).toHaveBeenLastCalledWith('repo-1', 'bob', 'contributor');
        expect(text(trigger(bob))).toContain('Contributeur');
      });
    });

    it('shows skeleton rows while loading, then explains the empty list', () => {
      const { fixture, repositorySettingsStub } = setup();
      const pending = new Subject<CollaboratorSummary[]>();
      repositorySettingsStub.listCollaborators.mockReturnValue(pending);
      fixture.detectChanges();
      const el = fixture.nativeElement as HTMLElement;
      expect(el.querySelectorAll('gbt-skeleton-list .gbt-skeleton-list__row')).toHaveLength(3);
      // One polite status, outside any aria-busy region (a busy ancestor can hold the announcement back).
      expect(text(el.querySelector('gbt-skeleton-list [role="status"]'))).toBe('Chargement des collaborateurs…');
      expect(el.querySelector('[aria-busy="true"]')).toBeNull();

      pending.next([]);
      fixture.detectChanges();
      expect(el.querySelector('gbt-skeleton-list')).toBeNull();
      expect(text(el.querySelector('gbt-empty-state'))).toContain("Aucun collaborateur pour l'instant");
    });

    it('shows a failed state with a retry button when the list cannot be loaded', () => {
      const { fixture, repositorySettingsStub } = setup();
      repositorySettingsStub.listCollaborators.mockReturnValue(throwError(() => ({ status: 500 })));
      fixture.detectChanges();
      const el = fixture.nativeElement as HTMLElement;
      const failed = el.querySelector('gbt-alert .gbt-alert');
      expect(text(failed)).toContain("Les collaborateurs n'ont pas pu être chargés");
      expect(failed?.getAttribute('data-variant')).toBe('error');
      // The error toast announces the failure, so the inline block stays silent (one live region, not two).
      expect(failed?.getAttribute('role')).toBeNull();
      expect(failed?.getAttribute('aria-live')).toBeNull();
      expect(failed?.querySelector('button')?.textContent?.trim()).toBe('Réessayer');
      expect(el.querySelectorAll('[role="alert"], [aria-live]')).toHaveLength(0);

      repositorySettingsStub.listCollaborators.mockReturnValue(of(COLLABORATORS));
      button(el, 'Réessayer')!.click();
      fixture.detectChanges();
      expect(el.querySelectorAll('gbt-list-row')).toHaveLength(2);
    });
  });
});
