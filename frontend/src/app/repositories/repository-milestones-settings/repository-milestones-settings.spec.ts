import { of, Subject, throwError } from 'rxjs';
import { RepositoryMilestonesSettings } from './repository-milestones-settings';
import { Milestone, MilestonesService } from '../../milestones/milestones.service';
import { createSettingsSection } from '../settings-section-testing';

describe('RepositoryMilestonesSettings', () => {
  function setup() {
    const milestonesServiceStub = {
      listForRepository: vi.fn(() => of<Milestone[]>([])),
      create: vi.fn(() =>
        of<Milestone>({
          id: 'm1',
          title: 'v1.0',
          description: '',
          dueDate: null,
          state: 'open',
          repositoryId: 'repo-1',
          groupId: null,
          createdAt: '2026-01-01T00:00:00Z',
        }),
      ),
      delete: vi.fn(() => of<void>(undefined)),
    };
    return { ...createSettingsSection(RepositoryMilestonesSettings, [{ provide: MilestonesService, useValue: milestonesServiceStub }]), milestonesServiceStub };
  }

  it('lists configured milestones', () => {
    const { fixture, milestonesServiceStub } = setup();
    milestonesServiceStub.listForRepository.mockReturnValue(
      of([
        {
          id: 'm1',
          title: 'v1.0',
          description: '',
          dueDate: '2026-03-01T00:00:00.000Z',
          state: 'open',
          repositoryId: 'repo-1',
          groupId: null,
          createdAt: '2026-01-01T00:00:00Z',
        },
      ]),
    );
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('v1.0');
    // The raw ISO instant should not reach the page. It is formatted in French, in UTC, so the stored day does not shift west of UTC.
    expect(fixture.nativeElement.textContent).toContain('1 mars 2026');
    expect(fixture.nativeElement.textContent).not.toContain('2026-03-01T00:00:00.000Z');
  });

  it('surfaces an error when loading milestones fails', () => {
    const { fixture, milestonesServiceStub, toastStub } = setup();
    milestonesServiceStub.listForRepository.mockReturnValue(throwError(() => ({ status: 500 })));
    fixture.detectChanges();

    expect(toastStub.show).toHaveBeenCalledWith('Impossible de charger les réglages. Réessayez plus tard.', 'error');
  });

  it('adds a milestone with the given title and due date, and clears the form', () => {
    const { component, milestonesServiceStub } = setup();

    component['newMilestoneTitle'].set('v1.0');
    component['newMilestoneDueDate'].set(new Date(2026, 2, 1));
    component.addMilestone();

    // `gbt-date-picker` gives a local-midnight `Date`, but the API's `due_date` only accepts RFC 3339 (anything else is a 422), so it is turned into a UTC instant.
    expect(milestonesServiceStub.create).toHaveBeenCalledWith({ repositoryId: 'repo-1' }, 'v1.0', '', '2026-03-01T00:00:00.000Z');
    expect(component['newMilestoneTitle']()).toBe('');
    expect(component['newMilestoneDueDate']()).toBeNull();
  });

  it('converts a picked calendar day to RFC 3339 without shifting the day across a timezone', () => {
    const { component } = setup();

    // Rebuilt at UTC midnight from the same Y/M/D, because `.toISOString()` on a local-midnight `Date` moves the day back in time zones ahead of UTC.
    expect(component['dueDateForApi'](new Date(2026, 2, 1))).toBe('2026-03-01T00:00:00.000Z');
    expect(component['dueDateForApi'](new Date(2026, 11, 31))).toBe('2026-12-31T00:00:00.000Z');
    expect(component['dueDateForApi'](null)).toBeNull();
  });

  it('adds a milestone with a null due date when none is given', () => {
    const { component, milestonesServiceStub } = setup();

    component['newMilestoneTitle'].set('v1.0');
    component.addMilestone();

    expect(milestonesServiceStub.create).toHaveBeenCalledWith({ repositoryId: 'repo-1' }, 'v1.0', '', null);
  });

  it('surfaces an error and keeps the form filled in when adding a milestone fails', () => {
    const { component, fixture, milestonesServiceStub, toastStub } = setup();
    milestonesServiceStub.create.mockReturnValue(throwError(() => ({ status: 400 })));

    component['newMilestoneTitle'].set('v1.0');
    component.addMilestone();
    fixture.detectChanges();

    expect(component['newMilestoneTitle']()).toBe('v1.0');
    expect(toastStub.show).toHaveBeenCalledWith('Impossible de créer ce milestone.', 'error');
  });

  it("deletes a milestone after confirming in the modal, then closes it", () => {
    const { component, fixture, milestonesServiceStub } = setup();
    const milestone = {
      id: 'm1',
      title: 'v1.0',
      description: '',
      dueDate: null,
      state: 'open' as const,
      repositoryId: 'repo-1',
      groupId: null,
      createdAt: '2026-01-01T00:00:00Z',
    };
    milestonesServiceStub.listForRepository.mockReturnValue(of([milestone]));
    fixture.detectChanges();

    const removeButton = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll<HTMLButtonElement>('button')).find(
      (el) => el.getAttribute('aria-label') === 'Supprimer le milestone v1.0' && el.closest('li')?.textContent?.includes('v1.0'),
    ) as HTMLButtonElement;
    removeButton.click();
    fixture.detectChanges();

    expect(component['milestonePendingDelete']()).toEqual(milestone);
    // French terms only: "tickets" and "demandes de fusion", not "issues" or "merge requests".
    expect((fixture.nativeElement as HTMLElement).querySelector('[role="dialog"]')?.textContent).toContain(
      "Ce milestone sera retiré de tous les tickets et de toutes les demandes de fusion qui l'utilisent. Cette action est irréversible.",
    );

    // Skips `ConfirmDangerModal`'s typed confirmation, which has its own tests.
    component.deleteMilestone();
    fixture.detectChanges();

    expect(milestonesServiceStub.delete).toHaveBeenCalledWith('m1');
    expect(component['milestonePendingDelete']()).toBeNull();
  });

  it('surfaces an error and closes the modal when deleting a milestone fails', () => {
    const { component, fixture, milestonesServiceStub, toastStub } = setup();
    const milestone = {
      id: 'm1',
      title: 'v1.0',
      description: '',
      dueDate: null,
      state: 'open' as const,
      repositoryId: 'repo-1',
      groupId: null,
      createdAt: '2026-01-01T00:00:00Z',
    };
    milestonesServiceStub.delete.mockReturnValue(throwError(() => ({ status: 500 })));

    component.confirmDeleteMilestone(milestone);
    component.deleteMilestone();
    fixture.detectChanges();

    expect(component['milestonePendingDelete']()).toBeNull();
    expect(toastStub.show).toHaveBeenCalledWith('Impossible de supprimer ce milestone.', 'error');
  });

  describe('layout', () => {
    const milestone = (fields: Partial<Milestone> & Pick<Milestone, 'id' | 'title'>): Milestone => ({
      description: '',
      dueDate: null,
      state: 'open',
      repositoryId: 'repo-1',
      groupId: null,
      createdAt: '2026-01-01T00:00:00Z',
      ...fields,
    });
    const MILESTONES: Milestone[] = [
      milestone({ id: 'm1', title: 'v1.0', dueDate: '2026-03-01T00:00:00Z', state: 'closed' }),
      milestone({ id: 'm2', title: 'v1.1', dueDate: '2999-10-15T00:00:00Z' }),
      milestone({ id: 'm3', title: 'v1.0.1', dueDate: '2020-06-30T00:00:00Z' }),
      milestone({ id: 'm4', title: 'Backlog' }),
    ];
    const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();
    const button = (root: Element, label: string) =>
      Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find((b) => b.textContent?.trim() === label || b.getAttribute('aria-label') === label);

    function loaded(milestones: Milestone[] = MILESTONES) {
      const ctx = setup();
      ctx.milestonesServiceStub.listForRepository.mockReturnValue(of(milestones));
      ctx.fixture.detectChanges();
      return { ...ctx, el: ctx.fixture.nativeElement as HTMLElement };
    }

    it('has a creation card (title, a French date picker) and a list card with the number of milestones', () => {
      const { el } = loaded();

      expect(Array.from(el.querySelectorAll('gbt-card h2')).map(text)).toEqual(['Nouveau milestone', 'Milestones 4']);
      const form = el.querySelector('form')!;
      expect(text(form.querySelector('gbt-input label'))).toBe('Titre');
      expect(text(form.querySelector('gbt-date-picker label'))).toBe("Date d'échéance (facultative)");
      expect(text(form.querySelector('gbt-date-picker'))).not.toContain('Select');
    });

    it('adds the milestone when the form is submitted (Enter in the field)', () => {
      const { component, el, milestonesServiceStub } = loaded();
      component['newMilestoneTitle'].set('v2.0');

      el.querySelector('form')!.dispatchEvent(new Event('submit', { cancelable: true }));

      expect(milestonesServiceStub.create).toHaveBeenCalledWith({ repositoryId: 'repo-1' }, 'v2.0', '', null);
    });

    it('lists each milestone as a row: title, state in words, due date (or none), late when an open one is past due', () => {
      const { el } = loaded();

      const rows = Array.from(el.querySelectorAll('ul > li > gbt-list-row'));
      expect(rows.map((row) => text(row.querySelector('.repository-milestones-settings__title')))).toEqual(['v1.0', 'v1.1', 'v1.0.1', 'Backlog']);
      expect(rows.map((row) => text(row.querySelector('.repository-milestones-settings__state')))).toEqual(['Fermé', 'Ouvert', 'Ouvert', 'Ouvert']);
      expect(rows.map((row) => row.querySelector('.gbt-list-row__leading')?.getAttribute('data-tone'))).toEqual(['success', 'info', 'info', 'info']);
      expect(text(rows[0].querySelector('[row-meta]'))).toBe('Échéance le 1 mars 2026');
      expect(text(rows[3].querySelector('[row-meta]'))).toBe('Sans échéance');
      expect(rows.map((row) => !!row.querySelector('.repository-milestones-settings__late'))).toEqual([false, false, true, false]);
      expect(text(rows[2].querySelector('.repository-milestones-settings__late'))).toBe('En retard');
    });

    it('shows skeleton rows while loading, then explains the empty list', () => {
      const { fixture, milestonesServiceStub } = setup();
      const pending = new Subject<Milestone[]>();
      milestonesServiceStub.listForRepository.mockReturnValue(pending);
      fixture.detectChanges();
      const el = fixture.nativeElement as HTMLElement;
      expect(el.querySelectorAll('gbt-skeleton-list .gbt-skeleton-list__row')).toHaveLength(3);
      // One polite status, outside any aria-busy region (a busy ancestor can hold the announcement back).
      expect(text(el.querySelector('gbt-skeleton-list [role="status"]'))).toBe('Chargement des milestones…');
      expect(el.querySelector('[aria-busy="true"]')).toBeNull();

      pending.next([]);
      fixture.detectChanges();
      expect(el.querySelector('gbt-skeleton-list')).toBeNull();
      expect(text(el.querySelector('gbt-empty-state'))).toContain("Aucun milestone pour l'instant");
    });

    it('shows a failed state with a retry button when the list cannot be loaded', () => {
      const { fixture, milestonesServiceStub } = setup();
      milestonesServiceStub.listForRepository.mockReturnValue(throwError(() => ({ status: 500 })));
      fixture.detectChanges();
      const el = fixture.nativeElement as HTMLElement;
      const failed = el.querySelector('gbt-alert .gbt-alert');
      expect(text(failed)).toContain("Les milestones n'ont pas pu être chargés");
      expect(failed?.getAttribute('data-variant')).toBe('error');
      // The error toast announces the failure, so the inline block stays silent (one live region, not two).
      expect(failed?.getAttribute('role')).toBeNull();
      expect(failed?.getAttribute('aria-live')).toBeNull();
      expect(failed?.querySelector('button')?.textContent?.trim()).toBe('Réessayer');
      expect(el.querySelectorAll('[role="alert"], [aria-live]')).toHaveLength(0);

      milestonesServiceStub.listForRepository.mockReturnValue(of(MILESTONES));
      button(el, 'Réessayer')!.click();
      fixture.detectChanges();
      expect(el.querySelectorAll('gbt-list-row')).toHaveLength(4);
    });
  });
});
