import { TestBed } from '@angular/core/testing';
import { of, Subject, throwError } from 'rxjs';
import { RepositoryLabelsSettings } from './repository-labels-settings';
import { Label, LabelsService } from '../../labels/labels.service';
import { GbtToastService } from '@masmarino/gabarit';

describe('RepositoryLabelsSettings', () => {
  function setup() {
    const labelsServiceStub = {
      listForRepository: vi.fn(() => of<Label[]>([])),
      create: vi.fn(() => of<Label>({ id: 'l1', name: 'bug', color: '#dc2626', repositoryId: 'repo-1', groupId: null, createdAt: '2026-01-01T00:00:00Z' })),
      delete: vi.fn(() => of<void>(undefined)),
    };
    const toastStub = { show: vi.fn() };
    TestBed.configureTestingModule({
      providers: [
        { provide: LabelsService, useValue: labelsServiceStub },
        { provide: GbtToastService, useValue: toastStub },
      ],
    });
    const fixture = TestBed.createComponent(RepositoryLabelsSettings);
    fixture.componentRef.setInput('repositoryId', 'repo-1');
    return { fixture, component: fixture.componentInstance, labelsServiceStub, toastStub };
  }

  it('lists configured labels', () => {
    const { fixture, labelsServiceStub } = setup();
    labelsServiceStub.listForRepository.mockReturnValue(
      of([{ id: 'l1', name: 'bug', color: '#dc2626', repositoryId: 'repo-1', groupId: null, createdAt: '2026-01-01T00:00:00Z' }]),
    );
    fixture.detectChanges();

    expect(fixture.nativeElement.textContent).toContain('bug');
  });

  it('surfaces an error when loading labels fails', () => {
    const { fixture, labelsServiceStub, toastStub } = setup();
    labelsServiceStub.listForRepository.mockReturnValue(throwError(() => ({ status: 500 })));
    fixture.detectChanges();

    expect(toastStub.show).toHaveBeenCalledWith('Impossible de charger les réglages. Réessayez plus tard.', 'error');
  });

  it('adds a label with the selected color and clears the form', () => {
    const { component, labelsServiceStub } = setup();

    component['newLabelName'].set('bug');
    component['newLabelColor'].set('#16a34a');
    component.addLabel();

    expect(labelsServiceStub.create).toHaveBeenCalledWith({ repositoryId: 'repo-1' }, 'bug', '#16a34a');
    expect(component['newLabelName']()).toBe('');
    expect(labelsServiceStub.listForRepository).toHaveBeenCalledTimes(1);
  });

  it('surfaces an error and keeps the form filled in when adding a label fails', () => {
    const { component, fixture, labelsServiceStub, toastStub } = setup();
    labelsServiceStub.create.mockReturnValue(throwError(() => ({ status: 400 })));

    component['newLabelName'].set('bug');
    component.addLabel();
    fixture.detectChanges();

    expect(component['newLabelName']()).toBe('bug');
    expect(toastStub.show).toHaveBeenCalledWith('Impossible de créer ce label.', 'error');
  });

  it('deletes a label after confirming in the modal, then closes it', () => {
    const { component, fixture, labelsServiceStub } = setup();
    const label = { id: 'l1', name: 'bug', color: '#dc2626', repositoryId: 'repo-1', groupId: null, createdAt: '2026-01-01T00:00:00Z' };
    labelsServiceStub.listForRepository.mockReturnValue(of([label]));
    fixture.detectChanges();

    const removeButton = Array.from((fixture.nativeElement as HTMLElement).querySelectorAll<HTMLButtonElement>('button')).find(
      (el) => el.getAttribute('aria-label') === 'Supprimer le label bug' && el.closest('li')?.textContent?.includes('bug'),
    ) as HTMLButtonElement;
    removeButton.click();
    fixture.detectChanges();

    expect(component['labelPendingDelete']()).toEqual(label);
    // French terms only: "tickets" and "demandes de fusion", not "issues" or "merge requests".
    expect((fixture.nativeElement as HTMLElement).querySelector('[role="dialog"]')?.textContent).toContain(
      "Ce label sera retiré de tous les tickets et de toutes les demandes de fusion qui l'utilisent. Cette action est irréversible.",
    );

    // Skips `ConfirmDangerModal`'s typed confirmation, which has its own tests.
    component.deleteLabel();
    fixture.detectChanges();

    expect(labelsServiceStub.delete).toHaveBeenCalledWith('l1');
    expect(component['labelPendingDelete']()).toBeNull();
  });

  it('surfaces an error and closes the modal when deleting a label fails', () => {
    const { component, fixture, labelsServiceStub, toastStub } = setup();
    const label = { id: 'l1', name: 'bug', color: '#dc2626', repositoryId: 'repo-1', groupId: null, createdAt: '2026-01-01T00:00:00Z' };
    labelsServiceStub.delete.mockReturnValue(throwError(() => ({ status: 500 })));

    component.confirmDeleteLabel(label);
    component.deleteLabel();
    fixture.detectChanges();

    expect(component['labelPendingDelete']()).toBeNull();
    expect(toastStub.show).toHaveBeenCalledWith('Impossible de supprimer ce label.', 'error');
  });

  describe('layout', () => {
    const LABELS: Label[] = [
      { id: 'l1', name: 'bug', color: '#dc2626', repositoryId: 'repo-1', groupId: null, createdAt: '2026-01-01T00:00:00Z' },
      { id: 'l2', name: 'documentation', color: '#16a34a', repositoryId: 'repo-1', groupId: null, createdAt: '2026-01-02T00:00:00Z' },
    ];
    const text = (el: Element | null | undefined) => el?.textContent?.replace(/\s+/g, ' ').trim();
    const button = (root: Element, label: string) =>
      Array.from(root.querySelectorAll<HTMLButtonElement>('button')).find((b) => b.textContent?.trim() === label || b.getAttribute('aria-label') === label);

    function loaded(labels: Label[] = LABELS) {
      const ctx = setup();
      ctx.labelsServiceStub.listForRepository.mockReturnValue(of(labels));
      ctx.fixture.detectChanges();
      return { ...ctx, el: ctx.fixture.nativeElement as HTMLElement };
    }

    it('has a creation card and a list card with the number of labels', () => {
      const { el } = loaded();

      expect(Array.from(el.querySelectorAll('gbt-card h2')).map(text)).toEqual(['Nouveau label', 'Labels 2']);
    });

    it('picks the colour among named swatches (a radio group), the chosen one checked', () => {
      const { component, fixture, el } = loaded();

      const fieldset = el.querySelector('fieldset.repository-labels-settings__palette')!;
      expect(text(fieldset.querySelector('legend'))).toBe('Couleur');
      const radios = Array.from(fieldset.querySelectorAll<HTMLInputElement>('input[type="radio"]'));
      expect(radios).toHaveLength(12);
      expect(new Set(radios.map((radio) => radio.name)).size).toBe(1);
      expect(radios.map((radio) => radio.getAttribute('aria-label'))).toContain('Vert');
      expect(radios.filter((radio) => radio.checked).map((radio) => radio.getAttribute('aria-label'))).toEqual(['Rouge']);

      radios.find((radio) => radio.getAttribute('aria-label') === 'Vert')!.click();
      fixture.detectChanges();
      expect(component['newLabelColor']()).toBe('#16a34a');
      expect(radios.filter((radio) => radio.checked).map((radio) => radio.getAttribute('aria-label'))).toEqual(['Vert']);
    });

    it('previews the label as it will look', () => {
      const { component, fixture, el } = loaded();
      const preview = () => el.querySelector('.repository-labels-settings__preview gbt-tag');

      expect(text(preview())).toBe('Aperçu');
      component['newLabelName'].set('à trier');
      component['newLabelColor'].set('#2563eb');
      fixture.detectChanges();
      expect(text(preview())).toBe('à trier');
    });

    it('adds the label when the form is submitted (Enter in the field)', () => {
      const { component, el, labelsServiceStub } = loaded();
      component['newLabelName'].set('urgent');

      el.querySelector('form')!.dispatchEvent(new Event('submit', { cancelable: true }));

      expect(labelsServiceStub.create).toHaveBeenCalledWith({ repositoryId: 'repo-1' }, 'urgent', '#dc2626');
    });

    it('lists each label as a row: the coloured tag and a delete action', () => {
      const { el } = loaded();

      const rows = Array.from(el.querySelectorAll('ul > li > gbt-list-row'));
      expect(rows.map((row) => text(row.querySelector('gbt-tag')))).toEqual(['bug', 'documentation']);
      expect(button(rows[1], 'Supprimer le label documentation')).toBeDefined();
    });

    it('shows skeleton rows while loading, then explains the empty list', () => {
      const { fixture, labelsServiceStub } = setup();
      const pending = new Subject<Label[]>();
      labelsServiceStub.listForRepository.mockReturnValue(pending);
      fixture.detectChanges();
      const el = fixture.nativeElement as HTMLElement;
      expect(el.querySelector('[aria-busy="true"]')).not.toBeNull();

      pending.next([]);
      fixture.detectChanges();
      expect(el.querySelector('[aria-busy="true"]')).toBeNull();
      expect(text(el.querySelector('gbt-empty-state'))).toContain("Aucun label pour l'instant");
    });

    it('shows a failed state with a retry button when the list cannot be loaded', () => {
      const { fixture, labelsServiceStub } = setup();
      labelsServiceStub.listForRepository.mockReturnValue(throwError(() => ({ status: 500 })));
      fixture.detectChanges();
      const el = fixture.nativeElement as HTMLElement;
      const failed = el.querySelector('gbt-alert .gbt-alert');
      expect(text(failed)).toContain("Les labels n'ont pas pu être chargés");
      expect(failed?.getAttribute('data-variant')).toBe('error');
      // The error toast announces the failure, so the inline block stays silent (one live region, not two).
      expect(failed?.getAttribute('role')).toBeNull();
      expect(failed?.getAttribute('aria-live')).toBeNull();
      expect(failed?.querySelector('button')?.textContent?.trim()).toBe('Réessayer');
      expect(el.querySelectorAll('[role="alert"], [aria-live]')).toHaveLength(0);

      labelsServiceStub.listForRepository.mockReturnValue(of(LABELS));
      button(el, 'Réessayer')!.click();
      fixture.detectChanges();
      expect(el.querySelectorAll('gbt-list-row')).toHaveLength(2);
    });
  });
});
